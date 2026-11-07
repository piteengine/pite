// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-script`: `ScriptBackend` trait. PyO3 is the only impl;
//! future backends (or a mock for tests) plug in without touching runtime.

mod audio;
pub mod descriptor;
mod host;
mod input;
pub mod physics_bridge;
mod python;
pub mod vec2;

use anyhow::Result;

pub use descriptor::{child, node, PyChildDescriptor, PyNodeDescriptor};
pub use host::{
    fire_area_entered, fire_area_exited, fire_body_entered, fire_body_exited, fire_pressed,
    fire_timeout, proxy_for, resolve_caller, resolve_caller_py, set_current_host, NodeProxy,
    ScriptHost,
};
pub use input::{
    input_begin_frame, input_clear, input_held, input_mouse, input_pressed, input_released,
    input_set_key, input_set_mouse,
};
#[doc(hidden)]
pub use paste;
pub use physics_bridge::{current_physics, set_current_physics, with_physics};
pub use pyo3;
pub use python::{is_stdlib_module, InputQueryKind, PyInputQuery, Pyo3Backend, ScriptError};
pub use vec2::Vec2;

pub const RESERVED_METHODS: &[&str] = &["emit", "connect"];

/// Project root used to resolve `res://` asset refs from Python.
static PROJECT_DIR: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

pub fn set_project_dir(dir: Option<std::path::PathBuf>) {
    *PROJECT_DIR.lock().expect("project dir lock") = dir;
}

pub fn project_dir() -> Option<std::path::PathBuf> {
    PROJECT_DIR.lock().expect("project dir lock").clone()
}

pub fn is_reserved(name: &str) -> bool {
    RESERVED_METHODS.contains(&name)
}

/// Backend interface consumed by `pite-runtime`. `Pyo3Backend` is the only
/// impl today; `NoopBackend` exists for headless tests.
pub trait ScriptBackend {
    /// Load a user script file and instantiate `class` from it.
    fn load(&mut self, path: &str, class: &str) -> Result<()>;

    /// Instantiate the engine's own pyclass for `type_name` (e.g.
    /// `"Sprite2D"` → `pite.Sprite2D`), with no user code attached. Used
    /// for scriptless nodes so engine-owned behaviors (Timer.timeout,
    /// Button.pressed in a future commit) can live on the pyclass.
    ///
    /// Defaults to an error: backends that only drive user scripts (mocks
    /// in tests, future non-Python bindings) opt out entirely, and the
    /// runtime falls back to no slot for the affected node.
    fn load_base(&mut self, _type_name: &str) -> Result<()> {
        anyhow::bail!("this backend cannot instantiate base pyclasses")
    }

    fn call_ready(&mut self) -> Result<()>;
    fn call_process(&mut self, delta: f64) -> Result<()>;
    fn call_physics_process(&mut self, _delta: f64) -> Result<()> {
        Ok(())
    }
    fn reload(&mut self) -> Result<()>;

    fn last_error(&self) -> Option<String> {
        None
    }

    fn last_script_error(&self) -> Option<ScriptError> {
        None
    }

    /// Whether the *instance* exposes `_ready` / `_process` /
    /// `_physics_process` (inherited methods count — the probe walks the
    /// class hierarchy). Defaults to `true` so backends that don't track
    /// this still get called; `Pyo3Backend` overrides with a real probe at
    /// instantiation time.
    fn has_ready(&self) -> bool {
        true
    }
    fn has_process(&self) -> bool {
        true
    }
    fn has_physics_process(&self) -> bool {
        true
    }
}

#[derive(Debug, Default)]
pub struct NoopBackend;

impl ScriptBackend for NoopBackend {
    fn load(&mut self, _path: &str, _class: &str) -> Result<()> {
        Ok(())
    }

    fn call_ready(&mut self) -> Result<()> {
        Ok(())
    }

    fn call_process(&mut self, _delta: f64) -> Result<()> {
        Ok(())
    }

    fn reload(&mut self) -> Result<()> {
        Ok(())
    }
}
