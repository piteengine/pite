// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "Button")]
pub(crate) struct PyButton {
    pub(crate) size: Py<crate::vec2::Vec2>,
}

#[pymethods]
impl PyButton {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        let size = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyButton { size }))
    }

    #[getter]
    fn pressed(slf: &Bound<'_, Self>) -> PyResult<crate::host::BoundSignal> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let host = crate::host::current_host().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("signal access has no script host")
        })?;
        Ok(crate::host::BoundSignal::with_host(
            host,
            node_path,
            "pressed".to_string(),
        ))
    }
}

crate::pite_props! {
    PyButton {
        text: Str,
        font_size: Num,
        color: Color,
        size: Vec2,
        disabled: Bool,
    }
}
