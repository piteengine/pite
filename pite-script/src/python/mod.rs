// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};
use std::sync::Once;

use anyhow::{Context, Result};
use pyo3::prelude::*;

use std::collections::HashMap;

use crate::ScriptBackend;

mod nodes;

pub(crate) mod props;

use nodes::{
    PyArea2D, PyButton, PyCamera2D, PyCharacterBody2D, PyCollisionShape2D, PyLabel, PyNode,
    PyNode2D, PyRigidBody2D, PySprite2D, PyStaticBody2D, PyTimer,
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
    m.add_class::<PyStaticBody2D>()?;
    m.add_class::<PyCharacterBody2D>()?;
    m.add_class::<PyRigidBody2D>()?;
    m.add_class::<PyArea2D>()?;
    m.add_class::<PyCollisionShape2D>()?;
    m.add_class::<crate::vec2::Vec2>()?;
    m.add_class::<PyInputQuery>()?;
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
    Ok(())
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

fn ensure_init() {
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

/// Fetch the current `ScriptHost` and this node's path from a `Bound<PyAny>`.
/// Works at any inheritance depth — the `node_path` attribute is on `PyNode`
/// (the base), so `getattr` reaches it regardless of the intermediate classes.
/// Used by `pite_props!` (which is scoped to `pite-script`) and by custom
/// node methods that need the same pair.
#[allow(dead_code)] // exercised by pite_props! in cfg(test); real accessors land in commit 5
pub(crate) fn host_and_path(
    slf: &Bound<'_, PyAny>,
) -> PyResult<(std::sync::Arc<crate::ScriptHost>, String)> {
    let node_path: String = slf.getattr("node_path")?.extract()?;
    let host = crate::host::current_host()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("no script host"))?;
    Ok((host, node_path))
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
        self.exec()
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

    fn reload(&mut self) -> Result<()> {
        let pos = self.position();
        let vel = self.velocity();
        let text = self.text();
        let size = self.size();
        let wait = self.wait_time();
        let old_dict: Option<std::collections::HashMap<String, Py<PyAny>>> = Python::attach(|py| {
            let inst = self.instance.as_ref()?.bind(py);
            let dict = inst.getattr("__dict__").ok()?;
            let py_dict = dict.cast::<pyo3::types::PyDict>().ok()?;
            let mut attrs = std::collections::HashMap::new();
            for (k, v) in py_dict {
                if let Ok(key) = k.extract::<String>() {
                    if key != "node_path"
                        && key != "_descriptor_cache"
                        && key != "position"
                        && key != "velocity"
                    {
                        attrs.insert(key, v.into());
                    }
                }
            }
            Some(attrs)
        });
        self.last_error = None;
        self.exec()?;
        if let Some((x, y)) = pos {
            self.set_position(x, y);
        }
        if let Some((vx, vy)) = vel {
            self.set_velocity(vx, vy);
        }
        if let Some(text) = text {
            self.set_text(&text);
        }
        if let Some((w, h)) = size {
            self.set_size(w, h);
        }
        if let Some(wait) = wait {
            self.set_wait_time(wait);
        }
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

    fn position(&self) -> Option<(f64, f64)> {
        Python::attach(|py| {
            let inst = self.instance.as_ref()?.bind(py);
            let attr = inst.getattr("position").ok()?;
            if let Ok(v) = attr.extract::<crate::vec2::Vec2>() {
                Some((v.x(), v.y()))
            } else if let Ok(t) = attr.extract::<(f64, f64)>() {
                Some(t)
            } else {
                None
            }
        })
    }

    fn set_position(&mut self, x: f64, y: f64) {
        Python::attach(|py| {
            if let Some(inst) = &self.instance {
                let _ = inst.bind(py).setattr("position", (x, y));
            }
        });
    }

    fn velocity(&self) -> Option<(f64, f64)> {
        Python::attach(|py| {
            let inst = self.instance.as_ref()?.bind(py);
            let attr = inst.getattr("velocity").ok()?;
            if let Ok(v) = attr.extract::<crate::vec2::Vec2>() {
                Some((v.x(), v.y()))
            } else if let Ok(t) = attr.extract::<(f64, f64)>() {
                Some(t)
            } else {
                None
            }
        })
    }

    fn set_velocity(&mut self, vx: f64, vy: f64) {
        Python::attach(|py| {
            if let Some(inst) = &self.instance {
                let _ = inst.bind(py).setattr("velocity", (vx, vy));
            }
        });
    }

    fn set_on_floor(&mut self, on_floor: bool) {
        Python::attach(|py| {
            if let Some(inst) = &self.instance {
                let _ = inst.bind(py).setattr("_is_on_floor", on_floor);
            }
        });
    }

    fn set_on_wall(&mut self, on_wall: bool) {
        Python::attach(|py| {
            if let Some(inst) = &self.instance {
                let _ = inst.bind(py).setattr("_is_on_wall", on_wall);
            }
        });
    }

    fn set_on_ceiling(&mut self, on_ceiling: bool) {
        Python::attach(|py| {
            if let Some(inst) = &self.instance {
                let _ = inst.bind(py).setattr("_is_on_ceiling", on_ceiling);
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

    fn size(&self) -> Option<(f64, f64)> {
        Python::attach(|py| {
            let inst = self.instance.as_ref()?.bind(py);
            let attr = inst.getattr("size").ok()?;
            if let Ok(v) = attr.extract::<crate::vec2::Vec2>() {
                Some((v.x(), v.y()))
            } else if let Ok(t) = attr.extract::<(f64, f64)>() {
                Some(t)
            } else {
                None
            }
        })
    }

    fn set_size(&mut self, w: f64, h: f64) {
        Python::attach(|py| {
            if let Some(inst) = &self.instance {
                let _ = inst.bind(py).setattr("size", (w, h));
            }
        });
    }

    fn wait_time(&self) -> Option<f64> {
        Python::attach(|py| {
            self.instance
                .as_ref()?
                .bind(py)
                .getattr("wait_time")
                .ok()?
                .extract()
                .ok()
        })
    }

    fn set_wait_time(&mut self, wait: f64) {
        Python::attach(|py| {
            if let Some(inst) = &self.instance {
                let _ = inst.bind(py).setattr("wait_time", wait);
            }
        });
    }

    fn last_error(&self) -> Option<String> {
        self.last_error.clone()
    }

    fn last_script_error(&self) -> Option<ScriptError> {
        self.last_script_error.clone()
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

    #[test]
    fn vendor_wheels_lists_sorted_whls() {
        let dir = test_dir("wheels");
        assert!(vendor_wheels(&dir.join("missing")).is_empty());
        std::fs::write(dir.join("b-1.0-py3-none-any.whl"), b"w").unwrap();
        std::fs::write(dir.join("a-2.0-py3-none-any.whl"), b"w").unwrap();
        std::fs::write(dir.join("notes.txt"), b"w").unwrap();
        let got = vendor_wheels(&dir);
        assert_eq!(
            got.iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec![
                "a-2.0-py3-none-any.whl".to_string(),
                "b-1.0-py3-none-any.whl".to_string()
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
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
            "plain_noop.py",
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
            d.props
                .insert("position".to_string(), pite_core::PropValue::Vec2(0.0, 0.0));
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
        let dir = test_dir("notetext");
        let file = fixture(
            &dir,
            "plain_notetext.py",
            "import pite\n\nclass Plain(pite.Node):\n    pass\n",
        );
        let mut backend = Pyo3Backend::new("plain");
        backend.load(&file, "Plain").unwrap();
        assert_eq!(backend.text(), None);
        backend.set_text("ignored");
        backend.call_process(0.016).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    const SIZER: &str = r#"
import pite

class Sizer(pite.Button):
    def _ready(self):
        self.size = (120.0, 40.0)

    def _process(self, delta):
        w, h = self.size
        self.size = (w + 1.0, h)
"#;

    #[test]
    fn button_size_syncs_both_ways() {
        let dir = test_dir("size");
        let file = fixture(&dir, "sizer.py", SIZER);
        let mut backend = Pyo3Backend::new("sizer");
        backend.load(&file, "Sizer").unwrap();
        backend.set_size(10.0, 20.0);
        assert_eq!(backend.size(), Some((10.0, 20.0)));
        backend.call_ready().unwrap();
        assert_eq!(backend.size(), Some((120.0, 40.0)));
        backend.call_process(0.016).unwrap();
        assert_eq!(backend.size(), Some((121.0, 40.0)));
        std::fs::remove_dir_all(&dir).ok();
    }

    const SENSOR: &str = r#"
import pite

class Sensor(pite.Node2D):
    tripped = pite.signal(int, "str")

    def _process(self, delta):
        if pite.pressed.space:
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
            d.props
                .insert("position".to_string(), pite_core::PropValue::Vec2(0.0, 0.0));
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

        crate::input_clear();
        crate::input_begin_frame();
        crate::input_set_key("Space", true);
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
        crate::input_clear();
        crate::input_begin_frame();
        crate::input_set_key("Space", true);
        sensor.call_process(0.016).unwrap();
        assert_eq!(listener.position(), Some((7.0, 0.0)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn api_redesign_vec2_input_and_descriptors() {
        let _guard = crate::host::HOST_SERIAL.lock().unwrap();
        crate::python::ensure_init();

        pyo3::Python::attach(|py| {
            // 1. Vec2 math and operations
            let v = crate::vec2::Vec2::new(10.0, 20.0);
            assert_eq!(v.x(), 10.0);
            assert_eq!(v.y(), 20.0);

            let v_obj = pyo3::Bound::new(py, v).unwrap();
            let sum: crate::vec2::Vec2 = v_obj
                .call_method1("__add__", ((5.0, 5.0),))
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(sum.x(), 15.0);
            assert_eq!(sum.y(), 25.0);

            // Unpacking x, y = vec
            let iter = v_obj.try_iter().unwrap();
            let items: Vec<f64> = iter
                .map(|item| item.unwrap().extract::<f64>().unwrap())
                .collect();
            assert_eq!(items, vec![10.0, 20.0]);

            // 2. Input queries (held, pressed, released)
            crate::input_begin_frame();
            crate::input_set_key("Space", true);

            let held_query = pyo3::Bound::new(
                py,
                PyInputQuery {
                    kind: InputQueryKind::Held,
                },
            )
            .unwrap();
            let is_space_held: bool = held_query.getattr("space").unwrap().extract().unwrap();
            assert!(is_space_held);

            // Calling pite.held("Space") raises TypeError
            let err = held_query.call1(("Space",)).unwrap_err();
            assert!(err.is_instance_of::<pyo3::exceptions::PyTypeError>(py));
            assert!(err.to_string().contains("attribute-based accessor"));

            let pressed_query = pyo3::Bound::new(
                py,
                PyInputQuery {
                    kind: InputQueryKind::Pressed,
                },
            )
            .unwrap();
            let is_space_pressed: bool = pressed_query.getattr("space").unwrap().extract().unwrap();
            assert!(is_space_pressed);
        });
    }

    #[test]
    fn missing_node_returns_default_not_panic() {
        // A scripted node can be removed from the tree while its slot still
        // holds a Python instance (the runtime keeps slots until rebuild).
        // Reading its props must return the type default, not panic — that
        // is the contract `dropped_button_disconnects_silently` in
        // pite-runtime exercises end-to-end.
        let _guard = crate::host::HOST_SERIAL.lock().unwrap();
        use pite_core::{NodeDesc, NodeId, NodeTree};
        use std::sync::Arc;

        let mut tree = NodeTree::new();
        let mut root = NodeDesc::new(NodeId::from("root".to_string()), "Node2D");
        root.name = "Root".to_string();
        tree.insert(root).unwrap();
        let host = Arc::new(crate::ScriptHost::new(tree));

        // Both getters must return the zero default with the node absent.
        assert_eq!(host.get_f64("ghost", "wait_time"), 0.0);
        assert_eq!(host.get_bool("ghost", "disabled"), false);
        assert_eq!(host.get_i64("ghost", "collision_layer"), 0);
        assert_eq!(host.get_string("ghost", "text"), "");
        let v = host.get_vec2("ghost", "position");
        assert_eq!((v.x, v.y), (0.0, 0.0));

        // Writing to a missing node is a silent no-op (already true before
        // commit 4; the schema-defaults work must not change it).
        host.set_prop("ghost", "position", pite_core::PropValue::Vec2(9.0, 9.0));
    }
}
