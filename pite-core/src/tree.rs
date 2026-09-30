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

    pub fn contains(&self, id: &NodeId) -> bool {
        self.nodes.contains_key(id.as_str())
    }

    pub fn children_of(&self, id: &NodeId) -> Vec<NodeId> {
        self.nodes
            .get(id.as_str())
            .map(|n| n.children.clone())
            .unwrap_or_default()
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

    pub fn remove(&mut self, id: &NodeId) -> Result<()> {
        let node = self
            .nodes
            .get(id.as_str())
            .ok_or_else(|| CoreError::UnknownNode {
                id: id.to_string(),
            })?;
        if self.root.as_ref() == Some(id) {
            return Err(CoreError::RemoveRoot);
        }
        let parent = node.parent.clone();
        let mut doomed = vec![id.to_string()];
        let mut i = 0;
        while i < doomed.len() {
            let key = doomed[i].clone();
            if let Some(n) = self.nodes.get(&key) {
                doomed.extend(n.children.iter().map(ToString::to_string));
            }
            i += 1;
        }
        if let Some(parent_id) = parent {
            if let Some(p) = self.nodes.get_mut(parent_id.as_str()) {
                p.children.retain(|c| c.as_str() != id.as_str());
            }
        }
        for key in doomed {
            self.nodes.remove(&key);
            self.order.retain(|k| k != &key);
        }
        Ok(())
    }

    pub fn reparent(&mut self, id: &NodeId, new_parent: Option<NodeId>) -> Result<()> {
        if !self.nodes.contains_key(id.as_str()) {
            return Err(CoreError::UnknownNode {
                id: id.to_string(),
            });
        }
        if self.root.as_ref() == Some(id) && new_parent.is_some() {
            return Err(CoreError::RemoveRoot);
        }
        if let Some(parent_id) = &new_parent {
            if !self.nodes.contains_key(parent_id.as_str()) {
                return Err(CoreError::MissingParent {
                    child: id.to_string(),
                    parent: parent_id.to_string(),
                });
            }
            let mut cursor = Some(parent_id.clone());
            while let Some(current) = cursor {
                if &current == id {
                    return Err(CoreError::Cycle {
                        child: id.to_string(),
                        parent: parent_id.to_string(),
                    });
                }
                cursor = self
                    .nodes
                    .get(current.as_str())
                    .and_then(|n| n.parent.clone());
            }
        }
        let old_parent = self.nodes.get(id.as_str()).and_then(|n| n.parent.clone());
        if let Some(old_id) = old_parent {
            if let Some(old) = self.nodes.get_mut(old_id.as_str()) {
                old.children.retain(|c| c.as_str() != id.as_str());
            }
        }
        if let Some(parent_id) = &new_parent {
            if let Some(new) = self.nodes.get_mut(parent_id.as_str()) {
                new.children.push(id.clone());
            }
        }
        if let Some(node) = self.nodes.get_mut(id.as_str()) {
            node.parent = new_parent;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeDesc;

    fn desc(id: &str, parent: Option<&str>) -> NodeDesc {
        let mut d = NodeDesc::new(NodeId::from(id.to_string()), "Node2D");
        d.name = id.to_string();
        d.parent = parent.map(|p| NodeId::from(p.to_string()));
        d
    }

    fn sample_tree() -> NodeTree {
        let mut tree = NodeTree::new();
        tree.insert(desc("root", None)).unwrap();
        tree.insert(desc("a", Some("root"))).unwrap();
        tree.insert(desc("b", Some("a"))).unwrap();
        tree
    }

    #[test]
    fn remove_drops_subtree_and_unlinks_parent() {
        let mut tree = sample_tree();
        tree.remove(&NodeId::from("a".to_string())).unwrap();
        assert_eq!(tree.len(), 1);
        assert!(tree.get(&NodeId::from("b".to_string())).is_none());
        assert!(tree.children_of(&NodeId::from("root".to_string())).is_empty());
    }

    #[test]
    fn remove_root_is_forbidden() {
        let mut tree = sample_tree();
        let err = tree.remove(&NodeId::from("root".to_string())).unwrap_err();
        assert!(matches!(err, CoreError::RemoveRoot));
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn remove_unknown_errors() {
        let mut tree = sample_tree();
        let err = tree.remove(&NodeId::from("ghost".to_string())).unwrap_err();
        assert!(matches!(err, CoreError::UnknownNode { .. }));
    }

    #[test]
    fn reparent_moves_linkage() {
        let mut tree = sample_tree();
        tree.insert(desc("c", Some("root"))).unwrap();
        tree.reparent(
            &NodeId::from("b".to_string()),
            Some(NodeId::from("c".to_string())),
        )
        .unwrap();
        let b = tree.get(&NodeId::from("b".to_string())).unwrap();
        assert_eq!(b.parent, Some(NodeId::from("c".to_string())));
        assert!(tree.children_of(&NodeId::from("a".to_string())).is_empty());
        assert_eq!(tree.children_of(&NodeId::from("c".to_string())).len(), 1);
    }

    #[test]
    fn reparent_missing_parent_errors() {
        let mut tree = sample_tree();
        let err = tree
            .reparent(
                &NodeId::from("b".to_string()),
                Some(NodeId::from("ghost".to_string())),
            )
            .unwrap_err();
        assert!(matches!(err, CoreError::MissingParent { .. }));
    }

    #[test]
    fn reparent_into_descendant_is_cycle() {
        let mut tree = sample_tree();
        let err = tree
            .reparent(
                &NodeId::from("a".to_string()),
                Some(NodeId::from("b".to_string())),
            )
            .unwrap_err();
        assert!(matches!(err, CoreError::Cycle { .. }));
    }
}
