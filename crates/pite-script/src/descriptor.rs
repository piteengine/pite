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
