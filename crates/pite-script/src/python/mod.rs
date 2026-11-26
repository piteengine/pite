// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};
use std::sync::Once;

use anyhow::{Context, Result};
use pyo3::prelude::*;

use std::collections::HashMap;

use crate::ScriptBackend;

mod nodes;

#[cfg(test)]
mod tests;

pub(crate) mod props;

use nodes::{
    PyAnimatedSprite2D, PyArea2D, PyButton, PyCamera2D, PyCharacterBody2D, PyCollisionShape2D,
    PyLabel, PyNode, PyNode2D, PyRigidBody2D, PySprite2D, PyStaticBody2D, PyTileMap2D, PyTimer,
};

#[pyfunction]
#[pyo3(signature = (from, to, mask = 0xFFFFFFFF))]
fn raycast(
    py: Python<'_>,
    from: (f64, f64),
    to: (f64, f64),
    mask: u32,
) -> PyResult<Option<Py<PyAny>>> {
    let hit = crate::with_physics(|w| {
        w.raycast(
            (from.0 as f32, from.1 as f32),
            (to.0 as f32, to.1 as f32),
            mask,
        )
    })
    .flatten();

    if let Some(h) = hit {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("point", (h.point.0 as f64, h.point.1 as f64))?;
        dict.set_item("normal", (h.normal.0 as f64, h.normal.1 as f64))?;
        dict.set_item("distance", h.distance as f64)?;
        dict.set_item("collider", h.collider_node.as_str())?;
        dict.set_item(
            "body",
            h.body_node.as_ref().map(|b| b.as_str()).unwrap_or(""),
        )?;
        Ok(Some(dict.into_any().unbind()))
    } else {
        Ok(None)
    }
}

/// Frame names in a sprite-sheet atlas (`res://…ptatlas`).
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

/// Hold the Python GIL once across a batch of script calls on the current thread.
pub fn with_gil<R>(f: impl FnOnce() -> R) -> R {
    Python::attach(|_| f())
}

#[pymodule]
fn pite(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyNode>()?;
    m.add_class::<PyNode2D>()?;
    m.add_class::<PySprite2D>()?;
    m.add_class::<PyAnimatedSprite2D>()?;
    m.add_class::<PyCamera2D>()?;
    m.add_class::<PyTimer>()?;
    m.add_class::<PyLabel>()?;
    m.add_class::<PyButton>()?;
    m.add_class::<PyStaticBody2D>()?;
    m.add_class::<PyCharacterBody2D>()?;
    m.add_class::<PyRigidBody2D>()?;
    m.add_class::<PyArea2D>()?;
    m.add_class::<PyCollisionShape2D>()?;
    m.add_class::<PyTileMap2D>()?;
    m.add_class::<crate::vec2::Vec2>()?;
    m.add_class::<PyInputQuery>()?;
    m.add_class::<PyInputAction>()?;
    m.add_class::<crate::tween::PyTween>()?;
    m.add_class::<crate::tween::PyPropertyTweener>()?;
    m.add_function(wrap_pyfunction!(tween, m)?)?;
    m.add("action", PyInputAction)?;
    m.add_class::<crate::descriptor::PyChildDescriptor>()?;
    m.add_class::<crate::descriptor::PyNodeDescriptor>()?;
    m.add(
        "held",
        PyInputQuery {
            kind: InputQueryKind::Held,
        },
    )?;
    m.add(
        "pressed",
        PyInputQuery {
            kind: InputQueryKind::Pressed,
        },
    )?;
    m.add(
        "released",
        PyInputQuery {
            kind: InputQueryKind::Released,
        },
    )?;
    m.add_function(wrap_pyfunction!(raycast, m)?)?;
    m.add_function(wrap_pyfunction!(mouse, m)?)?;
    m.add_function(wrap_pyfunction!(signal, m)?)?;
    m.add_class::<PySignalDecl>()?;
    m.add_function(wrap_pyfunction!(frames, m)?)?;
    m.add_function(wrap_pyfunction!(crate::descriptor::child, m)?)?;
    m.add_function(wrap_pyfunction!(crate::descriptor::node, m)?)?;
    m.add_function(wrap_pyfunction!(crate::audio::play, m)?)?;
    m.add_function(wrap_pyfunction!(crate::audio::stop, m)?)?;
    m.add_function(wrap_pyfunction!(crate::audio::set_volume, m)?)?;
    m.add_function(wrap_pyfunction!(crate::audio::is_playing, m)?)?;
    m.add_class::<crate::PyCanvas2D>()?;
    m.add_function(wrap_pyfunction!(plugin, m)?)?;
    m.add_function(wrap_pyfunction!(autoload, m)?)?;
    Ok(())
}

