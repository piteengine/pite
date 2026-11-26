// SPDX-License-Identifier: MIT OR Apache-2.0
//! Runtime plugin & autoload singleton host (`pite.plugin("<id>")` / `pite.autoload("<Name>")`).

use pyo3::prelude::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

struct LoadedRuntimePlugin {
    id: String,
    instance: Py<PyAny>,
    has_on_scene_loaded: bool,
    has_process: bool,
    has_physics_process: bool,
    errored: bool,
}

struct LoadedAutoload {
    plugin_id: String,
    instance: Py<PyAny>,
    has_on_scene_loaded: bool,
    has_process: bool,
    has_physics_process: bool,
    errored: bool,
}

/// Descriptor of a single enabled plugin's runtime and autoload entrypoints.
#[derive(Debug, Clone)]
pub struct RuntimePluginSpec {
    pub id: String,
    pub runtime: Option<String>,
    pub autoloads: Vec<(String, String)>,
}

/// Hosts runtime plugin entrypoints and autoload singletons outside the scene [`NodeTree`].
#[derive(Default)]
pub struct PluginHost {
    inner: Mutex<PluginHostInner>,
}

#[derive(Default)]
struct PluginHostInner {
    plugins: Vec<LoadedRuntimePlugin>,
    plugin_index: HashMap<String, usize>,
    autoloads: Vec<LoadedAutoload>,
    autoload_index: HashMap<String, usize>,
}

/// Imports `"module.path:ClassName"` (or `"module.path"` when no `:` is present) and
/// instantiates the class if `:ClassName` was given.
pub fn instantiate_entrypoint<'py>(py: Python<'py>, spec: &str) -> PyResult<Bound<'py, PyAny>> {
    if let Some((module_name, class_name)) = spec.split_once(':') {
        let module = py.import(module_name.trim())?;
        let cls = module.getattr(class_name.trim())?;
        cls.call0()
    } else {
        Ok(py.import(spec.trim())?.into_any())
    }
}

fn format_py_err(py: Python<'_>, err: &PyErr) -> String {
    let tb = err
        .traceback(py)
        .and_then(|t| t.format().ok())
        .unwrap_or_default();
    if tb.is_empty() {
        err.to_string()
    } else {
        format!("{tb}{err}")
    }
}

impl PluginHost {
    /// Creates an empty [`PluginHost`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads runtime entrypoints and autoload singletons for all `plugins`, calling
    /// `on_ready()` / `_ready()` on each instantiated object.
    ///
    /// Returns `(plugin_id, error_message)` pairs for any plugin or autoload that failed.
    pub fn load_plugins(
        &self,
        host: &Arc<crate::ScriptHost>,
        plugins: &[RuntimePluginSpec],
    ) -> Vec<(String, String)> {
        crate::python::ensure_init();
        let mut errors = Vec::new();
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return vec![("<host>".into(), "PluginHost lock poisoned".into())],
        };
        inner.plugins.clear();
        inner.plugin_index.clear();
        inner.autoloads.clear();
        inner.autoload_index.clear();

