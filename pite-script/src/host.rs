// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use anyhow::Context as _;
use pite_core::{NodeId, NodeTree, SignalRegistry, Vec2};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

static CURRENT_HOST: RwLock<Option<std::sync::Arc<ScriptHost>>> = RwLock::new(None);

#[cfg(test)]
pub(crate) static HOST_SERIAL: Mutex<()> = Mutex::new(());

pub fn set_current_host(host: std::sync::Arc<ScriptHost>) {
    *CURRENT_HOST.write().unwrap() = Some(host);
}

pub(crate) fn current_host() -> Option<std::sync::Arc<ScriptHost>> {
    CURRENT_HOST.read().unwrap().clone()
}

pub(crate) fn bound_target(callable: &Bound<'_, PyAny>) -> anyhow::Result<(String, String)> {
    let owner = callable
        .getattr("__self__")
        .map_err(|_| anyhow::anyhow!("connect needs a bound pite method"))?;
    let node_path: String = owner
        .getattr("node_path")
        .map_err(|_| anyhow::anyhow!("connect target is not a pite node method"))?
        .extract()?;
    let name: String = callable
        .getattr("__name__")
        .map_err(|_| anyhow::anyhow!("connect target has no method name"))?
        .extract()?;
    Ok((node_path, name))
}

pub struct ScriptHost {
    tree: Mutex<NodeTree>,
    instances: Mutex<HashMap<String, Py<PyAny>>>,
    signals: Mutex<HashMap<String, SignalRegistry>>,
    connections: Mutex<HashMap<(String, String), Vec<(String, String)>>>,
    callable_connections: Mutex<HashMap<(String, String), Vec<Py<PyAny>>>>,
    proxy_cache: Mutex<HashMap<(String, String), Py<NodeProxy>>>,
}

pub const SIGNAL_TYPES: &[&str] = &["int", "float", "str", "bool"];

#[derive(Debug, Clone, PartialEq)]
pub enum PayloadValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
}

