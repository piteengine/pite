use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use pite_core::{NodeId, NodeTree};
use pyo3::prelude::*;

static CURRENT_HOST: RwLock<Option<std::sync::Arc<ScriptHost>>> = RwLock::new(None);

pub fn set_current_host(host: std::sync::Arc<ScriptHost>) {
    *CURRENT_HOST.write().unwrap() = Some(host);
}

fn current_host() -> Option<std::sync::Arc<ScriptHost>> {
    CURRENT_HOST.read().unwrap().clone()
}

pub struct ScriptHost {
    tree: Mutex<NodeTree>,
    instances: Mutex<HashMap<String, Py<PyAny>>>,
}

impl ScriptHost {
    pub fn new(tree: NodeTree) -> Self {
        Self {
            tree: Mutex::new(tree),
            instances: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, node: impl Into<String>, instance: Py<PyAny>) {
        self.instances
            .lock()
            .unwrap()
            .insert(node.into(), instance);
    }

    pub fn unregister(&self, node: &str) {
        self.instances.lock().unwrap().remove(node);
    }

    pub fn with_tree<R>(&self, f: impl FnOnce(&NodeTree) -> R) -> R {
        f(&self.tree.lock().unwrap())
    }

    pub fn with_tree_mut<R>(&self, f: impl FnOnce(&mut NodeTree) -> R) -> R {
        f(&mut self.tree.lock().unwrap())
    }

    pub fn resolve(&self, caller: &str, path: &str) -> Option<String> {
        let tree = self.tree.lock().unwrap();
        if let Some(abs) = path.strip_prefix('/') {
            return tree.get(&NodeId::from(abs.to_string())).map(|n| n.id.to_string());
        }
        let mut cursor = tree.get(&NodeId::from(caller.to_string()))?.clone();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    let parent = cursor.parent.clone()?;
                    cursor = tree.get(&parent)?.clone();
                }
                name => {
                    let child = cursor
                        .children
                        .iter()
                        .map(|id| tree.get(id))
                        .find(|n| n.map(|n| n.name == name).unwrap_or(false))?;
                    cursor = child?.clone();
                }
            }
        }
        Some(cursor.id.to_string())
    }

    pub fn lookup_instance(&self, py: Python<'_>, node: &str) -> Option<Py<PyAny>> {
        self.instances
            .lock()
            .unwrap()
            .get(node)
            .map(|inst| inst.clone_ref(py))
    }
}

#[pyclass(name = "NodeProxy")]
pub struct NodeProxy {
    host: std::sync::Arc<ScriptHost>,
    target: String,
}

#[pymethods]
impl NodeProxy {
    #[getter]
    fn position(&self) -> PyResult<(f64, f64)> {
        use pite_core::PropValue;
        self.host.with_tree(|tree| {
            let node =
                tree.get(&NodeId::from(self.target.clone())).ok_or_else(|| {
                    pyo3::exceptions::PyKeyError::new_err(format!(
                        "node {:?} no longer exists",
                        self.target
                    ))
                })?;
            match node.props.get("position") {
                Some(PropValue::Vec2(x, y)) => Ok((*x, *y)),
                _ => Ok((0.0, 0.0)),
            }
        })
    }

    #[setter]
    fn set_position(&self, pos: (f64, f64)) {
        use pite_core::PropValue;
        self.host.with_tree_mut(|tree| {
            if let Some(node) = tree.get_mut(&NodeId::from(self.target.clone())) {
                node.props.insert(
                    "position".to_string(),
                    PropValue::Vec2(pos.0, pos.1),
                );
            }
        });
    }

    fn __getattr__(&self, py: Python<'_>, name: String) -> PyResult<Py<PyAny>> {
        if name == "position" {
            return Err(pyo3::exceptions::PyAttributeError::new_err("position"));
        }
        let inst = self.host.lookup_instance(py, &self.target).ok_or_else(|| {
            pyo3::exceptions::PyAttributeError::new_err(format!(
                "node {:?} has no script with {name:?}",
                self.target
            ))
        })?;
        inst.bind(py).getattr(name.as_str()).map(|b| b.into())
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
