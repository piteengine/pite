use std::path::{Path, PathBuf};
use std::sync::Once;

use anyhow::{Context, Result};
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use std::collections::HashMap;

use crate::host::NodeProxy;
use crate::{resolve_caller, ScriptBackend};

#[pyclass(subclass, name = "Node")]
struct PyNode {
    #[pyo3(get, set)]
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

    fn get_node(&self, py: Python<'_>, path: &str) -> PyResult<Py<NodeProxy>> {
        let proxy = resolve_caller(&self.node_path, path)
            .map_err(|e| pyo3::exceptions::PyKeyError::new_err(e.to_string()))?;
        Py::new(py, proxy)
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
        (
            Self {
                position: (0.0, 0.0),
            },
            PyNode::new(),
        )
    }
}

#[pyclass(extends = PyNode2D, subclass, name = "Sprite2D")]
struct PySprite2D {
    #[pyo3(get, set)]
    texture: String,
    #[pyo3(get, set)]
    atlas: String,
    #[pyo3(get, set)]
    frame: String,
}

#[pymethods]
impl PySprite2D {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D {
                position: (0.0, 0.0),
            })
            .add_subclass(PySprite2D {
                texture: String::new(),
                atlas: String::new(),
                frame: String::new(),
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
            .add_subclass(PyNode2D {
                position: (0.0, 0.0),
            })
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

#[pyclass(extends = PyNode, subclass, name = "Label")]
struct PyLabel {
    #[pyo3(get, set)]
    text: String,
}

#[pymethods]
impl PyLabel {
    #[new]
    fn new() -> (Self, PyNode) {
        (
            Self {
                text: String::new(),
            },
            PyNode::new(),
        )
    }
}

#[pyclass(extends = PyNode2D, subclass, name = "Button")]
struct PyButton {
    #[pyo3(get, set)]
    text: String,
}

#[pymethods]
impl PyButton {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D {
                position: (0.0, 0.0),
            })
            .add_subclass(PyButton {
                text: String::new(),
            })
    }
}

/// Frame names in a sprite-sheet atlas (`res://…atlas.json`).
#[pyfunction]
fn frames(atlas_ref: &str) -> PyResult<Vec<String>> {
    let root = crate::project_dir().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err(
            "no project root known; pite.frames() needs res:// inside a project",
        )
    })?;
    let atlas = pite_assets::load_sidecar(&root, atlas_ref)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e:#}")))?;
    Ok(atlas.names().into_iter().map(str::to_string).collect())
}

#[pymodule]
fn pite(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyNode>()?;
    m.add_class::<PyNode2D>()?;
    m.add_class::<PySprite2D>()?;
    m.add_class::<PyCamera2D>()?;
    m.add_class::<PyTimer>()?;
    m.add_class::<PyLabel>()?;
    m.add_class::<PyButton>()?;
    m.add_function(wrap_pyfunction!(held, m)?)?;
    m.add_function(wrap_pyfunction!(pressed, m)?)?;
    m.add_function(wrap_pyfunction!(released, m)?)?;
    m.add_function(wrap_pyfunction!(mouse, m)?)?;
    m.add_function(wrap_pyfunction!(signal, m)?)?;
    m.add_class::<PySignalDecl>()?;
    m.add_function(wrap_pyfunction!(frames, m)?)?;
    m.add_function(wrap_pyfunction!(crate::audio::play, m)?)?;
    m.add_function(wrap_pyfunction!(crate::audio::stop, m)?)?;
    m.add_function(wrap_pyfunction!(crate::audio::set_volume, m)?)?;
    m.add_function(wrap_pyfunction!(crate::audio::is_playing, m)?)?;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (*types))]
fn signal(types: Vec<Bound<'_, PyAny>>) -> PyResult<PySignalDecl> {
    Ok(PySignalDecl {
        payload: read_payload_types(&types)?,
        attr: String::new(),
    })
}