#[pyfunction]
fn plugin(py: Python<'_>, id: &str) -> PyResult<Py<PyAny>> {
    let host = crate::host::current_host().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("pite.plugin() requires an active script host")
    })?;
    let plugin_host = host.plugin_host().ok_or_else(|| {
        pyo3::exceptions::PyKeyError::new_err(format!("no plugin host active for {id:?}"))
    })?;
    plugin_host
        .get_plugin(py, id)
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(format!("plugin {id:?} not found")))
}

#[pyfunction]
fn autoload(py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
    let host = crate::host::current_host().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("pite.autoload() requires an active script host")
    })?;
    let plugin_host = host.plugin_host().ok_or_else(|| {
        pyo3::exceptions::PyKeyError::new_err(format!("no plugin host active for {name:?}"))
    })?;
    plugin_host.get_autoload(py, name).ok_or_else(|| {
        pyo3::exceptions::PyKeyError::new_err(format!("autoload {name:?} not found"))
    })
}

fn map_key_attr(attr: &str) -> &'static str {
    match attr {
        "space" => "Space",
        "enter" | "return" => "Enter",
        "escape" | "esc" => "Escape",
        "tab" => "Tab",
        "backspace" => "Backspace",
        "shift" => "Shift",
        "ctrl" | "control" => "Control",
        "alt" => "Alt",
        "left" | "arrow_left" => "ArrowLeft",
        "right" | "arrow_right" => "ArrowRight",
        "up" | "arrow_up" => "ArrowUp",
        "down" | "arrow_down" => "ArrowDown",
        "mouse_left" | "mouse1" => "MouseLeft",
        "mouse_right" | "mouse2" => "MouseRight",
        "mouse_middle" | "mouse3" => "MouseMiddle",
        "a" => "A",
        "b" => "B",
        "c" => "C",
        "d" => "D",
        "e" => "E",
        "f" => "F",
        "g" => "G",
        "h" => "H",
        "i" => "I",
        "j" => "J",
        "k" => "K",
        "l" => "L",
        "m" => "M",
        "n" => "N",
        "o" => "O",
        "p" => "P",
        "q" => "Q",
        "r" => "R",
        "s" => "S",
        "t" => "T",
        "u" => "U",
        "v" => "V",
        "w" => "W",
        "x" => "X",
        "y" => "Y",
        "z" => "Z",
        "num0" | "zero" => "0",
        "num1" | "one" => "1",
        "num2" | "two" => "2",
        "num3" | "three" => "3",
        "num4" | "four" => "4",
        "num5" | "five" => "5",
        "num6" | "six" => "6",
        "num7" | "seven" => "7",
        "num8" | "eight" => "8",
        "num9" | "nine" => "9",
        _ => "",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputQueryKind {
    Held,
    Pressed,
    Released,
}

impl InputQueryKind {
    fn name(&self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Pressed => "pressed",
            Self::Released => "released",
        }
    }
}

#[pyclass(name = "InputQuery")]
pub struct PyInputQuery {
    pub(crate) kind: InputQueryKind,
}

#[pymethods]
impl PyInputQuery {
    fn __getattr__(&self, name: &str) -> bool {
        let key = map_key_attr(name);
        let key_str = if key.is_empty() { name } else { key };
        match self.kind {
            InputQueryKind::Held => crate::input_held(key_str),
            InputQueryKind::Pressed => crate::input_pressed(key_str),
            InputQueryKind::Released => crate::input_released(key_str),
        }
    }

