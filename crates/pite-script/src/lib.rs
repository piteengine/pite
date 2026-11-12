// SPDX-License-Identifier: MIT OR Apache-2.0
//! Embedded Python 3.12 scripting bridge and gameplay API for the Pite game engine.
//!
//! Powered by PyO3, this crate implements the `ScriptBackend` trait to drive script
//! lifecycle hooks (`_ready`, `_process`, `_physics_process`), provide the public `pite`
//! Python module, and facilitate zero-overhead bidirectional communication between
//! Rust engine nodes and Python gameplay scripts.

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
    fire_animation_finished, fire_animation_looped, fire_area_entered, fire_area_exited,
    fire_body_entered, fire_body_exited, proxy_for, resolve_caller, resolve_caller_py,
    set_current_host, NodeProxy, ScriptHost,
};
pub use input::{
    input_begin_frame, input_clear, input_held, input_mouse, input_pressed, input_released,
    input_set_key, input_set_mouse, screen_to_world, set_camera_view,
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

pub trait ScriptBackend {
    fn load(&mut self, path: &str, class: &str) -> Result<()>;
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
