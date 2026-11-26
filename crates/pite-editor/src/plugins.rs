// SPDX-License-Identifier: MIT OR Apache-2.0
//! Editor-only plugin host (`EditorUI`, `EditorContext`, bottom tabs, inspector sections, menus,
//! and viewport hooks), gated strictly behind `workspace_trusted == true`.

use eframe::egui;
use pite_core::PropValue;
use pite_script::pyo3::{self, prelude::*, types::PyAnyMethods};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    ptr::NonNull,
    sync::{Arc, Mutex},
};

struct ActiveUi {
    ptr: NonNull<egui::Ui>,
    token: u64,
}

thread_local! {
    static NEXT_TOKEN: Cell<u64> = const { Cell::new(1) };
    static ACTIVE_UI: RefCell<Option<ActiveUi>> = const { RefCell::new(None) };
    static UI_IN_USE: Cell<bool> = const { Cell::new(false) };
}

/// RAII scope binding an `&mut egui::Ui` to a unique `frame_token` for the duration of a single
/// Python editor UI callback. Dropping the scope invalidates all [`PyEditorUI`] handles from it.
pub struct EditorUiScope {
    prev: Option<ActiveUi>,
    token: u64,
}

impl EditorUiScope {
    /// Enters a new scoped UI frame, returning the RAII guard and its matching `EditorUI` handle.
    pub fn enter(ui: &mut egui::Ui) -> (Self, PyEditorUI) {
        let token = NEXT_TOKEN.with(|c| {
            let t = c.get();
            c.set(t.wrapping_add(1).max(1));
            t
        });
        let prev = ACTIVE_UI.with(|cell| {
            cell.replace(Some(ActiveUi {
                ptr: NonNull::from(ui),
                token,
            }))
        });
        (Self { prev, token }, PyEditorUI { token })
    }
}

impl Drop for EditorUiScope {
    fn drop(&mut self) {
        ACTIVE_UI.with(|cell| {
            let prev = self.prev.take();
            let cur = cell.replace(prev);
            debug_assert!(cur.as_ref().is_none_or(|a| a.token == self.token));
        });
    }
}

fn with_scoped_ui<R>(token: u64, f: impl FnOnce(&mut egui::Ui) -> PyResult<R>) -> PyResult<R> {
    if UI_IN_USE.with(|c| c.get()) {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "re-entrant EditorUI call is not permitted",
        ));
    }
    let ptr = ACTIVE_UI.with(|cell| {
        let borrow = cell.borrow();
        let active = borrow.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "EditorUI used outside an active editor UI callback",
            )
        })?;
        if active.token != token {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "EditorUI handle is stale (from an earlier frame or outer scope)",
            ));
        }
        Ok(active.ptr)
    })?;

    struct InUseReset;
    impl Drop for InUseReset {
        fn drop(&mut self) {
            UI_IN_USE.with(|c| c.set(false));
        }
    }
    UI_IN_USE.with(|c| c.set(true));
    let _reset = InUseReset;

    // SAFETY: `ptr` was created from `&mut egui::Ui` in `EditorUiScope::enter` on this exact UI
    // thread. `EditorUiScope::drop` clears/restores `ACTIVE_UI` on scope exit or unwind, `token`
    // ensures stale handles from other scopes/frames are rejected, and `UI_IN_USE` guarantees
    // exclusive `&mut` access with no aliasing or re-entrancy.
    let ui = unsafe { &mut *ptr.as_ptr() };
    f(ui)
}

/// Scoped immediate-mode UI surface passed to Python editor plugin callbacks.
#[pyclass(crate = "pite_script::pyo3", name = "EditorUI", unsendable)]
#[derive(Clone, Copy)]
pub struct PyEditorUI {
    token: u64,
}

#[pymethods(crate = "pite_script::pyo3")]
impl PyEditorUI {
    fn label(&self, text: &str) -> PyResult<()> {
        with_scoped_ui(self.token, |ui| {
            ui.label(text);
            Ok(())
        })
    }

    fn heading(&self, text: &str) -> PyResult<()> {
        with_scoped_ui(self.token, |ui| {
            ui.heading(text);
            Ok(())
        })
    }

    fn button(&self, text: &str) -> PyResult<bool> {
        with_scoped_ui(self.token, |ui| Ok(ui.button(text).clicked()))
    }

    fn small_button(&self, text: &str) -> PyResult<bool> {
        with_scoped_ui(self.token, |ui| Ok(ui.small_button(text).clicked()))
    }

