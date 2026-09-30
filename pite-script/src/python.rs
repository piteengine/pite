use std::path::{Path, PathBuf};
use std::sync::Once;

use anyhow::{Context, Result};
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use crate::ScriptBackend;

#[pyclass(subclass, name = "Node")]
struct PyNode {
    #[pyo3(get)]
    node_path: String,
}

#[pymethods]
impl PyNode {
    #[new]
    fn new() -> Self {
        Self {
            node_path: String::new(),
        }
    }

    fn get_node(&self, _path: &str) -> PyResult<Py<PyAny>> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "get_node wires up with the runtime scene tree after M1b; direct calls only for now",
        ))
    }
}

#[pyclass(extends = PyNode, subclass, name = "Node2D")]
struct PyNode2D {
    #[pyo3(get, set)]
    position: (f64, f64),
}

#[pymethods]
impl PyNode2D {
    #[new]
    fn new() -> (Self, PyNode) {
        (Self { position: (0.0, 0.0) }, PyNode::new())
    }
}

#[pyclass(extends = PyNode2D, subclass, name = "Sprite2D")]
struct PySprite2D {
    #[pyo3(get, set)]
    texture: String,
}

#[pymethods]
impl PySprite2D {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: (0.0, 0.0) })
            .add_subclass(PySprite2D {
                texture: String::new(),
            })
    }
}

#[pyclass(extends = PyNode2D, subclass, name = "Camera2D")]
struct PyCamera2D;

#[pymethods]
impl PyCamera2D {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: (0.0, 0.0) })
            .add_subclass(PyCamera2D)
    }
}

#[pyclass(extends = PyNode, subclass, name = "Timer")]
struct PyTimer {
    #[pyo3(get, set)]
    wait_time: f64,
}

#[pymethods]
impl PyTimer {
    #[new]
    fn new() -> (Self, PyNode) {
        (Self { wait_time: 1.0 }, PyNode::new())
    }
}

#[pymodule]
fn pite(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyNode>()?;
    m.add_class::<PyNode2D>()?;
    m.add_class::<PySprite2D>()?;
    m.add_class::<PyCamera2D>()?;
    m.add_class::<PyTimer>()?;
    Ok(())
}

static INIT: Once = Once::new();

fn ensure_init() {
    INIT.call_once(|| {
        pyo3::append_to_inittab!(pite);
        Python::initialize();
    });
}

fn format_error(py: Python<'_>, err: PyErr, ctx: &str) -> String {
    let tb = err
        .traceback(py)
        .and_then(|t| t.format().ok())
        .unwrap_or_default();
    if tb.is_empty() {
        format!("{ctx}: {err}")
    } else {
        format!("{ctx}: {err}\n{tb}")
    }
}

pub struct Pyo3Backend {
    node_path: String,
    file: PathBuf,
    class: String,
    instance: Option<Py<PyAny>>,
    last_error: Option<String>,
}

impl Pyo3Backend {
    pub fn new(node_path: impl Into<String>) -> Self {
        ensure_init();
        Self {
            node_path: node_path.into(),
            file: PathBuf::new(),
            class: String::new(),
            instance: None,
            last_error: None,
        }
    }

    fn exec(&mut self) -> Result<()> {
        let src = std::fs::read_to_string(&self.file)
            .with_context(|| format!("cannot read script {}", self.file.display()))?;
        let dir = self
            .file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = self
            .file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "script".to_string());
        Python::attach(|py| {
            let sys = py.import("sys")?;
            let path = sys.getattr("path")?;
            path.call_method1("insert", (0, dir.to_string_lossy().as_ref()))?;
            let code = std::ffi::CString::new(src.clone())?;
            let filename = std::ffi::CString::new(format!("{stem}.py"))?;
            let modname = std::ffi::CString::new(stem.clone())?;
            let module = PyModule::from_code(py, &code, &filename, &modname)
                .with_context(|| format!("cannot exec script {}", self.file.display()))?;
            let cls = module.getattr(self.class.as_str()).with_context(|| {
                format!(
                    "script {} has no class {:?}",
                    self.file.display(),
                    self.class
                )
            })?;
            let instance = cls.call0().with_context(|| {
                format!(
                    "cannot instantiate {} in {}",
                    self.class,
                    self.file.display()
                )
            })?;
            self.instance = Some(instance.into());
            Ok(())
        })
    }

    fn call_opt(&mut self, method: &str, arg: Option<f64>) -> Result<()> {
        let ctx = format!(
            "{} ({}:{}.{})",
            self.node_path,
            self.file.display(),
            self.class,
            method
        );
        Python::attach(|py| {
            let Some(inst) = &self.instance else {
                return Ok(());
            };
            let bound = inst.bind(py);
            let callable = match bound.getattr(method) {
                Ok(attr) => attr,
                Err(_) => return Ok(()),
            };
            if callable.is_none() || !callable.is_callable() {
                return Ok(());
            }
            let result = match arg {
                Some(delta) => callable.call1((delta,)),
                None => callable.call0(),
            };
            if let Err(err) = result {
                let msg = format_error(py, err, &ctx);
                self.last_error = Some(msg.clone());
                anyhow::bail!("{msg}");
            }
            Ok(())
        })
    }
}

