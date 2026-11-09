// SPDX-License-Identifier: MIT OR Apache-2.0
//! Node type registration and factory instantiation for Pite.

use std::collections::HashMap;

use crate::error::{CoreError, Result};
use crate::node::{Node, NodeId};

/// Factory function closure producing a newly instantiated [`Node`] record for a given [`NodeId`].
pub type NodeFactory = Box<dyn Fn(NodeId) -> Node + Send + Sync>;

/// Registry managing instantiable node types and their corresponding construction factories.
///
/// Built-in registered set includes: `Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`,
/// `Label`, `Button`, `StaticBody2D`, `CharacterBody2D`, `RigidBody2D`, `Area2D`,
/// `CollisionShape2D`, and `TileMap2D`.
pub struct NodeTypeRegistry {
    factories: HashMap<String, NodeFactory>,
}

impl NodeTypeRegistry {
    /// Initializes a node type registry pre-populated with all engine schema node types.
    pub fn new() -> Self {
        let mut factories: HashMap<String, NodeFactory> = HashMap::new();
        for schema in crate::schema::NODE_SCHEMAS {
            let type_name = schema.type_name;
            factories.insert(
                type_name.to_string(),
                Box::new(move |id: NodeId| Node::new(id, type_name)),
            );
        }
        Self { factories }
    }

    /// Registers a custom node type name with its corresponding factory closure.
    pub fn register(&mut self, type_name: impl Into<String>, factory: NodeFactory) {
        self.factories.insert(type_name.into(), factory);
    }

    /// Checks if a node type is registered.
    pub fn contains(&self, type_name: &str) -> bool {
        self.factories.contains_key(type_name)
    }

    /// Instantiates a new [`Node`] of the requested type with the specified [`NodeId`].
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
