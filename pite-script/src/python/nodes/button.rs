// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "Button")]
pub(crate) struct PyButton {
    #[pyo3(get)]
    text: String,
    size: Py<crate::vec2::Vec2>,
}

#[pymethods]
impl PyButton {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        let size = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyButton {
                text: String::new(),
                size,
            }))
    }

    #[setter]
    fn set_text(slf: &Bound<'_, Self>, text: String) -> PyResult<()> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        if let Some(host) = crate::host::current_host() {
            host.set_node_text(&node_path, &text);
        }
        let mut b = slf.borrow_mut();
        b.text = text;
        Ok(())
    }

    #[getter]
    fn size(&self, py: Python<'_>) -> Py<crate::vec2::Vec2> {
        self.size.clone_ref(py)
    }

    #[setter]
    fn set_size(slf: &Bound<'_, Self>, val: &Bound<'_, PyAny>) -> PyResult<()> {
        let (w, h) = if let Ok(v) = val.extract::<crate::vec2::Vec2>() {
            (v.x(), v.y())
        } else if let Ok(t) = val.extract::<(f64, f64)>() {
            t
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "size must be Vec2 or (w, h) tuple",
            ));
        };
        let node_path: String = slf.getattr("node_path")?.extract()?;
        if let Some(host) = crate::host::current_host() {
            host.set_node_size(&node_path, w, h);
        }
        let py = slf.py();
        let b = slf.borrow();
        let mut s = b.size.borrow_mut(py);
        s.set_x(w);
        s.set_y(h);
        Ok(())
    }
}