    fn checkbox(&self, label: &str, mut checked: bool) -> PyResult<bool> {
        with_scoped_ui(self.token, |ui| {
            ui.checkbox(&mut checked, label);
            Ok(checked)
        })
    }

    fn text_edit(&self, mut value: String) -> PyResult<String> {
        with_scoped_ui(self.token, |ui| {
            ui.text_edit_singleline(&mut value);
            Ok(value)
        })
    }

    fn slider_float(&self, label: &str, mut value: f64, min: f64, max: f64) -> PyResult<f64> {
        with_scoped_ui(self.token, |ui| {
            ui.add(egui::Slider::new(&mut value, min..=max).text(label));
            Ok(value)
        })
    }

    fn slider_int(&self, label: &str, mut value: i64, min: i64, max: i64) -> PyResult<i64> {
        with_scoped_ui(self.token, |ui| {
            ui.add(egui::Slider::new(&mut value, min..=max).text(label));
            Ok(value)
        })
    }

    #[pyo3(signature = (label, value, speed = 1.0))]
    fn drag_float(&self, label: &str, mut value: f64, speed: f64) -> PyResult<f64> {
        with_scoped_ui(self.token, |ui| {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(egui::DragValue::new(&mut value).speed(speed));
            });
            Ok(value)
        })
    }

    fn separator(&self) -> PyResult<()> {
        with_scoped_ui(self.token, |ui| {
            ui.separator();
            Ok(())
        })
    }

    fn collapsing(&self, py: Python<'_>, title: &str, callback: Py<PyAny>) -> PyResult<()> {
        with_scoped_ui(self.token, |ui| {
            let mut inner_err: Option<PyErr> = None;
            ui.collapsing(title, |inner| {
                UI_IN_USE.with(|c| c.set(false));
                let (_guard, inner_ui) = EditorUiScope::enter(inner);
                if let Err(e) = callback.call1(py, (inner_ui,)) {
                    inner_err = Some(e);
                }
                UI_IN_USE.with(|c| c.set(true));
            });
            if let Some(e) = inner_err {
                return Err(e);
            }
            Ok(())
        })
    }

    fn horizontal(&self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        with_scoped_ui(self.token, |ui| {
            let mut inner_err: Option<PyErr> = None;
            ui.horizontal(|inner| {
                UI_IN_USE.with(|c| c.set(false));
                let (_guard, inner_ui) = EditorUiScope::enter(inner);
                if let Err(e) = callback.call1(py, (inner_ui,)) {
                    inner_err = Some(e);
                }
                UI_IN_USE.with(|c| c.set(true));
            });
            if let Some(e) = inner_err {
                return Err(e);
            }
            Ok(())
        })
    }

    fn vertical(&self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        with_scoped_ui(self.token, |ui| {
            let mut inner_err: Option<PyErr> = None;
            ui.vertical(|inner| {
                UI_IN_USE.with(|c| c.set(false));
                let (_guard, inner_ui) = EditorUiScope::enter(inner);
                if let Err(e) = callback.call1(py, (inner_ui,)) {
                    inner_err = Some(e);
                }
                UI_IN_USE.with(|c| c.set(true));
            });
            if let Some(e) = inner_err {
                return Err(e);
            }
            Ok(())
        })
    }
}

/// Deferred editor mutations requested by Python editor plugins through [`PyEditorContext`].
#[derive(Debug, Clone, PartialEq)]
pub enum EditorAction {
    Log(String),
    SetProp {
        node_id: String,
        key: String,
        value: PropValue,
    },
    AddNode {
        type_name: String,
        name: String,
        parent_id: Option<String>,
    },
    OpenScene(String),
    SaveScene,
}

/// Snapshot of editor state passed into [`PyEditorContext`] for a single hook invocation.
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    pub selected_node: Option<String>,
    pub scene_path: String,
    pub node_ids: Vec<String>,
}

/// Context object passed to Python editor plugins (`on_enable`, `on_bottom_panel`,
/// `on_inspector`, `on_menu_action`, `on_viewport_overlay`, `on_viewport_pointer`).
#[pyclass(crate = "pite_script::pyo3", name = "EditorContext")]
#[derive(Clone)]
pub struct PyEditorContext {
    snapshot: EditorSnapshot,
    actions: Arc<Mutex<Vec<EditorAction>>>,
}

