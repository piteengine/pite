// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashMap;
use std::sync::OnceLock;

use pite_core::{NodeId, NodeTypeSchema};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropZone {
    None,
    Before,
    Under,
    After,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDndPayload {
    pub id: String,
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone)]
pub struct AddNodeModal {
    pub parent: Option<NodeId>,
    pub selected_type: String,
    pub name: String,
    pub search: String,
}

#[derive(Debug, Clone)]
pub struct RenameNodeModal {
    pub id: NodeId,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct InstanceSceneModal {
    pub parent: Option<NodeId>,
}

pub struct NodeTypeInfo {
    pub type_name: &'static str,
    pub description: &'static str,
    pub children: Vec<NodeTypeInfo>,
}

/// Add-node tree derived from `pite_core::NODE_SCHEMAS`: every concrete
/// type sits under its nearest **non-abstract** ancestor. Abstract bases
/// (`Control`, `CollisionObject2D`, `PhysicsBody2D`) flatten out, so
/// `Label`/`Button` hang off `Node` and physics bodies off `Node2D` —
/// the same shape the old hardcoded tree had, now impossible to drift
/// from the schema.
pub fn node_hierarchy() -> &'static [NodeTypeInfo] {
    static HIERARCHY: OnceLock<Vec<NodeTypeInfo>> = OnceLock::new();
    HIERARCHY.get_or_init(build_node_hierarchy).as_slice()
}

fn build_node_hierarchy() -> Vec<NodeTypeInfo> {
    let mut by_parent: HashMap<&'static str, Vec<&'static NodeTypeSchema>> = HashMap::new();
    let mut roots: Vec<&'static NodeTypeSchema> = Vec::new();
    for &schema in pite_core::NODE_SCHEMAS {
        if schema.abstract_class {
            continue;
        }
        match nearest_visible_ancestor(schema.type_name) {
            Some(parent) => by_parent.entry(parent).or_default().push(schema),
            None => roots.push(schema),
        }
    }
    roots
        .into_iter()
        .map(|schema| build_branch(schema, &by_parent))
        .collect()
}

fn build_branch(
    schema: &'static NodeTypeSchema,
    by_parent: &HashMap<&'static str, Vec<&'static NodeTypeSchema>>,
) -> NodeTypeInfo {
    let children = by_parent
        .get(schema.type_name)
        .map(|kids| kids.iter().map(|s| build_branch(s, by_parent)).collect())
        .unwrap_or_default();
    NodeTypeInfo {
        type_name: schema.type_name,
        description: schema.description,
        children,
    }
}

/// Walk `inherits` until a non-abstract ancestor; `None` for the root.
fn nearest_visible_ancestor(type_name: &'static str) -> Option<&'static str> {
    let mut current = pite_core::node_type_schema(type_name)?.inherits;
    while let Some(name) = current {
        let schema = pite_core::node_type_schema(name)?;
        if !schema.abstract_class {
            return Some(schema.type_name);
        }
        current = schema.inherits;
    }
    None
}

pub(super) fn node_type_matches_search(info: &NodeTypeInfo, query: &str) -> bool {
    if info.type_name.to_lowercase().contains(query)
        || info.description.to_lowercase().contains(query)
    {
        return true;
    }
    info.children
        .iter()
        .any(|child| node_type_matches_search(child, query))
}
