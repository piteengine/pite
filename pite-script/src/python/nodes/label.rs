// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;

#[pyclass(extends = PyNode, subclass, name = "Label")]
pub(crate) struct PyLabel;

#[pymethods]
impl PyLabel {
    #[new]
    pub(crate) fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyNode::new()).add_subclass(PyLabel)
    }
}

crate::pite_props! {
    PyLabel {
        text: Str,
        font_size: Num,
        color: Color,
    }
}