impl PyEditorContext {
    pub fn new(snapshot: EditorSnapshot) -> (Self, Arc<Mutex<Vec<EditorAction>>>) {
        let actions = Arc::new(Mutex::new(Vec::new()));
        let ctx = Self {
            snapshot,
            actions: Arc::clone(&actions),
        };
        (ctx, actions)
    }

    fn push(&self, action: EditorAction) -> PyResult<()> {
        let mut guard = self.actions.lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("EditorContext action queue poisoned")
        })?;
        guard.push(action);
        Ok(())
    }
}

fn py_to_prop_value(val: &Bound<'_, PyAny>) -> PyResult<PropValue> {
    if let Ok(b) = val.extract::<bool>() {
        return Ok(PropValue::Bool(b));
    }
    if let Ok(i) = val.extract::<i64>() {
        return Ok(PropValue::Int(i));
    }
    if let Ok(f) = val.extract::<f64>() {
        return Ok(PropValue::Num(f));
    }
    if let Ok(s) = val.extract::<String>() {
        return Ok(PropValue::Str(s));
    }
    if let Ok((x, y)) = val.extract::<(f64, f64)>() {
        return Ok(PropValue::Vec2(x, y));
    }
    if let Ok((r, g, b, a)) = val.extract::<(u8, u8, u8, u8)>() {
        return Ok(PropValue::Color(r, g, b, a));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "unsupported property value type",
    ))
}

#[pymethods(crate = "pite_script::pyo3")]
impl PyEditorContext {
    fn selected_node(&self) -> Option<&str> {
        self.snapshot.selected_node.as_deref()
    }

    fn scene_path(&self) -> &str {
        &self.snapshot.scene_path
    }

    fn nodes(&self) -> Vec<String> {
        self.snapshot.node_ids.clone()
    }

    fn log(&self, message: String) -> PyResult<()> {
        self.push(EditorAction::Log(message))
    }

    fn set_prop(&self, node_id: String, key: String, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let pv = py_to_prop_value(value)?;
        self.push(EditorAction::SetProp {
            node_id,
            key,
            value: pv,
        })
    }

    #[pyo3(signature = (type_name, name, parent_id = None))]
    fn add_node(&self, type_name: String, name: String, parent_id: Option<String>) -> PyResult<()> {
        self.push(EditorAction::AddNode {
            type_name,
            name,
            parent_id,
        })
    }

    fn open_scene(&self, path: String) -> PyResult<()> {
        self.push(EditorAction::OpenScene(path))
    }

    fn save_scene(&self) -> PyResult<()> {
        self.push(EditorAction::SaveScene)
    }
}

struct LoadedEditorPlugin {
    id: String,
    name: String,
    instance: Py<PyAny>,
    has_bottom_panel: bool,
    has_inspector: bool,
    has_viewport_overlay: bool,
    has_viewport_pointer: bool,
    menu_items: Vec<String>,
    errored: bool,
}

/// Pointer event forwarded to editor viewport pointer hooks when not playing.
#[derive(Debug, Clone, Copy)]
pub struct ViewportPointerEvent {
    pub kind: &'static str,
    pub x: f64,
    pub y: f64,
    pub button: &'static str,
}

/// Manages editor-only Python plugin extensions (`manifest.editor`).
#[derive(Default)]
pub struct EditorPluginHost {
    plugins: Vec<LoadedEditorPlugin>,
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

impl EditorPluginHost {
    /// Clears all loaded editor plugins (used on Safe Mode or scene/project switches).
    pub fn clear(&mut self) {
        self.plugins.clear();
    }

