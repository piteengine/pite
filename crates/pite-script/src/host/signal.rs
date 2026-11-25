// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bound Python signal handle and engine-owned physics/animation signal emitters.

use pyo3::prelude::*;
use pyo3::types::PyTuple;

use super::{bound_target, ScriptHost};

#[pyclass(name = "BoundSignal")]
pub struct BoundSignal {
    host: std::sync::Arc<ScriptHost>,
    node: String,
    signal: String,
}

impl BoundSignal {
    pub fn with_host(host: std::sync::Arc<ScriptHost>, node: String, signal: String) -> Self {
        Self { host, node, signal }
    }
}

#[pymethods]
impl BoundSignal {
    fn connect(&self, py: Python<'_>, callable: Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok((target, handler)) = bound_target(&callable) {
            self.host
                .connect(&self.node, &self.signal, &target, &handler)
                .map_err(|e| pyo3::exceptions::PyKeyError::new_err(e.to_string()))
        } else {
            self.host
                .connect_callable(py, &self.node, &self.signal, callable.into_any().unbind())
                .map_err(|e| pyo3::exceptions::PyKeyError::new_err(e.to_string()))
        }
    }

    #[pyo3(signature = (*args))]
    fn emit(&self, py: Python<'_>, args: &Bound<'_, PyTuple>) -> PyResult<()> {
        self.host
            .emit(py, &self.node, &self.signal, args)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

fn emit_single_arg_signal(
    host: &ScriptHost,
    node_id: &str,
    signal: &str,
    arg: &str,
) -> anyhow::Result<()> {
    Python::attach(|py| {
        let arg = PyTuple::new(py, [arg])?;
        host.emit(py, node_id, signal, &arg)
    })
}

pub fn fire_body_entered(host: &ScriptHost, area_id: &str, body_id: &str) -> anyhow::Result<()> {
    emit_single_arg_signal(host, area_id, "body_entered", body_id)
}

pub fn fire_body_exited(host: &ScriptHost, area_id: &str, body_id: &str) -> anyhow::Result<()> {
    emit_single_arg_signal(host, area_id, "body_exited", body_id)
}

pub fn fire_area_entered(
    host: &ScriptHost,
    area_id: &str,
    other_area_id: &str,
) -> anyhow::Result<()> {
    emit_single_arg_signal(host, area_id, "area_entered", other_area_id)
}

pub fn fire_area_exited(
    host: &ScriptHost,
    area_id: &str,
    other_area_id: &str,
) -> anyhow::Result<()> {
    emit_single_arg_signal(host, area_id, "area_exited", other_area_id)
}

pub fn fire_animation_finished(
    host: &ScriptHost,
    node_id: &str,
    anim_name: &str,
) -> anyhow::Result<()> {
    emit_single_arg_signal(host, node_id, "animation_finished", anim_name)
}

pub fn fire_animation_looped(
    host: &ScriptHost,
    node_id: &str,
    anim_name: &str,
) -> anyhow::Result<()> {
    emit_single_arg_signal(host, node_id, "animation_looped", anim_name)
}