        crate::host::with_current_host(host, || {
            Python::attach(|py| {
                if let Some(root) = crate::project_dir() {
                    let plugins_dir = root.join("plugins");
                    if plugins_dir.is_dir() {
                        if let Ok(sys) = py.import("sys") {
                            if let Ok(path) = sys.getattr("path") {
                                let dir_str = plugins_dir.to_string_lossy().to_string();
                                let already = path
                                    .call_method1("__contains__", (dir_str.as_str(),))
                                    .and_then(|v| v.extract::<bool>())
                                    .unwrap_or(false);
                                if !already {
                                    let _ = path.call_method1("insert", (0, dir_str.as_str()));
                                }
                            }
                        }
                    }
                }

                for plugin in plugins {
                    let mut plugin_failed = false;
                    if let Some(runtime_spec) = plugin.runtime.as_deref() {
                        match instantiate_entrypoint(py, runtime_spec) {
                            Ok(inst) => {
                                if let Err(e) = call_ready_hook(&inst) {
                                    errors.push((plugin.id.clone(), format_py_err(py, &e)));
                                    plugin_failed = true;
                                } else {
                                    let has_on_scene_loaded =
                                        inst.hasattr("on_scene_loaded").unwrap_or(false);
                                    let has_process = inst.hasattr("_process").unwrap_or(false)
                                        || inst.hasattr("on_process").unwrap_or(false);
                                    let has_physics_process =
                                        inst.hasattr("_physics_process").unwrap_or(false)
                                            || inst.hasattr("on_physics_process").unwrap_or(false);
                                    let idx = inner.plugins.len();
                                    inner.plugins.push(LoadedRuntimePlugin {
                                        id: plugin.id.clone(),
                                        instance: inst.unbind(),
                                        has_on_scene_loaded,
                                        has_process,
                                        has_physics_process,
                                        errored: false,
                                    });
                                    inner.plugin_index.insert(plugin.id.clone(), idx);
                                }
                            }
                            Err(e) => {
                                errors.push((plugin.id.clone(), format_py_err(py, &e)));
                                plugin_failed = true;
                            }
                        }
                    }

                    if plugin_failed {
                        continue;
                    }

                    for (name, spec) in &plugin.autoloads {
                        if inner.autoload_index.contains_key(name) {
                            errors.push((
                                plugin.id.clone(),
                                format!("duplicate autoload singleton name {name:?}"),
                            ));
                            continue;
                        }
                        match instantiate_entrypoint(py, spec) {
                            Ok(inst) => {
                                if let Err(e) = call_ready_hook(&inst) {
                                    errors.push((plugin.id.clone(), format_py_err(py, &e)));
                                } else {
                                    let has_on_scene_loaded =
                                        inst.hasattr("on_scene_loaded").unwrap_or(false);
                                    let has_process = inst.hasattr("_process").unwrap_or(false)
                                        || inst.hasattr("on_process").unwrap_or(false);
                                    let has_physics_process =
                                        inst.hasattr("_physics_process").unwrap_or(false)
                                            || inst.hasattr("on_physics_process").unwrap_or(false);
                                    let idx = inner.autoloads.len();
                                    inner.autoloads.push(LoadedAutoload {
                                        plugin_id: plugin.id.clone(),
                                        instance: inst.unbind(),
                                        has_on_scene_loaded,
                                        has_process,
                                        has_physics_process,
                                        errored: false,
                                    });
                                    inner.autoload_index.insert(name.clone(), idx);
                                }
                            }
                            Err(e) => {
                                errors.push((plugin.id.clone(), format_py_err(py, &e)));
                            }
                        }
                    }
                }
            });
        });