    /// Loads `editor` entrypoints for enabled plugins in `project_dir`, returning
    /// `(errors, actions)` so the caller can log errors to Console and keep the editor open.
    pub fn load_from_project(
        &mut self,
        project_dir: &Path,
        snapshot: EditorSnapshot,
    ) -> (Vec<String>, Vec<EditorAction>) {
        self.plugins.clear();
        let mut errors = Vec::new();
        let mut actions = Vec::new();

        let Ok(manifest) = pite_project::load_manifest(project_dir) else {
            return (errors, actions);
        };
        let loaded = pite_project::load_enabled_plugins(project_dir, &manifest);
        for err in &loaded.errors {
            errors.push(err.to_string());
        }

        let editor_plugins: Vec<_> = loaded
            .plugins
            .into_iter()
            .filter(|p| p.manifest.editor.is_some())
            .collect();
        if editor_plugins.is_empty() {
            return (errors, actions);
        }

        let (ctx, queue) = PyEditorContext::new(snapshot);
        pite_script::ensure_python_init();
        Python::attach(|py| {
            let plugins_dir = project_dir.join("plugins");
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

            let ctx_py = match Bound::new(py, ctx) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(format_py_err(py, &e));
                    return;
                }
            };

            for plugin in editor_plugins {
                let Some(spec) = plugin.manifest.editor.as_deref() else {
                    continue;
                };
                match pite_script::instantiate_entrypoint(py, spec) {
                    Ok(inst) => {
                        let enable_res = if inst.hasattr("on_enable").unwrap_or(false) {
                            inst.call_method1("on_enable", (&ctx_py,))
                        } else if inst.hasattr("on_ready").unwrap_or(false) {
                            inst.call_method1("on_ready", (&ctx_py,))
                        } else {
                            Ok(py.None().into_bound(py))
                        };
                        if let Err(e) = enable_res {
                            errors.push(format!(
                                "editor plugin {}: {}",
                                plugin.id,
                                format_py_err(py, &e)
                            ));
                            continue;
                        }
                        let menu_items = if inst.hasattr("menu_items").unwrap_or(false) {
                            inst.call_method0("menu_items")
                                .and_then(|v| v.extract::<Vec<String>>())
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        let title = if plugin.manifest.name.is_empty() {
                            plugin.id.clone()
                        } else {
                            plugin.manifest.name.clone()
                        };
                        self.plugins.push(LoadedEditorPlugin {
                            id: plugin.id,
                            name: title,
                            has_bottom_panel: inst.hasattr("on_bottom_panel").unwrap_or(false),
                            has_inspector: inst.hasattr("on_inspector").unwrap_or(false),
                            has_viewport_overlay: inst
                                .hasattr("on_viewport_overlay")
                                .unwrap_or(false),
                            has_viewport_pointer: inst
                                .hasattr("on_viewport_pointer")
                                .unwrap_or(false),
                            menu_items,
                            instance: inst.unbind(),
                            errored: false,
                        });
                    }
                    Err(e) => {
                        errors.push(format!(
                            "editor plugin {}: {}",
                            plugin.id,
                            format_py_err(py, &e)
                        ));
                    }
                }
            }
        });

        if let Ok(mut guard) = queue.lock() {
            actions.append(&mut *guard);
        }
        (errors, actions)
    }

    /// Lists `(plugin_id, tab_title)` for every active plugin providing `on_bottom_panel`.
    pub fn bottom_panel_tabs(&self) -> Vec<(String, String)> {
        self.plugins
            .iter()
            .filter(|p| !p.errored && p.has_bottom_panel)
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect()
    }

    /// Lists `(plugin_id, menu_label)` for every active plugin menu action.
    pub fn menu_actions(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for p in &self.plugins {
            if p.errored {
                continue;
            }
            for item in &p.menu_items {
                out.push((p.id.clone(), item.clone()));
            }
        }
        out
    }

    /// Draws the active bottom panel tab for `plugin_id`.
    pub fn draw_bottom_panel(
        &mut self,
        plugin_id: &str,
        ui: &mut egui::Ui,
        snapshot: EditorSnapshot,
    ) -> (Vec<String>, Vec<EditorAction>) {
        let mut errors = Vec::new();
        let mut actions = Vec::new();
        let Some(plugin) = self
            .plugins
            .iter_mut()
            .find(|p| p.id == plugin_id && !p.errored && p.has_bottom_panel)
        else {
            return (errors, actions);
        };
        let (ctx, queue) = PyEditorContext::new(snapshot);
        let (_scope, ui_py) = EditorUiScope::enter(ui);
        Python::attach(|py| {
            if let Err(e) = plugin
                .instance
                .bind(py)
                .call_method1("on_bottom_panel", (ui_py, ctx))
            {
                plugin.errored = true;
                errors.push(format!(
                    "editor plugin {} on_bottom_panel: {}",
                    plugin.id,
                    format_py_err(py, &e)
                ));
            }
        });
        if let Ok(mut guard) = queue.lock() {
            actions.append(&mut *guard);
        }
        (errors, actions)
    }

