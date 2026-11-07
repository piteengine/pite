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
    }
}