        errors
    }

    /// Notifies all active runtime plugins and autoloads that a scene has been loaded/rebuilt.
    pub fn on_scene_loaded(
        &self,
        host: &Arc<crate::ScriptHost>,
        scene_path: &str,
    ) -> Vec<(String, String)> {
        let mut errors = Vec::new();
        let Ok(mut inner) = self.inner.lock() else {
            return errors;
        };
        crate::host::with_current_host(host, || {
            Python::attach(|py| {
                for p in &mut inner.plugins {
                    if p.errored || !p.has_on_scene_loaded {
                        continue;
                    }
                    if let Err(e) = p
                        .instance
                        .bind(py)
                        .call_method1("on_scene_loaded", (scene_path,))
                    {
                        p.errored = true;
                        errors.push((p.id.clone(), format_py_err(py, &e)));
                    }
                }
                for a in &mut inner.autoloads {
                    if a.errored || !a.has_on_scene_loaded {
                        continue;
                    }
                    if let Err(e) = a
                        .instance
                        .bind(py)
                        .call_method1("on_scene_loaded", (scene_path,))
                    {
                        a.errored = true;
                        errors.push((a.plugin_id.clone(), format_py_err(py, &e)));
                    }
                }
            });
        });
        errors
    }

    /// Ticks `_process` / `on_process` on all active runtime plugins and autoloads.
    pub fn on_process(&self, host: &Arc<crate::ScriptHost>, dt: f64) -> Vec<(String, String)> {
        self.tick_dt(host, dt, false)
    }

    /// Ticks `_physics_process` / `on_physics_process` on all active runtime plugins and autoloads.
    pub fn on_physics_process(
        &self,
        host: &Arc<crate::ScriptHost>,
        dt: f64,
    ) -> Vec<(String, String)> {
        self.tick_dt(host, dt, true)
    }

    fn tick_dt(
        &self,
        host: &Arc<crate::ScriptHost>,
        dt: f64,
        physics: bool,
    ) -> Vec<(String, String)> {
        let mut errors = Vec::new();
        let Ok(mut inner) = self.inner.lock() else {
            return errors;
        };
        crate::host::with_current_host(host, || {
            Python::attach(|py| {
                for p in &mut inner.plugins {
                    let active = if physics {
                        p.has_physics_process
                    } else {
                        p.has_process
                    };
                    if p.errored || !active {
                        continue;
                    }
                    let res = if physics {
                        call_dt_hook(
                            p.instance.bind(py),
                            "_physics_process",
                            "on_physics_process",
                            dt,
                        )
                    } else {
                        call_dt_hook(p.instance.bind(py), "_process", "on_process", dt)
                    };
                    if let Err(e) = res {
                        p.errored = true;
                        errors.push((p.id.clone(), format_py_err(py, &e)));
                    }
                }
                for a in &mut inner.autoloads {
                    let active = if physics {
                        a.has_physics_process
                    } else {
                        a.has_process
                    };
                    if a.errored || !active {
                        continue;
                    }
                    let res = if physics {
                        call_dt_hook(
                            a.instance.bind(py),
                            "_physics_process",
                            "on_physics_process",
                            dt,
                        )
                    } else {
                        call_dt_hook(a.instance.bind(py), "_process", "on_process", dt)
                    };
                    if let Err(e) = res {
                        a.errored = true;
                        errors.push((a.plugin_id.clone(), format_py_err(py, &e)));
                    }
                }
            });
        });
        errors
    }

    /// Returns a reference to the runtime plugin object for `id`, if registered.
    pub fn get_plugin(&self, py: Python<'_>, id: &str) -> Option<Py<PyAny>> {
        let inner = self.inner.lock().ok()?;
        let &idx = inner.plugin_index.get(id)?;
        let slot = inner.plugins.get(idx)?;
        Some(slot.instance.clone_ref(py))
    }

    /// Returns a reference to the autoload singleton for `name`, if registered.
    pub fn get_autoload(&self, py: Python<'_>, name: &str) -> Option<Py<PyAny>> {
        let inner = self.inner.lock().ok()?;
        let &idx = inner.autoload_index.get(name)?;
        let slot = inner.autoloads.get(idx)?;
        Some(slot.instance.clone_ref(py))
    }

    /// Returns `true` if no runtime plugins or autoloads are registered.
    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .map(|g| g.plugins.is_empty() && g.autoloads.is_empty())
            .unwrap_or(true)
    }
}

fn call_ready_hook(inst: &Bound<'_, PyAny>) -> PyResult<()> {
    if inst.hasattr("on_ready")? {
        inst.call_method0("on_ready")?;
    } else if inst.hasattr("_ready")? {
        inst.call_method0("_ready")?;
    }
    Ok(())
}

fn call_dt_hook(inst: &Bound<'_, PyAny>, primary: &str, fallback: &str, dt: f64) -> PyResult<()> {
    if inst.hasattr(primary)? {
        inst.call_method1(primary, (dt,))?;
    } else if inst.hasattr(fallback)? {
        inst.call_method1(fallback, (dt,))?;
    }
    Ok(())
}