impl ScriptHost {
    pub fn new(tree: NodeTree) -> Self {
        Self {
            tree: Mutex::new(tree),
            instances: Mutex::new(HashMap::new()),
            signals: Mutex::new(HashMap::new()),
            connections: Mutex::new(HashMap::new()),
            callable_connections: Mutex::new(HashMap::new()),
            proxy_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Read a prop, trusting the `to_node_tree` invariant that every schema
    /// prop exists on every node from creation.
    ///
    /// Missing **node** is a normal runtime scenario — a script or the editor
    /// can drop a node mid-play, and `pite-runtime` still holds slots for it
    /// until the next `rebuild`. Returning `T::default()` and logging keeps
    /// that recoverable (`dropped_button_disconnects_silently` depends on it:
    /// removing a node then polling its position must not crash).
    ///
    /// Missing **prop** on an *existing* node is the invariant violation the
    /// `to_node_tree` / `add_node` schema-default filling was introduced to
    /// eliminate. Debug builds `debug_assert!(false)` to surface it; release
    /// builds log and fall back to the schema default (looked up via
    /// `all_props`, so inherited props are found) so a player never sees a
    /// crash.
    fn get_or_schema_default<T, F>(&self, node: &str, key: &str, extract: F) -> T
    where
        T: Default,
        F: Fn(&pite_core::PropValue) -> Option<T>,
    {
        self.with_tree(|t| {
            let node_id = NodeId::from(node.to_string());
            let Some(n) = t.get(&node_id) else {
                tracing::error!(node = %node, key = %key, "node missing; returning type default");
                return T::default();
            };
            if let Some(v) = n.props.get(key).and_then(|v| extract(v)) {
                return v;
            }
            debug_assert!(
                false,
                "prop {key:?} missing/mistyped on {node:?} (type {:?}); \
                 to_node_tree should have filled it",
                n.type_name
            );
            tracing::error!(
                node = %node,
                key = %key,
                ty = %n.type_name,
                "prop missing or wrong type; using schema default"
            );
            pite_core::all_props(&n.type_name)
                .iter()
                .find(|p| p.key == key)
                .and_then(|p| extract(&p.default_value.to_prop_value()))
                .unwrap_or_default()
        })
    }

    pub fn get_f64(&self, node: &str, key: &str) -> f64 {
        self.get_or_schema_default(node, key, |v| match v {
            pite_core::PropValue::Num(f) => Some(*f),
            pite_core::PropValue::Int(i) => Some(*i as f64),
            _ => None,
        })
    }

    pub fn get_i64(&self, node: &str, key: &str) -> i64 {
        self.get_or_schema_default(node, key, |v| match v {
            pite_core::PropValue::Int(i) => Some(*i),
            pite_core::PropValue::Num(f) => Some(*f as i64),
            _ => None,
        })
    }

    pub fn get_bool(&self, node: &str, key: &str) -> bool {
        self.get_or_schema_default(node, key, |v| match v {
            pite_core::PropValue::Bool(b) => Some(*b),
            _ => None,
        })
    }

    pub fn get_string(&self, node: &str, key: &str) -> String {
        self.get_or_schema_default(node, key, |v| match v {
            pite_core::PropValue::Str(s) => Some(s.clone()),
            _ => None,
        })
    }

    pub fn get_color_tuple(&self, node: &str, key: &str) -> (u8, u8, u8, u8) {
        self.get_or_schema_default(node, key, |v| match v {
            pite_core::PropValue::Color(r, g, b, a) => Some((*r, *g, *b, *a)),
            _ => None,
        })
    }

    pub fn get_vec2(&self, node: &str, key: &str) -> Vec2 {
        self.get_or_schema_default(node, key, |v| match v {
            pite_core::PropValue::Vec2(x, y) => Some(Vec2 { x: *x, y: *y }),
            _ => None,
        })
    }

    pub fn set_prop(&self, node: &str, key: &str, value: pite_core::PropValue) {
        let mut tree = self.tree.lock().unwrap();
        if let Some(n) = tree.get_mut(&NodeId::from(node.to_string())) {
            n.props.insert(key.to_string(), value);
        }
    }

    pub fn resolve_proxy(
        self: &std::sync::Arc<Self>,
        py: Python<'_>,
        caller: &str,
        path: &str,
    ) -> anyhow::Result<Py<NodeProxy>> {
        let key = (caller.to_string(), path.to_string());
        {
            let cache = self.proxy_cache.lock().unwrap();
            if let Some(proxy) = cache.get(&key) {
                return Ok(proxy.clone_ref(py));
            }
        }
        let target = self
            .resolve(caller, path)
            .ok_or_else(|| anyhow::anyhow!("{caller}: get_node({path:?}) found nothing"))?;
        let proxy = Py::new(py, proxy_for(self.clone(), target))?;
        self.proxy_cache
            .lock()
            .unwrap()
            .insert(key, proxy.clone_ref(py));
        Ok(proxy)
    }

    pub fn register(&self, node: impl Into<String>, instance: Py<PyAny>) {
        self.instances.lock().unwrap().insert(node.into(), instance);
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
            return tree
                .get(&NodeId::from(abs.to_string()))
                .map(|n| n.id.to_string());
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

    pub fn declare_signals(
        &self,
        node: &str,
        defs: Vec<(String, Vec<String>)>,
    ) -> anyhow::Result<()> {
        for (name, payload) in &defs {
            for ty in payload {
                if !SIGNAL_TYPES.contains(&ty.as_str()) {
                    anyhow::bail!(
                        "{node}: signal {name:?} has unknown payload type {ty:?} (want one of {SIGNAL_TYPES:?})"
                    );
                }
            }
        }
        let mut signals = self.signals.lock().unwrap();
        let registry = signals.entry(node.to_string()).or_default();
        for (name, payload) in defs {
            registry.register(name, payload);
        }
        Ok(())
    }

    pub fn connect(
        &self,
        source: &str,
        signal: &str,
        target: &str,
        handler: &str,
    ) -> anyhow::Result<()> {
        let signals = self.signals.lock().unwrap();
        if signals
            .get(source)
            .and_then(|reg| reg.get(signal))
            .is_none()
        {
            anyhow::bail!("{source}: connect to unknown signal {signal:?}");
        }
        drop(signals);
        self.with_tree(|tree| {
            if tree.get(&NodeId::from(target.to_string())).is_none() {
                anyhow::bail!("connect target node {target:?} does not exist");
            }
            Ok(())
        })?;
        self.connections
            .lock()
            .unwrap()
            .entry((source.to_string(), signal.to_string()))
            .or_default()
            .push((target.to_string(), handler.to_string()));
        Ok(())
    }

    pub fn connect_callable(
        &self,
        _py: Python<'_>,
        source: &str,
        signal: &str,
        callable: Py<PyAny>,
    ) -> anyhow::Result<()> {
        let signals = self.signals.lock().unwrap();
        if signals
            .get(source)
            .and_then(|reg| reg.get(signal))
            .is_none()
        {
            anyhow::bail!("{source}: connect to unknown signal {signal:?}");
        }
        drop(signals);
        self.callable_connections
            .lock()
            .unwrap()
            .entry((source.to_string(), signal.to_string()))
            .or_default()
            .push(callable);
        Ok(())
    }

    pub fn emit(
        &self,
        py: Python<'_>,
        source: &str,
        signal: &str,
        args: &Bound<'_, PyTuple>,
    ) -> anyhow::Result<()> {
        let expected = self
            .signals
            .lock()
            .unwrap()
            .get(source)
            .and_then(|reg| reg.get(signal).cloned())
            .ok_or_else(|| anyhow::anyhow!("{source}: emit of unknown signal {signal:?}"))?;
        if args.len() != expected.payload.len() {
            anyhow::bail!(
                "{source}: signal {signal:?} wants {} values, got {}",
                expected.payload.len(),
                args.len()
            );
        }
        let mut values = Vec::with_capacity(args.len());
        for (i, (want, arg)) in expected.payload.iter().zip(args.iter()).enumerate() {
            let value = coerce_payload(want, &arg)
                .with_context(|| format!("{source}: signal {signal:?} arg {i} wants {want}"))?;
            values.push(value);
        }
        let callables: Vec<Py<PyAny>> = self
            .callable_connections
            .lock()
            .unwrap()
            .get(&(source.to_string(), signal.to_string()))
            .map(|list| list.iter().map(|c| c.clone_ref(py)).collect())
            .unwrap_or_default();
        if !callables.is_empty() {
            let py_args: Vec<Py<PyAny>> = values.iter().map(|v| v.to_object(py)).collect();
            let tuple = PyTuple::new(py, py_args).map_err(|e| anyhow::anyhow!("{e}"))?;
            for c in callables {
                c.bind(py)
                    .call(tuple.clone(), None)
                    .map_err(|e| anyhow::anyhow!("callable raised: {e}"))?;
            }
        }
        let targets: Vec<(String, String)> = self
            .connections
            .lock()
            .unwrap()
            .get(&(source.to_string(), signal.to_string()))
            .cloned()
            .unwrap_or_default();
        for (target, handler) in targets {
            let Some(inst) = self.lookup_instance(py, &target) else {
                continue;
            };
            let callable = inst.bind(py).getattr(handler.as_str()).map_err(|_| {
                anyhow::anyhow!("{target}: handler {handler:?} missing for signal {signal:?}")
            })?;
            let py_args: Vec<Py<PyAny>> = values.iter().map(|v| v.to_object(py)).collect();
            callable
                .call(
                    PyTuple::new(py, py_args).map_err(|e| anyhow::anyhow!("{e}"))?,
                    None,
                )
                .map_err(|e| anyhow::anyhow!("{target}.{handler} raised: {e}"))?;
        }
        Ok(())
    }

    pub fn has_signal(&self, node: &str, signal: &str) -> bool {
        self.signals
            .lock()
            .unwrap()
            .get(node)
            .and_then(|reg| reg.get(signal))
            .is_some()
    }

    pub fn disconnect_node(&self, node: &str) {
        self.signals.lock().unwrap().remove(node);
        self.connections
            .lock()
            .unwrap()
            .retain(|(src, _), targets| {
                if src == node {
                    return false;
                }
                targets.retain(|(t, _)| t != node);
                !targets.is_empty()
            });
        self.callable_connections
            .lock()
            .unwrap()
            .retain(|(src, _), _| src != node);
        self.proxy_cache
            .lock()
            .unwrap()
            .retain(|(src, _), _| src != node);
    }

    pub fn invalidate_proxy_cache(&self, node: &str) {
        self.proxy_cache
            .lock()
            .unwrap()
            .retain(|(src, _), _| src != node);
    }
}

fn coerce_payload(want: &str, arg: &Bound<'_, PyAny>) -> anyhow::Result<PayloadValue> {
    use pyo3::types::{PyBool, PyInt, PyString};
    match want {
        "bool" => {
            if arg.is_instance_of::<PyBool>() {
                Ok(PayloadValue::Bool(arg.extract()?))
            } else {
                anyhow::bail!("not a bool")
            }
        }
        "int" => {
            if arg.is_instance_of::<PyBool>() || !arg.is_instance_of::<PyInt>() {
                anyhow::bail!("not an int");
            }
            Ok(PayloadValue::Int(arg.extract()?))
        }
        "float" => {
            if arg.is_instance_of::<PyBool>() {
                anyhow::bail!("not a float");
            }
            if let Ok(f) = arg.extract::<f64>() {
                Ok(PayloadValue::Float(f))
            } else {
                anyhow::bail!("not a float");
            }
        }
        "str" => {
            if arg.is_instance_of::<PyString>() {
                Ok(PayloadValue::Str(arg.extract()?))
            } else {
                anyhow::bail!("not a str");
            }
        }
        other => anyhow::bail!("unknown payload type {other:?}"),
    }
}

impl PayloadValue {
    fn to_object(&self, py: Python<'_>) -> Py<PyAny> {
        match self {
            PayloadValue::Int(i) => i.into_pyobject(py).unwrap().into_any().unbind(),
            PayloadValue::Float(f) => f.into_pyobject(py).unwrap().into_any().unbind(),
            PayloadValue::Str(s) => s.into_pyobject(py).unwrap().into_any().unbind(),
            PayloadValue::Bool(b) => pyo3::types::PyBool::new(py, *b)
                .to_owned()
                .unbind()
                .into_any(),
        }
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

    fn __getattr__(&self, py: Python<'_>, name: String) -> PyResult<Py<PyAny>> {
        if name == "position" || name == "text" || name == "size" {
            return Err(pyo3::exceptions::PyAttributeError::new_err(name));
        }
        if self.host.has_signal(&self.target, &name) {
            let bound = BoundSignal {
                host: self.host.clone(),
                node: self.target.clone(),
                signal: name,
            };
            return Ok(Bound::new(py, bound)?.into_any().unbind());
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

#[pyclass(name = "BoundSignal")]
pub struct BoundSignal {
    host: std::sync::Arc<ScriptHost>,
    node: String,
    signal: String,
}

impl BoundSignal {
    pub fn with_host(host: std::sync::Arc<ScriptHost>, node: String, signal: String) -> Self {
        Self { host, node, signal }
    }
}

#[pymethods]
impl BoundSignal {
    fn connect(&self, py: Python<'_>, callable: Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok((target, handler)) = bound_target(&callable) {
            self.host
                .connect(&self.node, &self.signal, &target, &handler)
                .map_err(|e| pyo3::exceptions::PyKeyError::new_err(e.to_string()))
        } else {
            self.host
                .connect_callable(py, &self.node, &self.signal, callable.into_any().unbind())
                .map_err(|e| pyo3::exceptions::PyKeyError::new_err(e.to_string()))
        }
    }

    #[pyo3(signature = (*args))]
    fn emit(&self, py: Python<'_>, args: &Bound<'_, PyTuple>) -> PyResult<()> {
        self.host
            .emit(py, &self.node, &self.signal, args)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
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

pub fn fire_pressed(host: &ScriptHost, id: &str) -> anyhow::Result<()> {
    Python::attach(|py| host.emit(py, id, "pressed", &PyTuple::empty(py)))
}

pub fn fire_timeout(host: &ScriptHost, id: &str) -> anyhow::Result<()> {
    Python::attach(|py| host.emit(py, id, "timeout", &PyTuple::empty(py)))
}

pub fn fire_body_entered(host: &ScriptHost, area_id: &str, body_id: &str) -> anyhow::Result<()> {
    Python::attach(|py| {
        let arg = PyTuple::new(py, [body_id])?;
        host.emit(py, area_id, "body_entered", &arg)
    })
}

pub fn fire_body_exited(host: &ScriptHost, area_id: &str, body_id: &str) -> anyhow::Result<()> {
    Python::attach(|py| {
        let arg = PyTuple::new(py, [body_id])?;
        host.emit(py, area_id, "body_exited", &arg)
    })
}

pub fn fire_area_entered(
    host: &ScriptHost,
    area_id: &str,
    other_area_id: &str,
) -> anyhow::Result<()> {
    Python::attach(|py| {
        let arg = PyTuple::new(py, [other_area_id])?;
        host.emit(py, area_id, "area_entered", &arg)
    })
}

pub fn fire_area_exited(
    host: &ScriptHost,
    area_id: &str,
    other_area_id: &str,
) -> anyhow::Result<()> {
    Python::attach(|py| {
        let arg = PyTuple::new(py, [other_area_id])?;
        host.emit(py, area_id, "area_exited", &arg)
    })
}