pub(crate) fn read_payload_types(types: &[Bound<'_, PyAny>]) -> PyResult<Vec<String>> {
    let mut out = Vec::with_capacity(types.len());
    for ty in types {
        if let Ok(s) = ty.extract::<String>() {
            out.push(s);
        } else if ty.is_instance_of::<pyo3::types::PyType>() {
            let type_name: String = ty.getattr("__name__")?.extract()?;
            match type_name.as_str() {
                "int" | "float" | "str" | "bool" => out.push(type_name),
                _ => {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "signal payload type must be int, float, str or bool",
                    ));
                }
            }
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "signal payload type must be int, float, str or bool",
            ));
        }
    }
    Ok(out)
}

#[pyclass(name = "Signal")]
#[derive(Clone)]
pub(crate) struct PySignalDecl {
    #[pyo3(get)]
    payload: Vec<String>,
    #[pyo3(get, set)]
    attr: String,
}

#[pymethods]
impl PySignalDecl {
    fn __set_name__(&mut self, _owner: Bound<'_, PyAny>, name: String) {
        self.attr = name;
    }

    fn __get__(
        &self,
        py: Python<'_>,
        obj: Option<Bound<'_, PyAny>>,
        _owner: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match obj {
            None => Ok(Bound::new(py, self.clone())?.into_any().unbind()),
            Some(obj) => {
                let node_path: String = obj.getattr("node_path")?.extract()?;
                let host = crate::host::current_host().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("signal access has no script host")
                })?;
                Ok(Bound::new(
                    py,
                    crate::host::BoundSignal::with_host(host, node_path, self.attr.clone()),
                )?
                .into_any()
                .unbind())
            }
        }
    }
}

#[pyfunction]
fn held(key: &str) -> bool {
    crate::input_held(key)
}

#[pyfunction]
fn pressed(key: &str) -> bool {
    crate::input_pressed(key)
}

#[pyfunction]
fn released(key: &str) -> bool {
    crate::input_released(key)
}