    #[pyo3(signature = (*_args, **_kwargs))]
    fn __call__(
        &self,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let n = self.kind.name();
        Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "pite.{n} is an attribute-based accessor. Use 'pite.{n}.<key>' instead (e.g. pite.{n}.space, pite.{n}.right). Passing strings is no longer supported."
        )))
    }
}

/// Named input action query accessor exposed as `pite.action`.
#[pyclass(name = "InputAction")]
pub struct PyInputAction;

#[pymethods]
impl PyInputAction {
    fn held(&self, name: &str) -> bool {
        crate::action_held(name)
    }

    fn pressed(&self, name: &str) -> bool {
        crate::action_pressed(name)
    }

    fn released(&self, name: &str) -> bool {
        crate::action_released(name)
    }

    fn axis(&self, negative: &str, positive: &str) -> f64 {
        crate::action_axis(negative, positive)
    }

    fn vector(&self, left: &str, right: &str, up: &str, down: &str) -> crate::vec2::Vec2 {
        let (x, y) = crate::action_vector(left, right, up, down);
        crate::vec2::Vec2::new(x, y)
    }
}

#[pyfunction]
fn tween() -> crate::tween::PyTween {
    crate::tween::create_tween()
}

#[pyfunction]
fn mouse() -> crate::vec2::Vec2 {
    let (x, y) = crate::input_mouse();
    crate::vec2::Vec2::new(x, y)
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

static INIT: Once = Once::new();

pub(crate) fn ensure_init() {
    INIT.call_once(|| {
        pyo3::append_to_inittab!(pite);
        Python::initialize();
    });
}

/// Sorted wheel files inside `vendor_dir`. Missing dir reads as empty so
/// projects without vendored packages pay nothing.
pub(crate) fn vendor_wheels(vendor_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(vendor_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "whl") {
                out.push(path);
            }
        }
    }
    out.sort_unstable();
    out
}

/// True when `name` is a top-level stdlib module of the running interpreter
/// (`sys.stdlib_module_names`). Asked live so the answer tracks the pinned
/// CPython instead of a hand list; `pite check` uses it to tell stdlib
/// (ships in the export zip) from third-party (dropped) imports.
pub fn is_stdlib_module(name: &str) -> bool {
    ensure_init();
    Python::attach(|py| {
        py.import("sys")
            .and_then(|sys| sys.getattr("stdlib_module_names"))
            .and_then(|names| names.contains(name))
            .unwrap_or(false)
    })
}

#[derive(Debug, Clone)]
pub struct ScriptError {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub traceback: String,
}

pub fn extract_script_error(py: Python<'_>, err: &PyErr, ctx: &str) -> ScriptError {
    let mut file = None;
    let mut line = None;
    if let Some(tb) = err.traceback(py) {
        let mut curr = Some(tb);
        while let Some(t) = curr {
            let frame = t.getattr("tb_frame").ok();
            if let Some(f) = frame {
                let code = f.getattr("f_code").ok();
                if let Some(c) = code {
                    if let Ok(filename) =
                        c.getattr("co_filename").and_then(|x| x.extract::<String>())
                    {
                        let lineno = t
                            .getattr("tb_lineno")
                            .and_then(|x| x.extract::<usize>())
                            .unwrap_or(0);
                        file = Some(filename);
                        line = Some(lineno);
                    }
                }
            }
            curr = t.getattr("tb_next").ok().and_then(|n| n.extract().ok());
        }
    }
    let tb_str = err
        .traceback(py)
        .and_then(|t| t.format().ok())
        .unwrap_or_default();
    let message = format!("{ctx}: {err}");
    ScriptError {
        message,
        file,
        line,
        traceback: tb_str,
    }
}

pub struct Pyo3Backend {
    node_path: String,
    file: PathBuf,
    class: String,
    instance: Option<Py<PyAny>>,
    last_error: Option<String>,
    last_script_error: Option<ScriptError>,
    host: Option<std::sync::Arc<crate::ScriptHost>>,
    pending_signals: Vec<(String, Vec<String>)>,

