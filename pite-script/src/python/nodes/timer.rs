// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;

#[pyclass(extends = PyNode, subclass, name = "Timer")]
pub(crate) struct PyTimer;

#[pymethods]
impl PyTimer {
    #[new]
    pub(crate) fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyNode::new()).add_subclass(PyTimer)
    }

    /// Advance `current_elapsed` by `delta`, wrap and fire `timeout` on
    /// elapsed. Inherited by `RepeatingTimer(pite.Timer)` etc.; a subclass
    /// that overrides `_process` **must** call `super()._process(delta)` to
    /// keep ticking (same footgun class as Godot's `CharacterBody2D`).
    ///
    /// `wait_time <= 0` disables the timer entirely (never fires), matching
    /// the previous runtime tick.
    fn _process(slf: &Bound<'_, Self>, delta: f64) -> PyResult<()> {
        let py = slf.py();
        let (host, node) = crate::python::host_and_path(slf.as_any())?;
        let wait = host.get_f64(&node, "wait_time");
        if wait <= 0.0 {
            return Ok(());
        }
        let mut elapsed = host.get_f64(&node, "current_elapsed");
        elapsed += delta;
        if elapsed >= wait {
            // Reset BEFORE emit so reentrant handlers observe the reset state.
            host.set_prop(&node, "current_elapsed", pite_core::PropValue::Num(0.0));
            let empty = pyo3::types::PyTuple::empty(py);
            let _ = host.emit(py, &node, "timeout", &empty);
        } else {
            host.set_prop(&node, "current_elapsed", pite_core::PropValue::Num(elapsed));
        }
        Ok(())
    }

    #[getter]
    fn timeout(slf: &Bound<'_, Self>) -> PyResult<crate::host::BoundSignal> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let host = crate::host::current_host().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("signal access has no script host")
        })?;
        Ok(crate::host::BoundSignal::with_host(
            host,
            node_path,
            "timeout".to_string(),
        ))
    }
}

crate::pite_props! {
    PyTimer {
        wait_time: Num,
        current_elapsed: Num,
    }
}
