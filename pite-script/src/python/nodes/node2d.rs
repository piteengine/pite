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

    #[getter]
    fn position(&self, py: Python<'_>) -> Py<crate::vec2::Vec2> {
        self.position.clone_ref(py)
    }

    #[setter]
    fn set_position(&self, py: Python<'_>, val: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut b = self.position.borrow_mut(py);
        if let Ok(v) = val.extract::<crate::vec2::Vec2>() {
            b.set_x(v.x());
            b.set_y(v.y());
        } else if let Ok((x, y)) = val.extract::<(f64, f64)>() {
            b.set_x(x);
            b.set_y(y);
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "position must be Vec2 or (x, y) tuple",
            ));
        }
        Ok(())
    }
}
