//! `pite-script`: `ScriptBackend` trait. PyO3 is the only impl (M1b);
//! future backends (or a mock for tests) plug in without touching runtime.

mod host;
mod input;
mod python;

use anyhow::Result;

pub use host::{proxy_for, resolve_caller, NodeProxy, ScriptHost, set_current_host};
pub use input::{
    input_begin_frame, input_held, input_mouse, input_pressed, input_released, input_set_key,
    input_set_mouse,
};
pub use python::Pyo3Backend;

pub const RESERVED_METHODS: &[&str] = &["emit", "connect"];

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