impl ScriptBackend for Pyo3Backend {
    fn load(&mut self, path: &str, class: &str) -> Result<()> {
        self.file = PathBuf::from(path);
        self.class = class.to_string();
        self.last_error = None;
        self.exec()
    }

    fn call_ready(&mut self) -> Result<()> {
        self.call_opt("_ready", None)
    }

    fn call_process(&mut self, delta: f64) -> Result<()> {
        self.call_opt("_process", Some(delta))
    }

    fn reload(&mut self) -> Result<()> {
        let pos = self.position();
        self.last_error = None;
        self.exec()?;
        if let Some((x, y)) = pos {
            self.set_position(x, y);
        }
        Ok(())
    }

    fn position(&self) -> Option<(f64, f64)> {
        Python::attach(|py| {
            self.instance
                .as_ref()?
                .bind(py)
                .getattr("position")
                .ok()?
                .extract()
                .ok()
        })
    }

    fn set_position(&mut self, x: f64, y: f64) {
        Python::attach(|py| {
            if let Some(inst) = &self.instance {
                let _ = inst.bind(py).setattr("position", (x, y));
            }
        });
    }

    fn last_error(&self) -> Option<String> {
        self.last_error.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(dir: &Path, name: &str, text: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn test_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-py-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const MOVER: &str = r#"
import pite

class Player(pite.Node2D):
    speed: float = 200.0

    def _ready(self):
        self.position = (100.0, 200.0)

    def _process(self, delta: float):
        x, y = self.position
        self.position = (x + self.speed * delta, y)
"#;

    const BOOM: &str = r#"
import pite

class Boom(pite.Node2D):
    def _ready(self):
        raise RuntimeError("ready exploded")

    def _process(self, delta: float):
        raise RuntimeError("process exploded")
"#;

    #[test]
    fn python_moves_sprite_and_vec2_crosses() {
        let dir = test_dir("mover");
        let file = fixture(&dir, "player.py", MOVER);
        let mut backend = Pyo3Backend::new("/root/Player");
        backend.load(&file, "Player").unwrap();
        backend.set_position(5.0, 6.0);
        assert_eq!(backend.position(), Some((5.0, 6.0)));
        backend.call_ready().unwrap();
        assert_eq!(backend.position(), Some((100.0, 200.0)));
        backend.call_process(0.5).unwrap();
        assert_eq!(backend.position(), Some((200.0, 200.0)));
        assert!(backend.last_error().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn exception_is_reported_and_state_survives() {
        let dir = test_dir("boom");
        let file = fixture(&dir, "boom.py", BOOM);
        let mut backend = Pyo3Backend::new("/root/Boom");
        backend.load(&file, "Boom").unwrap();
        backend.set_position(1.0, 2.0);
        let err = backend.call_ready().unwrap_err();
        assert!(err.to_string().contains("ready exploded"));
        assert!(err.to_string().contains("boom.py"));
        assert_eq!(backend.position(), Some((1.0, 2.0)));
        assert!(backend.last_error().is_some());
        let err = backend.call_process(0.016).unwrap_err();
        assert!(err.to_string().contains("process exploded"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_method_is_noop() {
        let dir = test_dir("plain");
        let file = fixture(&dir, "plain.py", "import pite\n\nclass Plain(pite.Node):\n    pass\n");
        let mut backend = Pyo3Backend::new("/root/Plain");
        backend.load(&file, "Plain").unwrap();
        backend.call_ready().unwrap();
        backend.call_process(0.016).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
}
