use std::collections::HashMap;

use crate::error::{CoreError, Result};
use crate::node::{Node, NodeDesc, NodeId};

/// The scene tree. Owns all nodes; dropping a subtree drops its scripts.
#[derive(Debug, Default)]
pub struct NodeTree {
    nodes: HashMap<String, Node>,
    order: Vec<String>,
    root: Option<NodeId>,
}

impl NodeTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a node record. Parent linkage is best-effort: unknown parents
    /// are reported by `pite-scene::validate`, never a load failure.
    pub fn insert(&mut self, desc: NodeDesc) -> Result<()> {
        if desc.id.is_empty() {
            return Err(CoreError::EmptyNodeId);
        }
        if self.nodes.contains_key(desc.id.as_str()) {
            return Err(CoreError::DuplicateNode(desc.id.to_string()));
        }
        if self.root.is_none() && desc.parent.is_none() {
            self.root = Some(desc.id.clone());
        }
        let node = Node {
            id: desc.id.clone(),
            type_name: desc.type_name,
            name: desc.name,
            parent: desc.parent.clone(),
            children: Vec::new(),
            props: desc.props,
            script: desc.script,
        };
        if let Some(parent_id) = &desc.parent {
            if let Some(parent) = self.nodes.get_mut(parent_id.as_str()) {
                parent.children.push(desc.id.clone());
            }
        }
        self.order.push(desc.id.to_string());
        self.nodes.insert(desc.id.to_string(), node);
        Ok(())
    }

    pub fn get(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id.as_str())
    }

    pub fn root(&self) -> Option<&NodeId> {
        self.root.as_ref()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
