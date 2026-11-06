// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;

#[pyclass(extends = PyNode, subclass, name = "Timer")]
pub(crate) struct PyTimer {
    #[pyo3(get, set)]
    wait_time: f64,
}

#[pymethods]
impl PyTimer {
    #[new]
    pub(crate) fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyNode::new()).add_subclass(PyTimer { wait_time: 1.0 })
    }
}