    /// Draws all active plugin inspector sections below the selected node's properties.
    pub fn draw_inspector_sections(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: EditorSnapshot,
        node_id: &str,
        node_type: &str,
    ) -> (Vec<String>, Vec<EditorAction>) {
        let mut errors = Vec::new();
        let mut actions = Vec::new();
        let (ctx, queue) = PyEditorContext::new(snapshot);
        let (_scope, ui_py) = EditorUiScope::enter(ui);
        Python::attach(|py| {
            for plugin in &mut self.plugins {
                if plugin.errored || !plugin.has_inspector {
                    continue;
                }
                if let Err(e) = plugin
                    .instance
                    .bind(py)
                    .call_method1("on_inspector", (ui_py, ctx.clone(), node_id, node_type))
                {
                    plugin.errored = true;
                    errors.push(format!(
                        "editor plugin {} on_inspector: {}",
                        plugin.id,
                        format_py_err(py, &e)
                    ));
                }
            }
        });
        if let Ok(mut guard) = queue.lock() {
            actions.append(&mut *guard);
        }
        (errors, actions)
    }

    /// Dispatches a clicked menu action to the owning plugin.
    pub fn trigger_menu_action(
        &mut self,
        plugin_id: &str,
        label: &str,
        snapshot: EditorSnapshot,
    ) -> (Vec<String>, Vec<EditorAction>) {
        let mut errors = Vec::new();
        let mut actions = Vec::new();
        let Some(plugin) = self
            .plugins
            .iter_mut()
            .find(|p| p.id == plugin_id && !p.errored)
        else {
            return (errors, actions);
        };
        let (ctx, queue) = PyEditorContext::new(snapshot);
        pite_script::ensure_python_init();
        Python::attach(|py| {
            if let Err(e) = plugin
                .instance
                .bind(py)
                .call_method1("on_menu_action", (label, ctx))
            {
                plugin.errored = true;
                errors.push(format!(
                    "editor plugin {} on_menu_action: {}",
                    plugin.id,
                    format_py_err(py, &e)
                ));
            }
        });
        if let Ok(mut guard) = queue.lock() {
            actions.append(&mut *guard);
        }
        (errors, actions)
    }

    /// Draws viewport overlays for all active plugins (only called when not playing).
    pub fn draw_viewport_overlays(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: EditorSnapshot,
    ) -> (Vec<String>, Vec<EditorAction>) {
        let mut errors = Vec::new();
        let mut actions = Vec::new();
        let (ctx, queue) = PyEditorContext::new(snapshot);
        let (_scope, ui_py) = EditorUiScope::enter(ui);
        Python::attach(|py| {
            for plugin in &mut self.plugins {
                if plugin.errored || !plugin.has_viewport_overlay {
                    continue;
                }
                if let Err(e) = plugin
                    .instance
                    .bind(py)
                    .call_method1("on_viewport_overlay", (ui_py, ctx.clone()))
                {
                    plugin.errored = true;
                    errors.push(format!(
                        "editor plugin {} on_viewport_overlay: {}",
                        plugin.id,
                        format_py_err(py, &e)
                    ));
                }
            }
        });
        if let Ok(mut guard) = queue.lock() {
            actions.append(&mut *guard);
        }
        (errors, actions)
    }

    /// Dispatches a viewport pointer event to active plugins until one returns `True` (`consumed`).
    pub fn dispatch_viewport_pointer(
        &mut self,
        event: ViewportPointerEvent,
        snapshot: EditorSnapshot,
    ) -> (bool, Vec<String>, Vec<EditorAction>) {
        let mut consumed = false;
        let mut errors = Vec::new();
        let mut actions = Vec::new();
        let (ctx, queue) = PyEditorContext::new(snapshot);
        pite_script::ensure_python_init();
        Python::attach(|py| {
            let ev_dict = pyo3::types::PyDict::new(py);
            let _ = ev_dict.set_item("kind", event.kind);
            let _ = ev_dict.set_item("x", event.x);
            let _ = ev_dict.set_item("y", event.y);
            let _ = ev_dict.set_item("button", event.button);
            for plugin in &mut self.plugins {
                if plugin.errored || !plugin.has_viewport_pointer {
                    continue;
                }
                match plugin
                    .instance
                    .bind(py)
                    .call_method1("on_viewport_pointer", (&ev_dict, ctx.clone()))
                {
                    Ok(res) => {
                        if res.extract::<bool>().unwrap_or(false) {
                            consumed = true;
                            break;
                        }
                    }
                    Err(e) => {
                        plugin.errored = true;
                        errors.push(format!(
                            "editor plugin {} on_viewport_pointer: {}",
                            plugin.id,
                            format_py_err(py, &e)
                        ));
                    }
                }
            }
        });
        if let Ok(mut guard) = queue.lock() {
            actions.append(&mut *guard);
        }
        (consumed, errors, actions)
    }
}
