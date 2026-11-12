// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "StaticBody2D")]
pub(crate) struct PyStaticBody2D;

#[pymethods]
impl PyStaticBody2D {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyStaticBody2D))
    }
}

crate::pite_props! {
    PyStaticBody2D {
        collision_layer: Int,
        collision_mask: Int,
        friction: Num,
        restitution: Num,
    }
}
