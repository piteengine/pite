// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::{HashMap, HashSet};

/// A 2D floating-point vector with `x` and `y` components.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

/// Calculates the global world-space position of a node within a [`NodeTree`](crate::NodeTree).
///
/// Computes global position by accumulating local `position` vectors along the ancestor
/// chain from the target node up to the root. Nodes lacking a `position` property are
/// treated as having an offset of `(0.0, 0.0)`. Moving a parent node automatically carries
/// all of its descendants.
pub fn global_position(tree: &crate::NodeTree, id: &crate::NodeId) -> (f64, f64) {
    let mut x = 0.0;
    let mut y = 0.0;
    let mut cursor = tree.get(id);
    while let Some(node) = cursor {
        if let Some(crate::PropValue::Vec2(px, py)) = node.props.get("position") {
            x += px;
            y += py;
        }
        cursor = node.parent.as_ref().and_then(|p| tree.get(p));
    }
    (x, y)
}

/// Computes global world-space positions for every node in `tree` in $O(N)$
/// amortized time via memoized ancestor lookup.
pub fn global_positions(tree: &crate::NodeTree) -> HashMap<crate::NodeId, (f64, f64)> {
    fn resolve(
        tree: &crate::NodeTree,
        id: &crate::NodeId,
        cache: &mut HashMap<crate::NodeId, (f64, f64)>,
        visiting: &mut HashSet<crate::NodeId>,
    ) -> (f64, f64) {
        if let Some(&pos) = cache.get(id) {
            return pos;
        }
        if !visiting.insert(id.clone()) {
            return (0.0, 0.0);
        }
        let pos = match tree.get(id) {
            Some(node) => {
                let (lx, ly) = match node.props.get("position") {
                    Some(crate::PropValue::Vec2(px, py)) => (*px, *py),
                    _ => (0.0, 0.0),
                };
                let (px, py) = match node.parent.as_ref() {
                    Some(parent) => resolve(tree, parent, cache, visiting),
                    None => (0.0, 0.0),
                };
                (px + lx, py + ly)
            }
            None => (0.0, 0.0),
        };
        visiting.remove(id);
        cache.insert(id.clone(), pos);
        pos
    }

    let mut cache = HashMap::with_capacity(tree.len());
    let mut visiting = HashSet::new();
    for node in tree.iter() {
        resolve(tree, &node.id, &mut cache, &mut visiting);
    }
    cache
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeDesc, NodeId, NodeTree, PropValue, Props};

    fn placed(id: &str, parent: Option<&str>, pos: Option<(f64, f64)>) -> NodeDesc {
        let mut d = NodeDesc::new(NodeId::from(id.to_string()), "Node2D");
        d.name = id.to_string();
        d.parent = parent.map(|p| NodeId::from(p.to_string()));
        let mut props = Props::default();
        if let Some((x, y)) = pos {
            props.insert("position", PropValue::Vec2(x, y));
        }
        d.props = props;
        d
    }

    #[test]
    fn child_follows_moving_parent() {
        let mut tree = NodeTree::new();
        tree.insert(placed("root", None, Some((10.0, 0.0))))
            .unwrap();
        tree.insert(placed("child", Some("root"), Some((5.0, 0.0))))
            .unwrap();
        tree.insert(placed("grand", Some("child"), None)).unwrap();
        assert_eq!(
            global_position(&tree, &NodeId::from("grand".to_string())),
            (15.0, 0.0)
        );
        let root_id = NodeId::from("root".to_string());
        tree.get_mut(&root_id)
            .unwrap()
            .props
            .insert("position", PropValue::Vec2(100.0, 0.0));
        assert_eq!(
            global_position(&tree, &NodeId::from("grand".to_string())),
            (105.0, 0.0)
        );
    }
}
