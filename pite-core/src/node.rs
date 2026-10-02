use std::fmt;

use crate::Props;

/// Handle to a node inside a [`NodeTree`].
///
/// Ids are human-readable strings: they survive edits, read well
/// in diffs, and can be crossed to Python without an integer translation table.
#[derive(Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

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

/// Script attachment: project-relative path plus the exported class name.
///
/// One file exports one class, which keeps the `path + class` attach
/// mapping trivial.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptRef {
    pub path: String,
    pub class_name: String,
}

/// A node record. Data only: behaviour lives in [`crate::NodeTypeRegistry`] and
/// [`crate::LifecycleSink`], so the tree stays free of subsystem types.
///
/// The tree owns the structural invariants (`parent`/`children` agreement); treat
/// these fields as owned by [`crate::NodeTree`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub type_name: String,
    pub name: String,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub props: Props,
    pub script: Option<ScriptRef>,
}

impl Node {
    /// Minimal node record; the tree fills in `name`, `parent` and `children`.
    pub fn new(id: NodeId, type_name: impl Into<String>) -> Self {
        Self {
            id,
            type_name: type_name.into(),
            name: String::new(),
            ..Self::default()
        }
    }
}

/// Input used to build a [`Node`], e.g. from a scene file entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeDesc {
    pub id: NodeId,
    pub type_name: String,
    pub name: String,
    pub parent: Option<NodeId>,
    pub props: Props,
    pub script: Option<ScriptRef>,
}

impl NodeDesc {
    pub fn new(id: impl Into<NodeId>, type_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            type_name: type_name.into(),
            ..Self::default()
        }
    }
}