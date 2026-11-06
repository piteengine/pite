// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "CollisionShape2D")]
pub(crate) struct PyCollisionShape2D {
    #[pyo3(get, set)]
    shape: String,
    #[pyo3(get, set)]
    size: (f64, f64),
    #[pyo3(get, set)]
    radius: f64,
    #[pyo3(get, set)]
    height: f64,
    #[pyo3(get, set)]
    disabled: bool,
}

#[pymethods]
impl PyCollisionShape2D {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyCollisionShape2D {
                shape: "box".to_string(),
                size: (32.0, 32.0),
                radius: 16.0,
                height: 32.0,
                disabled: false,
            }))
    }
}
