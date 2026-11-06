// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;

#[pyclass(extends = PyNode, subclass, name = "Label")]
pub(crate) struct PyLabel {
    #[pyo3(get)]
    text: String,
}

#[pymethods]
impl PyLabel {
    #[new]
    pub(crate) fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyNode::new()).add_subclass(PyLabel {
            text: String::new(),
        })
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
}
