// SPDX-License-Identifier: MIT OR Apache-2.0
//! Declarative node descriptors for Python scripts (`pite.child` and `pite.node`).

use pyo3::prelude::*;

/// Python property descriptor resolving a direct child node by name on first access with caching.
#[pyclass(name = "ChildDescriptor")]
pub struct PyChildDescriptor {
    name: String,
    attr: Option<String>,
}

#[pymethods]
impl PyChildDescriptor {
    #[new]
    fn new(name: String) -> Self {
        Self { name, attr: None }
    }

    fn __set_name__(&mut self, _owner: &Bound<'_, PyAny>, name: String) {
        self.attr = Some(name);
    }

    fn __get__(
        &self,
        py: Python<'_>,
        obj: Option<&Bound<'_, PyAny>>,
        _owner: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let Some(obj) = obj else {
            return Ok(Bound::new(
                py,
                PyChildDescriptor {
                    name: self.name.clone(),
                    attr: self.attr.clone(),
                },
            )?
            .into_any()
            .unbind());
        };

        let cache = match obj.getattr("_descriptor_cache") {
            Ok(c) => c,
            Err(_) => {
                let dict = pyo3::types::PyDict::new(py);
                obj.setattr("_descriptor_cache", &dict)?;
                dict.into_any()
            }
        };

        let attr_key = self.attr.as_deref().unwrap_or(self.name.as_str());
        if let Ok(cached) = cache.get_item(attr_key) {
            return Ok(cached.into_any().unbind());
        }

        let resolved = obj.call_method1("get_node", (&self.name,))?;
        cache.set_item(attr_key, &resolved)?;
        Ok(resolved.into())
    }
}

/// Python property descriptor resolving a scene node by relative or absolute path on first access.
#[pyclass(name = "NodeDescriptor")]
pub struct PyNodeDescriptor {
    path: String,
    attr: Option<String>,
}

#[pymethods]
impl PyNodeDescriptor {
    #[new]
    fn new(path: String) -> Self {
        Self { path, attr: None }
    }

    fn __set_name__(&mut self, _owner: &Bound<'_, PyAny>, name: String) {
        self.attr = Some(name);
    }

    fn __get__(
        &self,
        py: Python<'_>,
        obj: Option<&Bound<'_, PyAny>>,
        _owner: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let Some(obj) = obj else {
            return Ok(Bound::new(
                py,
                PyNodeDescriptor {
                    path: self.path.clone(),
                    attr: self.attr.clone(),
                },
            )?
            .into_any()
            .unbind());
        };

        let cache = match obj.getattr("_descriptor_cache") {
            Ok(c) => c,
            Err(_) => {
                let dict = pyo3::types::PyDict::new(py);
                obj.setattr("_descriptor_cache", &dict)?;
                dict.into_any()
            }
        };

        let attr_key = self.attr.as_deref().unwrap_or(self.path.as_str());
        if let Ok(cached) = cache.get_item(attr_key) {
            return Ok(cached.into_any().unbind());
        }

        let resolved = obj.call_method1("get_node", (&self.path,))?;
        cache.set_item(attr_key, &resolved)?;
        Ok(resolved.into())
    }
}

/// Creates a declarative descriptor that resolves a child node by name on first attribute access.
#[pyfunction]
pub fn child(name: String) -> PyChildDescriptor {
    PyChildDescriptor::new(name)
}

/// Creates a declarative descriptor that resolves a node by path on first attribute access.
#[pyfunction]
pub fn node(path: String) -> PyNodeDescriptor {
    PyNodeDescriptor::new(path)
}

/// Live data descriptor attached to plugin-defined node classes for `[node.props]` fields.
///
/// Reads and writes directly against the node's `props` map in the active `ScriptHost`,
/// keeping `self.<prop>` and `proxy.<prop>` synchronized with scene and inspector values.
#[pyclass(name = "PluginPropDescriptor")]
#[derive(Clone)]
pub struct PyPluginPropDescriptor {
    key: String,
    hint: pite_core::PropHintSpec,
    default: pite_core::PropValue,
}

#[pymethods]
impl PyPluginPropDescriptor {
    fn __get__(
        &self,
        py: Python<'_>,
        obj: Option<&Bound<'_, PyAny>>,
        _owner: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let Some(obj) = obj else {
            return Ok(Bound::new(py, self.clone())?.into_any().unbind());
        };
        let node_path: String = obj.getattr("node_path")?.extract()?;
        let val = crate::host::current_host()
            .and_then(|host| host.prop_value_or_default(&node_path, &self.key))
            .unwrap_or_else(|| self.default.clone());
        crate::host::proxy::prop_val_to_py(py, &val)
    }

    fn __set__(&self, obj: &Bound<'_, PyAny>, val: &Bound<'_, PyAny>) -> PyResult<()> {
        let node_path: String = obj.getattr("node_path")?.extract()?;
        let prop_val = crate::host::proxy::prop_val_from_py(&self.hint, val)?;
        let host = crate::host::current_host().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "cannot set plugin prop {:?} outside an active script host",
                self.key
            ))
        })?;
        host.with_tree_mut(|tree| {
            if let Some(node) = tree.get_mut(&pite_core::NodeId::from(node_path)) {
                node.props.insert(self.key.clone(), prop_val);
            }
        });
        Ok(())
    }
}

/// Attaches [`PyPluginPropDescriptor`] attributes onto `cls` for any schema property not
/// already explicitly defined on the Python class.
pub fn attach_plugin_prop_descriptors(
    py: Python<'_>,
    cls: &Bound<'_, PyAny>,
    props: &[pite_core::PropDef],
) -> PyResult<()> {
    for prop in props {
        if cls.hasattr(prop.key.as_ref())? {
            continue;
        }
        let desc = Bound::new(
            py,
            PyPluginPropDescriptor {
                key: prop.key.to_string(),
                hint: prop.hint.clone(),
                default: prop.default_value.clone(),
            },
        )?;
        cls.setattr(prop.key.as_ref(), desc)?;
    }
    Ok(())
}
