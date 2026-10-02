use std::collections::HashMap;

use crate::error::{CoreError, Result};
use crate::node::{Node, NodeId};

/// Factory producing a default node record for a registered type.
pub type NodeFactory = Box<dyn Fn(NodeId) -> Node + Send + Sync>;

/// Registry for node types. New nodes = new registration,
/// never a core match-statement edit.
///
/// Registered set: `Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`,
/// `Label`, `Button`.
pub struct NodeTypeRegistry {
    factories: HashMap<String, NodeFactory>,
}

impl NodeTypeRegistry {
    pub fn new() -> Self {
        let mut factories: HashMap<String, NodeFactory> = HashMap::new();
        for type_name in [
            "Node", "Node2D", "Sprite2D", "Camera2D", "Timer", "Label", "Button",
        ] {
            factories.insert(
                type_name.to_string(),
                Box::new(move |id: NodeId| Node::new(id, type_name)),
            );
        }
        Self { factories }
    }

    pub fn register(&mut self, type_name: impl Into<String>, factory: NodeFactory) {
        self.factories.insert(type_name.into(), factory);
    }

    pub fn contains(&self, type_name: &str) -> bool {
        self.factories.contains_key(type_name)
    }

    pub fn create(&self, id: NodeId, type_name: &str) -> Result<Node> {
        self.factories
            .get(type_name)
            .map(|f| f(id))
            .ok_or_else(|| CoreError::UnknownNodeType(type_name.to_string()))
    }
}

impl Default for NodeTypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
