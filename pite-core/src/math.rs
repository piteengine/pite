/// 2D vector shared by value (composition, not inheritance).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

/// Axis-aligned rectangle, e.g. for the `query_overlap` seam.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A node's global position is its local
/// `position` plus every ancestor's, up to the root. Nodes without a
/// `position` prop contribute nothing. Moving a parent carries children.
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
