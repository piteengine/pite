// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashMap;

use pite_core::{NodeId, NodeSchema, SchemaRegistry};

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
    pub type_name: String,
    pub description: String,
    pub children: Vec<NodeTypeInfo>,
}

/// Add-node tree derived from a [`SchemaRegistry`]: every concrete type sits
/// under its nearest **non-abstract** ancestor. Abstract bases flatten out, so
/// plugin nodes inheriting `Control` or `Node2D` appear directly in the tree.
pub fn build_node_hierarchy_from(schemas: &SchemaRegistry) -> Vec<NodeTypeInfo> {
    let mut by_parent: HashMap<&str, Vec<&NodeSchema>> = HashMap::new();
    let mut roots: Vec<&NodeSchema> = Vec::new();
    for schema in schemas.iter() {
        if schema.abstract_class {
            continue;
        }
        match nearest_visible_ancestor(schemas, schema.type_name.as_ref()) {
            Some(parent) => by_parent.entry(parent).or_default().push(schema),
            None => roots.push(schema),
        }
    }
    roots
        .into_iter()
        .map(|schema| build_branch(schema, &by_parent))
        .collect()
}

fn build_branch(schema: &NodeSchema, by_parent: &HashMap<&str, Vec<&NodeSchema>>) -> NodeTypeInfo {
    let children = by_parent
        .get(schema.type_name.as_ref())
        .map(|kids| kids.iter().map(|s| build_branch(s, by_parent)).collect())
        .unwrap_or_default();
    NodeTypeInfo {
        type_name: schema.type_name.to_string(),
        description: schema.description.to_string(),
        children,
    }
}

/// Walk `inherits` until a non-abstract ancestor; `None` for the root.
fn nearest_visible_ancestor<'a>(
    schemas: &'a SchemaRegistry,
    type_name: &'a str,
) -> Option<&'a str> {
    let mut current = schemas.get(type_name)?.inherits.as_deref();
    while let Some(name) = current {
        let schema = schemas.get(name)?;
        if !schema.abstract_class {
            return Some(schema.type_name.as_ref());
        }
        current = schema.inherits.as_deref();
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
