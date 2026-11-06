// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;

use crate::host::NodeProxy;

#[pyclass(subclass, name = "Node")]
pub(crate) struct PyNode {
    #[pyo3(get, set)]
    node_path: String,
}

#[pymethods]
impl PyNode {
    #[new]
    pub(crate) fn new() -> Self {
        Self {
            node_path: String::new(),
        }
    }

    fn get_node(slf: &Bound<'_, Self>, path: &str) -> PyResult<Py<NodeProxy>> {
        let py = slf.py();
        let node_path = slf.borrow().node_path.clone();
        crate::resolve_caller_py(py, &node_path, path)
            .map_err(|e| pyo3::exceptions::PyKeyError::new_err(e.to_string()))
    }
}
