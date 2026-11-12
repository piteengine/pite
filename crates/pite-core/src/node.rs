// SPDX-License-Identifier: MIT OR Apache-2.0
use std::fmt;

use crate::Props;

/// Unique identifier for a node within a [`NodeTree`](crate::NodeTree).
///
/// Node identifiers are human-readable strings (e.g. `root`, `player`, `hud`).
/// They remain stable across scene edits, read cleanly in version-control diffs,
/// and map directly to scripting runtimes without requiring integer translation tables.
#[derive(Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(String);

impl NodeId {
    /// Creates a new `NodeId` from a string or string slice.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns a string slice of the node identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` if the node identifier is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({:?})", self.0)
    }
}

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for NodeId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for NodeId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Script attachment referencing a project-relative Python file and target class.
///
/// Each script file exports a single class, providing a direct 1-to-1 mapping
/// between the scene node and its scripting logic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptRef {
    /// Project-relative path to the script file (e.g. `res://scripts/player.py`).
    pub path: String,
    /// Name of the Python class inheriting from an engine node type.
    pub class_name: String,
}

/// A node instance within a [`NodeTree`](crate::NodeTree).
///
/// Contains pure state data (identifiers, properties, and tree links). Dynamic behaviors
/// are handled externally by [`crate::NodeTypeRegistry`] and [`crate::LifecycleSink`].
///
/// The enclosing [`NodeTree`](crate::NodeTree) maintains structural invariants (such as
/// parent-child consistency).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    /// Unique identifier of this node.
    pub id: NodeId,
    /// Registered node type name (e.g. `Node2D`, `Sprite2D`).
    pub type_name: String,
    /// Display name shown in editor and diagnostics.
    pub name: String,
    /// Optional identifier of the parent node.
    pub parent: Option<NodeId>,
    /// Ordered list of child node identifiers.
    pub children: Vec<NodeId>,
    /// Property map containing scalar, vector, and color attributes.
    pub props: Props,
    /// Optional attached Python script reference.
    pub script: Option<ScriptRef>,
}

impl Node {
    /// Constructs a minimal node record. Parent and child links are populated by the tree.
    pub fn new(id: NodeId, type_name: impl Into<String>) -> Self {
        Self {
            id,
            type_name: type_name.into(),
            name: String::new(),
            ..Self::default()
        }
    }
}

/// Descriptor used to construct a [`Node`], typically parsed from a `.ptscn` scene file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeDesc {
    /// Unique identifier for the node.
    pub id: NodeId,
    /// Concrete node type name.
    pub type_name: String,
    /// Human-friendly display name.
    pub name: String,
    /// Identifier of the parent node, if any.
    pub parent: Option<NodeId>,
    /// Node properties table.
    pub props: Props,
    /// Attached script reference, if any.
    pub script: Option<ScriptRef>,
}

impl NodeDesc {
    /// Constructs a new node descriptor with default properties.
    pub fn new(id: impl Into<NodeId>, type_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            type_name: type_name.into(),
            ..Self::default()
        }
    }
}
