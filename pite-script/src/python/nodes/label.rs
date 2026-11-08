// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "Label")]
pub(crate) struct PyLabel;

#[pymethods]
impl PyLabel {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyLabel))
    }
}

crate::pite_props! {
    PyLabel {
        text: Str,
        font_size: Num,
        color: Color,
    }
}
