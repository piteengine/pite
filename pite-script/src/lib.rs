// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-script`: `ScriptBackend` trait. PyO3 is the only impl;
//! future backends (or a mock for tests) plug in without touching runtime.

mod audio;
mod host;
mod input;
mod python;

use anyhow::Result;

pub use host::{fire_pressed, proxy_for, resolve_caller, set_current_host, NodeProxy, ScriptHost};
pub use input::{
    input_begin_frame, input_clear, input_held, input_mouse, input_pressed, input_released,
    input_set_key, input_set_mouse,
};
pub use python::Pyo3Backend;

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
    fn call_ready(&mut self) -> Result<()>;
    fn call_process(&mut self, delta: f64) -> Result<()>;
    fn reload(&mut self) -> Result<()>;

    fn position(&self) -> Option<(f64, f64)> {
        None
    }

    fn set_position(&mut self, _x: f64, _y: f64) {}

    fn text(&self) -> Option<String> {
        None
    }

    fn set_text(&mut self, _text: &str) {}

    fn last_error(&self) -> Option<String> {
        None
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
