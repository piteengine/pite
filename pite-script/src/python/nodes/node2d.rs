// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;

#[pyclass(extends = PyNode, subclass, name = "Node2D")]
pub(crate) struct PyNode2D {
    pub(crate) position: Py<crate::vec2::Vec2>,
}

#[pymethods]
impl PyNode2D {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new()).add_subclass(PyNode2D { position: pos }))
    }
}

crate::pite_props! {
    PyNode2D {
        position: Vec2,
    }
}
