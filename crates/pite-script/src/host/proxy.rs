// SPDX-License-Identifier: MIT OR Apache-2.0
//! Python proxy type for cross-node calls and property access.

use pite_core::NodeId;
use pyo3::prelude::*;

use super::{current_host, BoundSignal, ScriptHost};

#[pyclass(name = "NodeProxy")]
pub struct NodeProxy {
    host: std::sync::Arc<ScriptHost>,
    target: String,
}

#[pymethods]
impl NodeProxy {
    #[getter]
    fn position(&self) -> PyResult<crate::vec2::Vec2> {
        use pite_core::PropValue;
        self.host.with_tree(|tree| {
            let node = tree
                .get(&NodeId::from(self.target.clone()))
                .ok_or_else(|| {
                    pyo3::exceptions::PyKeyError::new_err(format!(
                        "node {:?} no longer exists",
                        self.target
                    ))
                })?;
            match node.props.get("position") {
                Some(PropValue::Vec2(x, y)) => Ok(crate::vec2::Vec2::new(*x, *y)),
                _ => Ok(crate::vec2::Vec2::new(0.0, 0.0)),
            }
        })
    }

    #[setter]
    fn set_position(&self, val: &Bound<'_, PyAny>) -> PyResult<()> {
        let (x, y) = if let Ok(v) = val.extract::<crate::vec2::Vec2>() {
            (v.x, v.y)
        } else if let Ok(t) = val.extract::<(f64, f64)>() {
            t
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "position must be Vec2 or (x, y) tuple",
            ));
        };
        use pite_core::PropValue;
        self.host.with_tree_mut(|tree| {
            if let Some(node) = tree.get_mut(&NodeId::from(self.target.clone())) {
                node.props
                    .insert("position".to_string(), PropValue::Vec2(x, y));
            }
        });
        Ok(())
    }

    #[getter]
    fn text(&self) -> PyResult<String> {
        use pite_core::PropValue;
        self.host.with_tree(|tree| {
            let node = tree
                .get(&NodeId::from(self.target.clone()))
                .ok_or_else(|| {
                    pyo3::exceptions::PyKeyError::new_err(format!(
                        "node {:?} no longer exists",
                        self.target
                    ))
                })?;
            match node.props.get("text") {
                Some(PropValue::Str(s)) => Ok(s.clone()),
                _ => Ok(String::new()),
            }
        })
    }

    #[setter]
    fn set_text(&self, text: String) {
        use pite_core::PropValue;
        self.host.with_tree_mut(|tree| {
            if let Some(node) = tree.get_mut(&NodeId::from(self.target.clone())) {
                node.props.insert("text".to_string(), PropValue::Str(text));
            }
        });
    }

    #[getter]
    fn size(&self) -> PyResult<crate::vec2::Vec2> {
        use pite_core::PropValue;
        self.host.with_tree(|tree| {
            let node = tree
                .get(&NodeId::from(self.target.clone()))
                .ok_or_else(|| {
                    pyo3::exceptions::PyKeyError::new_err(format!(
                        "node {:?} no longer exists",
                        self.target
                    ))
                })?;
            match node.props.get("size") {
                Some(PropValue::Vec2(w, h)) => Ok(crate::vec2::Vec2::new(*w, *h)),
                _ => Ok(crate::vec2::Vec2::new(0.0, 0.0)),
            }
        })
    }

    #[setter]
    fn set_size(&self, val: &Bound<'_, PyAny>) -> PyResult<()> {
        let (w, h) = if let Ok(v) = val.extract::<crate::vec2::Vec2>() {
            (v.x, v.y)
        } else if let Ok(t) = val.extract::<(f64, f64)>() {
            t
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "size must be Vec2 or (w, h) tuple",
            ));
        };
        use pite_core::PropValue;
        self.host.with_tree_mut(|tree| {
            if let Some(node) = tree.get_mut(&NodeId::from(self.target.clone())) {
                node.props.insert("size".to_string(), PropValue::Vec2(w, h));
            }
        });
        Ok(())
    }

    #[getter]
    fn color(&self) -> PyResult<(u8, u8, u8, u8)> {
        use pite_core::PropValue;
        self.host.with_tree(|tree| {
            let node = tree
                .get(&NodeId::from(self.target.clone()))
                .ok_or_else(|| {
                    pyo3::exceptions::PyKeyError::new_err(format!(
                        "node {:?} no longer exists",
                        self.target
                    ))
                })?;
            match node.props.get("color") {
                Some(PropValue::Color(r, g, b, a)) => Ok((*r, *g, *b, *a)),
                _ => Ok((255, 255, 255, 255)),
            }
        })
    }

    #[setter]
    fn set_color(&self, val: &Bound<'_, PyAny>) -> PyResult<()> {
        let (r, g, b, a) = if let Ok(s) = val.extract::<String>() {
            let c = pite_scene::parse_hex_color(&s).ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid hex color: {s}"))
            })?;
            (c.r, c.g, c.b, c.a)
        } else if let Ok(rgba) = val.extract::<(u8, u8, u8, u8)>() {
            rgba
        } else if let Ok(rgb) = val.extract::<(u8, u8, u8)>() {
            (rgb.0, rgb.1, rgb.2, 255)
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "color must be a hex string (e.g. '#EF4444') or (r, g, b[, a]) tuple",
            ));
        };
        use pite_core::PropValue;
        self.host.with_tree_mut(|tree| {
            if let Some(node) = tree.get_mut(&NodeId::from(self.target.clone())) {
                node.props
                    .insert("color".to_string(), PropValue::Color(r, g, b, a));
            }
        });
        Ok(())
    }

    fn __setattr__(&self, py: Python<'_>, name: String, val: Bound<'_, PyAny>) -> PyResult<()> {
        if name == "position" {
            return self.set_position(&val);
        }
        if name == "text" {
            let s = val.extract::<String>()?;
            self.set_text(s);
            return Ok(());
        }
        if name == "size" {
            return self.set_size(&val);
        }
        if name == "color" {
            return self.set_color(&val);
        }

        // Try setting on attached Python script instance first (e.g. player.speed = 0.0)
        if let Some(inst) = self.host.lookup_instance(py, &self.target) {
            if inst.bind(py).hasattr(name.as_str())? {
                return inst.bind(py).setattr(name.as_str(), val);
            }
        }

        // Check if it's a schema property on the node
        let node_type = self.host.with_tree(|tree| {
            tree.get(&NodeId::from(self.target.clone()))
                .map(|n| n.type_name.clone())
        });
        if let Some(type_name) = node_type {
            let schemas = self.host.schema_registry();
            let schema_props = schemas.all_props(&type_name);
            if let Some(prop_schema) = schema_props.iter().find(|p| p.key == name) {
                let prop_val = match &prop_schema.hint {
                    pite_core::PropHintSpec::Color => {
                        let (r, g, b, a) = if let Ok(s) = val.extract::<String>() {
                            let c = pite_scene::parse_hex_color(&s).ok_or_else(|| {
                                pyo3::exceptions::PyValueError::new_err(format!(
                                    "invalid hex color: {s}"
                                ))
                            })?;
                            (c.r, c.g, c.b, c.a)
                        } else if let Ok(rgba) = val.extract::<(u8, u8, u8, u8)>() {
                            rgba
                        } else if let Ok(rgb) = val.extract::<(u8, u8, u8)>() {
                            (rgb.0, rgb.1, rgb.2, 255)
                        } else {
                            return Err(pyo3::exceptions::PyTypeError::new_err(
                                "color must be hex string or tuple",
                            ));
                        };
                        pite_core::PropValue::Color(r, g, b, a)
                    }
                    pite_core::PropHintSpec::Num { .. } | pite_core::PropHintSpec::AngleDeg => {
                        let n = val.extract::<f64>()?;
                        pite_core::PropValue::Num(n)
                    }
                    pite_core::PropHintSpec::Int { .. } => {
                        let i = val.extract::<i64>()?;
                        pite_core::PropValue::Int(i)
                    }
                    pite_core::PropHintSpec::Bool => {
                        let b = val.extract::<bool>()?;
                        pite_core::PropValue::Bool(b)
                    }
                    pite_core::PropHintSpec::Vec2 { .. } => {
                        let (x, y) = if let Ok(v) = val.extract::<crate::vec2::Vec2>() {
                            (v.x(), v.y())
                        } else if let Ok(t) = val.extract::<(f64, f64)>() {
                            t
                        } else {
                            return Err(pyo3::exceptions::PyTypeError::new_err(
                                "expected Vec2 or (x, y)",
                            ));
                        };
                        pite_core::PropValue::Vec2(x, y)
                    }
                    _ => {
                        let s = val.extract::<String>()?;
                        pite_core::PropValue::Str(s)
                    }
                };
                self.host.with_tree_mut(|tree| {
                    if let Some(node) = tree.get_mut(&NodeId::from(self.target.clone())) {
                        node.props.insert(name.clone(), prop_val);
                    }
                });
                return Ok(());
            }
        }

        // If instance exists, set dynamic attribute on it
        if let Some(inst) = self.host.lookup_instance(py, &self.target) {
            return inst.bind(py).setattr(name.as_str(), val);
        }

        Err(pyo3::exceptions::PyAttributeError::new_err(format!(
            "node {:?} has no property or script attribute {:?}",
            self.target, name
        )))
    }

    fn __getattr__(&self, py: Python<'_>, name: String) -> PyResult<Py<PyAny>> {
        if name == "position" {
            return Ok(Bound::new(py, self.position()?)?.into_any().unbind());
        }
        if name == "text" {
            return Ok(self.text()?.into_pyobject(py)?.into_any().unbind());
        }
        if name == "size" {
            return Ok(Bound::new(py, self.size()?)?.into_any().unbind());
        }
        if name == "color" {
            return Ok(self.color()?.into_pyobject(py)?.into_any().unbind());
        }
        if self.host.has_signal(&self.target, &name) {
            let bound = BoundSignal::with_host(self.host.clone(), self.target.clone(), name);
            return Ok(Bound::new(py, bound)?.into_any().unbind());
        }

        if let Some(inst) = self.host.lookup_instance(py, &self.target) {
            if inst.bind(py).hasattr(name.as_str())? {
                return inst.bind(py).getattr(name.as_str()).map(|b| b.into());
            }
        }

        use pite_core::PropValue;
        let prop = self.host.with_tree(|tree| {
            tree.get(&NodeId::from(self.target.clone()))
                .and_then(|node| node.props.get(&name).cloned())
        });
        if let Some(prop) = prop {
            match prop {
                PropValue::Str(s) => return Ok(s.into_pyobject(py)?.into_any().unbind()),
                PropValue::Num(n) => return Ok(n.into_pyobject(py)?.into_any().unbind()),
                PropValue::Int(i) => return Ok(i.into_pyobject(py)?.into_any().unbind()),
                PropValue::Bool(b) => {
                    return Ok(pyo3::types::PyBool::new(py, b)
                        .to_owned()
                        .unbind()
                        .into_any());
                }
                PropValue::Vec2(x, y) => {
                    let v = crate::vec2::Vec2::new(x, y);
                    return Ok(Bound::new(py, v)?.into_any().unbind());
                }
                PropValue::Color(r, g, b, a) => {
                    return Ok((r, g, b, a).into_pyobject(py)?.into_any().unbind());
                }
            }
        }

        if let Some(inst) = self.host.lookup_instance(py, &self.target) {
            return inst.bind(py).getattr(name.as_str()).map(|b| b.into());
        }

        Err(pyo3::exceptions::PyAttributeError::new_err(format!(
            "node {:?} has no script or property with {name:?}",
            self.target
        )))
    }
}

pub fn proxy_for(host: std::sync::Arc<ScriptHost>, target: String) -> NodeProxy {
    NodeProxy { host, target }
}

pub fn resolve_caller(caller: &str, path: &str) -> anyhow::Result<NodeProxy> {
    let host = current_host().ok_or_else(|| anyhow::anyhow!("get_node has no script host"))?;
    let target = host
        .resolve(caller, path)
        .ok_or_else(|| anyhow::anyhow!("{caller}: get_node({path:?}) found nothing"))?;
    Ok(proxy_for(host, target))
}

pub fn resolve_caller_py(
    py: Python<'_>,
    caller: &str,
    path: &str,
) -> anyhow::Result<Py<NodeProxy>> {
    let host = current_host().ok_or_else(|| anyhow::anyhow!("get_node has no script host"))?;
    host.resolve_proxy(py, caller, path)
}
