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

    // -- custom properties ---------------------------------------------------

    /// Read a custom (non-schema) prop declared in `[props]` or set via
    /// `set_custom_property`. Returns `None` when unset.
    ///
    /// Passing a schema prop key (`position`, `text`, ...) raises
    /// `ValueError`: schema props have their own typed accessor, and
    /// silently routing through this method would let a caller bypass the
    /// type invariant.
    fn get_custom_property(slf: &Bound<'_, Self>, key: &str) -> PyResult<Option<Py<PyAny>>> {
        let (host, node) = crate::python::host_and_path(slf.as_any())?;
        let value = host
            .get_custom_property(&node, key)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        match value {
            None => Ok(None),
            Some(v) => Python::attach(|py| crate::python::props::prop_to_py(py, &v)).map(Some),
        }
    }

    /// Write a custom prop. The key must be declared in `[props]` (a project
    /// manifest section) with a matching type; both the declaration check and
    /// the type check run inside `ScriptHost` and raise `ValueError` on
    /// mismatch. Writing a schema prop key is an error — use its accessor.
    fn set_custom_property(
        slf: &Bound<'_, Self>,
        key: &str,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let (host, node) = crate::python::host_and_path(slf.as_any())?;
        let prop = crate::python::props::py_to_prop(value)?;
        host.set_custom_property(&node, key, prop)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Remove a custom prop. Returns `true` if it was present. Refusing
    /// schema prop keys keeps the invariant intact (schema props cannot be
    /// removed, only reset to their schema default via the editor).
    fn remove_custom_property(slf: &Bound<'_, Self>, key: &str) -> PyResult<bool> {
        let (host, node) = crate::python::host_and_path(slf.as_any())?;
        host.remove_custom_property(&node, key)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}
