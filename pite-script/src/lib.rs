//! `pite-script`: `ScriptBackend` trait. PyO3 is the only impl (M1b);
//! future backends (or a mock for tests) plug in without touching runtime.

use anyhow::Result;

pub const RESERVED_METHODS: &[&str] = &["emit", "connect"];

pub fn is_reserved(name: &str) -> bool {
    RESERVED_METHODS.contains(&name)
}

pub trait ScriptBackend {
    fn load(&mut self, path: &str, class: &str) -> Result<()>;
    fn call_ready(&mut self) -> Result<()>;
    fn call_process(&mut self, delta: f64) -> Result<()>;
    fn reload(&mut self) -> Result<()>;
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
