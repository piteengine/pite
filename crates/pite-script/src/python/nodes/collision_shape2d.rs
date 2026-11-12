// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "CollisionShape2D")]
pub(crate) struct PyCollisionShape2D {
    pub(crate) size: Py<crate::vec2::Vec2>,
}

#[pymethods]
impl PyCollisionShape2D {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        let size = Py::new(py, crate::vec2::Vec2::new(32.0, 32.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyCollisionShape2D { size }))
    }
}

crate::pite_props! {
    PyCollisionShape2D {
        shape: Str,
        size: Vec2,
        radius: Num,
        height: Num,
        disabled: Bool,
    }
}