#[pyfunction]
fn mouse() -> (f64, f64) {
    crate::input_mouse()
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
    host: Option<std::sync::Arc<crate::ScriptHost>>,
    pending_signals: Vec<(String, Vec<String>)>,
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
            host: None,
            pending_signals: Vec::new(),
        }
    }

    pub fn set_host(&mut self, host: std::sync::Arc<crate::ScriptHost>) {
        if !self.pending_signals.is_empty() {
            if let Err(e) = host.declare_signals(&self.node_path, self.pending_signals.clone()) {
                self.last_error = Some(e.to_string());
            } else {
                self.pending_signals.clear();
            }
        }
        self.host = Some(host);
    }

    fn read_signal_defs(
        py: Python<'_>,
        cls: &Bound<'_, PyAny>,
    ) -> Result<Vec<(String, Vec<String>)>> {
        let mut defs: Vec<(String, Vec<String>)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut consider = |name: String, value: Bound<'_, PyAny>| {
            if !seen.insert(name.clone()) {
                return;
            }
            if !value.is_instance_of::<PySignalDecl>() {
                return;
            }
            let Ok(payload) = value.getattr("payload") else {
                return;
            };
            let Ok(types) = payload.extract::<Vec<String>>() else {
                return;
            };
            defs.push((name, types));
        };
        for name in cls
            .dir()
            .map(|list| {
                list.iter()
                    .filter_map(|item| item.extract::<String>().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        {
            if let Ok(value) = cls.getattr(name.as_str()) {
                consider(name.to_string(), value);
            }
        }
        if let Ok(ann) = cls.getattr("__annotations__") {
            if !ann.is_none() {
                if let Ok(map) = ann.extract::<HashMap<String, Bound<'_, PyAny>>>() {
                    for (name, value) in map {
                        if value.is_instance_of::<PySignalDecl>() {
                            let _ = value.setattr("attr", name.as_str());
                            let _ = cls.setattr(name.as_str(), &value);
                        }
                        consider(name, value);
                    }
                }
            }
        }
        let _ = py;
        Ok(defs)
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
            let defs = Self::read_signal_defs(py, &cls).with_context(|| {
                format!(
                    "script {} class {} has bad signals declaration",
                    self.file.display(),
                    self.class
                )
            })?;
            if let Some(host) = &self.host {
                host.declare_signals(&self.node_path, defs.clone())?;
            }
            self.pending_signals = defs;
            let instance = cls.call0().with_context(|| {
                format!(
                    "cannot instantiate {} in {}",
                    self.class,
                    self.file.display()
                )
            })?;
            let owned: Py<PyAny> = instance.into();
            owned.bind(py).setattr("node_path", &self.node_path)?;
            if let Some(host) = &self.host {
                host.register(self.node_path.clone(), owned.clone_ref(py));
            }
            self.instance = Some(owned);
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
        let text = self.text();
        self.last_error = None;
        self.exec()?;
        if let Some((x, y)) = pos {
            self.set_position(x, y);
        }
        if let Some(text) = text {
            self.set_text(&text);
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

    fn text(&self) -> Option<String> {
        Python::attach(|py| {
            self.instance
                .as_ref()?
                .bind(py)
                .getattr("text")
                .ok()?
                .extract()
                .ok()
        })
    }

    fn set_text(&mut self, text: &str) {
        Python::attach(|py| {
            if let Some(inst) = &self.instance {
                let _ = inst.bind(py).setattr("text", text);
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
        let file = fixture(
            &dir,
            "plain.py",
            "import pite\n\nclass Plain(pite.Node):\n    pass\n",
        );
        let mut backend = Pyo3Backend::new("/root/Plain");
        backend.load(&file, "Plain").unwrap();
        backend.call_ready().unwrap();
        backend.call_process(0.016).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    const VICTIM: &str = r#"
import pite

class Victim(pite.Node2D):
    def _ready(self):
        self.hp = 3

    def take_damage(self, amount):
        self.hp = self.hp - amount
"#;

    const ATTACKER: &str = r#"
import pite

class Attacker(pite.Node2D):
    def _process(self, delta):
        if not getattr(self, "_hit", False):
            self._hit = True
            self.get_node("../Victim").take_damage(1)
"#;

    #[test]
    fn get_node_direct_call_hits_sibling() {
        let _guard = crate::host::HOST_SERIAL.lock().unwrap();
        use pite_core::{NodeDesc, NodeId, NodeTree};
        use std::sync::Arc;

        let dir = test_dir("getnode");
        let victim_file = fixture(&dir, "victim.py", VICTIM);
        let attacker_file = fixture(&dir, "attacker.py", ATTACKER);

        let mut tree = NodeTree::new();
        let mut root = NodeDesc::new(NodeId::from("root".to_string()), "Node");
        root.name = "Root".to_string();
        tree.insert(root).unwrap();
        for (id, name) in [("a", "Attacker"), ("v", "Victim")] {
            let mut d = NodeDesc::new(NodeId::from(id.to_string()), "Node2D");
            d.name = name.to_string();
            d.parent = Some(NodeId::from("root".to_string()));
            tree.insert(d).unwrap();
        }
        let host = Arc::new(crate::ScriptHost::new(tree));
        crate::set_current_host(host.clone());

        let mut victim = Pyo3Backend::new("v");
        victim.set_host(host.clone());
        victim.load(&victim_file, "Victim").unwrap();
        victim.call_ready().unwrap();

        let mut attacker = Pyo3Backend::new("a");
        attacker.set_host(host.clone());
        attacker.load(&attacker_file, "Attacker").unwrap();
        attacker.call_process(0.016).unwrap();

        Python::attach(|py| {
            let inst = host.lookup_instance(py, "v").unwrap();
            let hp: i64 = inst.bind(py).getattr("hp").unwrap().extract().unwrap();
            assert_eq!(hp, 2);
        });
        std::fs::remove_dir_all(&dir).ok();
    }

    const LABELER: &str = r#"
import pite

class Hud(pite.Label):
    def _ready(self):
        self.text = "HP: 3"

    def _process(self, delta):
        self.text = "HP: 2"
"#;

    #[test]
    fn label_text_syncs_both_ways() {
        let dir = test_dir("label");
        let file = fixture(&dir, "hud.py", LABELER);
        let mut backend = Pyo3Backend::new("hud");
        backend.load(&file, "Hud").unwrap();
        backend.set_text("seed");
        assert_eq!(backend.text().as_deref(), Some("seed"));
        backend.call_ready().unwrap();
        assert_eq!(backend.text().as_deref(), Some("HP: 3"));
        backend.call_process(0.016).unwrap();
        assert_eq!(backend.text().as_deref(), Some("HP: 2"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plain_node_has_no_text_channel() {
        let dir = test_dir("notext");
        let file = fixture(
            &dir,
            "plain.py",
            "import pite\n\nclass Plain(pite.Node):\n    pass\n",
        );
        let mut backend = Pyo3Backend::new("plain");
        backend.load(&file, "Plain").unwrap();
        assert_eq!(backend.text(), None);
        backend.set_text("ignored");
        backend.call_process(0.016).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    const SENSOR: &str = r#"
import pite

class Sensor(pite.Node2D):
    tripped: pite.signal(int, "str")

    def _process(self, delta):
        if pite.pressed("T-Trip"):
            self.tripped.emit(7, "hi")
"#;

    const LISTENER: &str = r#"
import pite

class Listener(pite.Node2D):
    def _ready(self):
        self.get_node("../Sensor").tripped.connect(self.on_tripped)

    def on_tripped(self, n, msg):
        self.position = (float(n), 0.0)
        self.last_msg = msg
"#;

    const BAD_DECL: &str =
        "import pite\n\nclass Bad(pite.Node2D):\n    bad = pite.signal(\"wat\")\n";

    #[test]
    fn signals_declare_connect_emit_drop() {
        let _guard = crate::host::HOST_SERIAL.lock().unwrap();
        use pite_core::{NodeDesc, NodeId, NodeTree};
        use std::sync::Arc;

        let dir = test_dir("signals");
        let sensor_file = fixture(&dir, "sensor.py", SENSOR);
        let listener_file = fixture(&dir, "listener.py", LISTENER);
        let bad_file = fixture(&dir, "bad.py", BAD_DECL);

        let mut tree = NodeTree::new();
        let mut root = NodeDesc::new(NodeId::from("root".to_string()), "Node");
        root.name = "Root".to_string();
        tree.insert(root).unwrap();
        for (id, name) in [("s", "Sensor"), ("l", "Listener")] {
            let mut d = NodeDesc::new(NodeId::from(id.to_string()), "Node2D");
            d.name = name.to_string();
            d.parent = Some(NodeId::from("root".to_string()));
            tree.insert(d).unwrap();
        }
        let host = Arc::new(crate::ScriptHost::new(tree));
        crate::set_current_host(host.clone());

        let mut bad = Pyo3Backend::new("bad");
        bad.set_host(host.clone());
        let err = bad.load(&bad_file, "Bad").unwrap_err();
        assert!(err.to_string().contains("unknown payload type"));

        let mut sensor = Pyo3Backend::new("s");
        sensor.set_host(host.clone());
        sensor.load(&sensor_file, "Sensor").unwrap();
        let mut listener = Pyo3Backend::new("l");
        listener.set_host(host.clone());
        listener.load(&listener_file, "Listener").unwrap();
        listener.call_ready().unwrap();

        crate::input_begin_frame();
        crate::input_set_key("T-Trip", true);
        sensor.call_process(0.016).unwrap();
        assert_eq!(listener.position(), Some((7.0, 0.0)));
        Python::attach(|py| {
            let inst = host.lookup_instance(py, "l").unwrap();
            let msg: String = inst
                .bind(py)
                .getattr("last_msg")
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(msg, "hi");
        });

        Python::attach(|py| {
            let wrong = pyo3::types::PyTuple::new(
                py,
                vec![
                    "nope".into_pyobject(py).unwrap(),
                    "hi".into_pyobject(py).unwrap(),
                ],
            )
            .unwrap();
            let err = host.emit(py, "s", "tripped", &wrong).unwrap_err();
            assert!(err.to_string().contains("wants int"));
            let empty = pyo3::types::PyTuple::empty(py);
            let err = host.emit(py, "s", "missing", &empty).unwrap_err();
            assert!(err.to_string().contains("unknown signal"));
            let err = host.connect("s", "missing", "l", "on_tripped").unwrap_err();
            assert!(err.to_string().contains("unknown signal"));
        });

        host.disconnect_node("l");
        crate::input_begin_frame();
        crate::input_set_key("T-Trip", true);
        sensor.call_process(0.016).unwrap();
        assert_eq!(listener.position(), Some((7.0, 0.0)));
        std::fs::remove_dir_all(&dir).ok();
    }
}