    // Lifecycle flags, probed once at load/load_base and re-probed after
    // reload. `false` means the pyclass does not define (or inherit) the
    // corresponding method, so the runtime skips the call entirely — a
    // scriptless Sprite2D costs zero Python calls per frame.
    has_ready: bool,
    has_process: bool,
    has_physics_process: bool,
    has_draw: bool,
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
            last_script_error: None,
            host: None,
            pending_signals: Vec::new(),
            has_ready: false,
            has_process: false,
            has_physics_process: false,
            has_draw: false,
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
        if let Some(inst) = &self.instance {
            Python::attach(|py| {
                if let Ok(pos_attr) = inst.bind(py).getattr("position") {
                    if let Ok(mut vec2_bound) =
                        pos_attr.extract::<pyo3::PyRefMut<crate::vec2::Vec2>>()
                    {
                        vec2_bound.set_target(
                            host.clone(),
                            self.node_path.clone(),
                            "position".to_string(),
                        );
                    }
                }
            });
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
            // Vendored third-party packages resolve in dev and export alike:
            // the export stages vendor/ whole, so the same entries work.
            // Unpacked trees import from the directory itself; each zipped
            // wheel needs its own entry (a dir of wheels imports nothing).
            if let Some(root) = crate::project_dir() {
                let plugins = root.join("plugins");
                if plugins.is_dir() {
                    let plugins_str = plugins.to_string_lossy().to_string();
                    let already = path
                        .call_method1("__contains__", (plugins_str.as_str(),))
                        .and_then(|v| v.extract::<bool>())
                        .unwrap_or(false);
                    if !already {
                        path.call_method1("insert", (0, plugins_str.as_str()))?;
                    }
                }
                let vendor = root.join("vendor");
                for (i, wheel) in vendor_wheels(&vendor).iter().enumerate() {
                    path.call_method1("insert", (i + 1, wheel.to_string_lossy().as_ref()))?;
                }
                path.call_method1("insert", (0, vendor.to_string_lossy().as_ref()))?;
            }
            path.call_method1("insert", (0, dir.to_string_lossy().as_ref()))?;
            let modules = sys.getattr("modules")?;
            let _ = modules.call_method1("pop", (stem.as_str(), py.None()));
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
                if let Ok(pos_attr) = owned.bind(py).getattr("position") {
                    if let Ok(mut vec2_bound) =
                        pos_attr.extract::<pyo3::PyRefMut<crate::vec2::Vec2>>()
                    {
                        vec2_bound.set_target(
                            host.clone(),
                            self.node_path.clone(),
                            "position".to_string(),
                        );
                    }
                }
                host.register(self.node_path.clone(), owned.clone_ref(py));
            }
            self.instance = Some(owned);
            Ok(())
        })
    }

    /// Query the instance once for `_ready` / `_process` / `_physics_process`
    /// presence. Inherited methods count (getattr resolves through MRO), so
    /// a `Player(prite.Sprite2D)` that only overrides `_process` reports
    /// `has_ready = false, has_process = true`.
    fn probe_lifecycle(&mut self) {
        let (ready, process, physics, draw) = Python::attach(|py| {
            let Some(inst) = &self.instance else {
                return (false, false, false, false);
            };
            let b = inst.bind(py);
            let check = |name: &str| -> bool {
                b.getattr(name)
                    .ok()
                    .map(|attr| !attr.is_none() && attr.is_callable())
                    .unwrap_or(false)
            };
            (
                check("_ready"),
                check("_process"),
                check("_physics_process"),
                check("_draw"),
            )
        });
        self.has_ready = ready;
        self.has_process = process;
        self.has_physics_process = physics;
        self.has_draw = draw;
    }

    fn call_opt(&mut self, method: &str, arg: Option<f64>) -> Result<()> {
        Python::attach(|py| {
            let Some(inst) = &self.instance else {
                return Ok(());
            };
            let bound = inst.bind(py);
            let result = match method {
                "_process" => {
                    let name = pyo3::intern!(py, "_process");
                    match arg {
                        Some(delta) => bound.call_method1(name, (delta,)),
                        None => bound.call_method0(name),
                    }
                }
                "_physics_process" => {
                    let name = pyo3::intern!(py, "_physics_process");
                    match arg {
                        Some(delta) => bound.call_method1(name, (delta,)),
                        None => bound.call_method0(name),
                    }
                }
                "_ready" => {
                    let name = pyo3::intern!(py, "_ready");
                    match arg {
                        Some(delta) => bound.call_method1(name, (delta,)),
                        None => bound.call_method0(name),
                    }
                }
                other => match arg {
                    Some(delta) => bound.call_method1(other, (delta,)),
                    None => bound.call_method0(other),
                },
            };
            if let Err(err) = result {
                if err.is_instance_of::<pyo3::exceptions::PyAttributeError>(py)
                    && !bound.hasattr(method).unwrap_or(false)
                {
                    return Ok(());
                }
                let ctx = format!(
                    "{} ({}:{}.{})",
                    self.node_path,
                    self.file.display(),
                    self.class,
                    method
                );
                let script_err = extract_script_error(py, &err, &ctx);
                let msg = script_err.message.clone();
                self.last_script_error = Some(script_err);
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
        self.last_script_error = None;
        self.exec()?;
        self.probe_lifecycle();
        Ok(())
    }

    fn load_base(&mut self, type_name: &str) -> Result<()> {
        // Reset script identity; the base pyclass lives in the engine's
        // `pite` module, not on disk.
        self.file = PathBuf::new();
        self.class = type_name.to_string();
        self.last_error = None;
        self.last_script_error = None;
        Python::attach(|py| -> Result<()> {
            let pite = py.import("pite")?;
            let cls = pite.getattr(type_name).with_context(|| {
                format!("no base pyclass named {type_name:?} in the pite module")
            })?;
            let instance = cls.call0()?;
            let owned: Py<PyAny> = instance.into();
            owned.bind(py).setattr("node_path", &self.node_path)?;
            if let Some(host) = &self.host {
                if let Ok(pos_attr) = owned.bind(py).getattr("position") {
                    if let Ok(mut vec2_bound) =
                        pos_attr.extract::<pyo3::PyRefMut<crate::vec2::Vec2>>()
                    {
                        vec2_bound.set_target(
                            host.clone(),
                            self.node_path.clone(),
                            "position".to_string(),
                        );
                    }
                }
                host.register(self.node_path.clone(), owned.clone_ref(py));
            }
            self.instance = Some(owned);
            Ok(())
        })?;
        self.probe_lifecycle();
        Ok(())
    }

    fn load_plugin_class(&mut self, spec: &str) -> Result<()> {
        self.file = PathBuf::new();
        self.class = spec.to_string();
        self.last_error = None;
        self.last_script_error = None;
        Python::attach(|py| -> Result<()> {
            if let Some(root) = crate::project_dir() {
                let plugins = root.join("plugins");
                if plugins.is_dir() {
                    let sys = py.import("sys")?;
                    let path = sys.getattr("path")?;
                    let plugins_str = plugins.to_string_lossy().to_string();
                    let already = path
                        .call_method1("__contains__", (plugins_str.as_str(),))
                        .and_then(|v| v.extract::<bool>())
                        .unwrap_or(false);
                    if !already {
                        path.call_method1("insert", (0, plugins_str.as_str()))?;
                    }
                }
            }
            let instance = crate::instantiate_entrypoint(py, spec)
                .with_context(|| format!("failed to instantiate plugin node class {spec:?}"))?;
            let owned: Py<PyAny> = instance.into();
            owned.bind(py).setattr("node_path", &self.node_path)?;
            if let Some(host) = &self.host {
                if let Ok(pos_attr) = owned.bind(py).getattr("position") {
                    if let Ok(mut vec2_bound) =
                        pos_attr.extract::<pyo3::PyRefMut<crate::vec2::Vec2>>()
                    {
                        vec2_bound.set_target(
                            host.clone(),
                            self.node_path.clone(),
                            "position".to_string(),
                        );
                    }
                }
                host.register(self.node_path.clone(), owned.clone_ref(py));
            }
            self.instance = Some(owned);
            Ok(())
        })?;
        self.probe_lifecycle();
        Ok(())
    }

    fn attach_plugin_props(&self, props: &[pite_core::PropDef]) -> Result<()> {
        if props.is_empty() {
            return Ok(());
        }
        Python::attach(|py| -> Result<()> {
            let Some(inst) = &self.instance else {
                return Ok(());
            };
            let cls = inst.bind(py).get_type();
            crate::attach_plugin_prop_descriptors(py, &cls.into_any(), props)?;
            Ok(())
        })
    }

    fn has_ready(&self) -> bool {
        self.has_ready
    }

    fn has_process(&self) -> bool {
        self.has_process
    }

    fn has_physics_process(&self) -> bool {
        self.has_physics_process
    }

    fn has_draw(&self) -> bool {
        self.has_draw
    }

    fn call_ready(&mut self) -> Result<()> {
        self.call_opt("_ready", None)
    }

    fn call_process(&mut self, delta: f64) -> Result<()> {
        self.call_opt("_process", Some(delta))
    }

    fn call_physics_process(&mut self, delta: f64) -> Result<()> {
        self.call_opt("_physics_process", Some(delta))
    }

    fn call_draw(&mut self, origin: (f64, f64), z_index: i32) -> Result<Vec<crate::DrawCommand>> {
        if !self.has_draw {
            return Ok(Vec::new());
        }
        let ctx = format!("{} ({}._draw)", self.node_path, self.class);
        Python::attach(|py| {
            let Some(inst) = &self.instance else {
                return Ok(Vec::new());
            };
            let (canvas, active, commands) = crate::PyCanvas2D::new(origin, z_index);
            let canvas_py = Bound::new(py, canvas)?;
            let res = inst.bind(py).call_method1("_draw", (&canvas_py,));
            active.store(false, std::sync::atomic::Ordering::Relaxed);
            if let Err(err) = res {
                let script_err = extract_script_error(py, &err, &ctx);
                let msg = script_err.message.clone();
                self.last_script_error = Some(script_err);
                self.last_error = Some(msg.clone());
                anyhow::bail!("{msg}");
            }
            let out = commands.lock().map(|g| g.clone()).unwrap_or_default();
            Ok(out)
        })
    }

    fn reload(&mut self) -> Result<()> {
        // Every tree-backed prop (position, velocity, text, size, wait_time,
        // ...) lives in `tree.props`, which reload never touches: the tree
        // is what the new Python instance reads through. Only custom user
        // attributes (self.foo, self._hit) need the `__dict__` swap.
        let old_dict: Option<std::collections::HashMap<String, Py<PyAny>>> = Python::attach(|py| {
            let inst = self.instance.as_ref()?.bind(py);
            let dict = inst.getattr("__dict__").ok()?;
            let py_dict = dict.cast::<pyo3::types::PyDict>().ok()?;
            let mut attrs = std::collections::HashMap::new();
            for (k, v) in py_dict {
                if let Ok(key) = k.extract::<String>() {
                    if key != "node_path" && key != "_descriptor_cache" {
                        attrs.insert(key, v.into());
                    }
                }
            }
            Some(attrs)
        });
        self.last_error = None;
        self.exec()?;
        self.probe_lifecycle();
        if let Some(attrs) = old_dict {
            Python::attach(|py| {
                if let Some(inst) = &self.instance {
                    if let Ok(dict) = inst.bind(py).getattr("__dict__") {
                        if let Ok(py_dict) = dict.cast::<pyo3::types::PyDict>() {
                            for (k, v) in attrs {
                                let _ = py_dict.set_item(k, v);
                            }
                        }
                    }
                }
            });
        }
        Ok(())
    }

    fn last_error(&self) -> Option<String> {
        self.last_error.clone()
    }

    fn last_script_error(&self) -> Option<ScriptError> {
        self.last_script_error.clone()
    }
}

/// Fetch the current `ScriptHost` and this node's path from a `Bound<PyAny>`.
/// Works at any inheritance depth — the `node_path` attribute is on `PyNode`
/// (the base), so `getattr` reaches it regardless of the intermediate classes.
/// Used by `pite_props!` (which is scoped to `pite-script`) and by custom
/// node methods that need the same pair.
pub(crate) fn host_and_path(
    slf: &Bound<'_, PyAny>,
) -> PyResult<(std::sync::Arc<crate::ScriptHost>, String)> {
    let node_path: String = slf.getattr("node_path")?.extract()?;
    let host = crate::host::current_host()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("no script host"))?;
    Ok((host, node_path))
}
