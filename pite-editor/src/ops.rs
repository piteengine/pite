// SPDX-License-Identifier: MIT OR Apache-2.0
//! Headless editor operations: pure tree/scene/error logic with no egui.
//!
//! `app.rs` calls these from UI callbacks and routes every `Err` to the
//! console; nothing here touches the UI, so it is all unit-testable.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use pite_core::{NodeDesc, NodeId, NodeTree, NodeTypeRegistry, PropValue};
use pite_scene::{SceneDoc, SceneNode, SceneScript, FORMAT_VERSION};

/// Closed node-type set. Mirrors `NodeTypeRegistry::new`; kept here because
/// the registry exposes no listing API and the editor must not touch core.
pub fn registered_types() -> Vec<String> {
    [
        "Node",
        "Node2D",
        "Sprite2D",
        "Camera2D",
        "Timer",
        "Label",
        "Button",
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

fn slugify(name: &str) -> String {
    let mut slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    slug = slug.trim_matches('_').to_string();
    if slug.is_empty() {
        slug = "node".to_string();
    }
    slug
}

/// Insert a node of `type_name` under `parent` (defaults to the tree root).
/// The id is derived from `name` plus a counter until unique.
/// Unknown types and missing parents are errors.
pub fn add_node(
    tree: &mut NodeTree,
    parent: Option<NodeId>,
    type_name: &str,
    name: &str,
) -> Result<NodeId> {
    let registry = NodeTypeRegistry::new();
    let slug = slugify(name);
    let mut candidate = slug.clone();
    let mut counter = 2;
    while tree.contains(&NodeId::from(candidate.clone())) {
        candidate = format!("{slug}_{counter}");
        counter += 1;
    }
    let id = NodeId::from(candidate);
    let mut node = registry
        .create(id.clone(), type_name)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    node.name = name.to_string();
    let parent = match parent {
        Some(p) => {
            if !tree.contains(&p) {
                anyhow::bail!("parent `{p}` does not exist");
            }
            Some(p)
        }
        None => tree.root().cloned(),
    };
    let desc = NodeDesc {
        id: id.clone(),
        type_name: node.type_name.clone(),
        name: node.name.clone(),
        parent,
        props: node.props.clone(),
        script: None,
    };
    tree.insert(desc).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(id)
}

/// Remove `id` and its subtree. Refusing the root is a console-grade error.
pub fn remove_node(tree: &mut NodeTree, id: &NodeId) -> Result<()> {
    if tree.root() == Some(id) {
        anyhow::bail!("cannot remove the root node `{id}`");
    }
    tree.remove(id).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

/// Reparent `id` under `new_parent`. Cycle and root errors surface as-is.
pub fn move_node(tree: &mut NodeTree, id: &NodeId, new_parent: Option<NodeId>) -> Result<()> {
    tree.reparent(id, new_parent)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

/// Set one prop on `id`. Unknown nodes are errors.
pub fn set_prop(tree: &mut NodeTree, id: &NodeId, key: &str, value: PropValue) -> Result<()> {
    match tree.get_mut(id) {
        Some(node) => {
            node.props.insert(key.to_string(), value);
            Ok(())
        }
        None => anyhow::bail!("node `{id}` does not exist"),
    }
}

fn prop_to_toml(value: &PropValue) -> toml::Value {
    match value {
        PropValue::Str(s) => toml::Value::String(s.clone()),
        PropValue::Num(f) => toml::Value::Float(*f),
        PropValue::Int(i) => toml::Value::Integer(*i),
        PropValue::Bool(b) => toml::Value::Boolean(*b),
        PropValue::Vec2(x, y) => toml::Value::Array(vec![
            toml::Value::Float(*x),
            toml::Value::Float(*y),
        ]),
        _ => toml::Value::String(format!("{value:?}")),
    }
}

/// Build a saveable [`SceneDoc`] from the live tree, in tree order.
pub fn build_doc(tree: &NodeTree) -> SceneDoc {
    let root = tree.root().map(ToString::to_string).unwrap_or_default();
    let node = tree
        .iter()
        .map(|n| {
            let mut props = HashMap::new();
            for (k, v) in &n.props.0 {
                props.insert(k.clone(), prop_to_toml(v));
            }
            SceneNode {
                id: n.id.to_string(),
                type_name: n.type_name.clone(),
                name: n.name.clone(),
                parent: n.parent.clone().map(|p| p.to_string()),
                props,
                script: n.script.as_ref().map(|s| SceneScript {
                    path: s.path.clone(),
                    class: s.class_name.clone(),
                }),
            }
        })
        .collect();
    SceneDoc {
        format_version: FORMAT_VERSION,
        root,
        node,
        instance: Vec::new(),
    }
}

/// Parse Python-traceback `line N` (`File "...", line N`) marks: one
/// 1-based line (last match wins) plus the error's last non-empty line
/// (the exception line, not the `Traceback` header). Entries without a
/// line number (clean strings) are ignored.
pub fn gutter_marks(errors: &[String]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for err in errors {
        let last = err
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .last()
            .unwrap_or("");
        let mut line_no: Option<usize> = None;
        for (idx, _) in err.match_indices("line ") {
            let rest = &err[idx + "line ".len()..];
            let digits: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                if let Ok(n) = digits.parse::<usize>() {
                    line_no = Some(n);
                }
            }
        }
        if let Some(n) = line_no {
            out.push((n, last.to_string()));
        }
    }
    out
}

/// Thin file write used by scene and script saves.
pub fn save_text(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pite_core::NodeId;

    fn rooted_tree() -> NodeTree {
        let mut tree = NodeTree::new();
        add_node(&mut tree, None, "Node2D", "Main").unwrap();
        tree
    }

    #[test]
    fn add_remove_round_trip() {
        let mut tree = rooted_tree();
        let root = tree.root().cloned().unwrap();
        let child = add_node(&mut tree, Some(root.clone()), "Sprite2D", "Player").unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.children_of(&root), vec![child.clone()]);
        remove_node(&mut tree, &child).unwrap();
        assert_eq!(tree.len(), 1);
        assert!(tree.children_of(&root).is_empty());
    }

    #[test]
    fn remove_root_is_refused() {
        let mut tree = rooted_tree();
        let root = tree.root().cloned().unwrap();
        let err = remove_node(&mut tree, &root).unwrap_err();
        assert!(err.to_string().contains("root"), "unexpected: {err:#}");
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn reparent_cycle_is_refused() {
        let mut tree = rooted_tree();
        let root = tree.root().cloned().unwrap();
        let a = add_node(&mut tree, Some(root), "Node2D", "a").unwrap();
        let b = add_node(&mut tree, Some(a.clone()), "Node2D", "b").unwrap();
        let err = move_node(&mut tree, &a, Some(b.clone())).unwrap_err();
        assert!(err.to_string().contains("cycle"), "unexpected: {err:#}");
        assert!(tree.get(&a).unwrap().children.contains(&b));
    }

    #[test]
    fn prop_set_get_round_trip() {
        let mut tree = rooted_tree();
        let root = tree.root().cloned().unwrap();
        set_prop(&mut tree, &root, "speed", PropValue::Num(3.5)).unwrap();
        set_prop(
            &mut tree,
            &root,
            "title",
            PropValue::Str("hi".to_string()),
        )
        .unwrap();
        let node = tree.get(&root).unwrap();
        assert_eq!(node.props.get("speed"), Some(&PropValue::Num(3.5)));
        assert_eq!(
            node.props.get("title"),
            Some(&PropValue::Str("hi".to_string()))
        );
    }

    #[test]
    fn build_save_parse_round_trip_preserves_nodes_and_props() {
        let mut tree = rooted_tree();
        let root = tree.root().cloned().unwrap();
        set_prop(
            &mut tree,
            &root,
            "position",
            PropValue::Vec2(100.0, 200.0),
        )
        .unwrap();
        let child = add_node(&mut tree, Some(root), "Label", "Score").unwrap();
        set_prop(
            &mut tree,
            &child,
            "text",
            PropValue::Str("0".to_string()),
        )
        .unwrap();
        let doc = build_doc(&tree);
        let text = pite_scene::save_scene(&doc).unwrap();
        let again = pite_scene::parse_scene_str(&text).unwrap();
        assert_eq!(again.node.len(), 2);
        assert_eq!(again.root, doc.root);
        let player = again.node.iter().find(|n| n.id == child.to_string()).unwrap();
        assert_eq!(
            player.props.get("text"),
            Some(&toml::Value::String("0".to_string()))
        );
        let main = again.node.iter().find(|n| n.id == doc.root).unwrap();
        let pos = main.props.get("position").unwrap();
        match pos {
            toml::Value::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected position array, got {other:?}"),
        }
    }

    #[test]
    fn gutter_marks_extract_lines_and_ignore_clean_strings() {
        let traceback = "Traceback (most recent call last):\n  File \"res://scripts/player.py\", line 12, in _ready\n    x = 1 / 0\n  File \"res://scripts/player.py\", line 27, in _process\n    update()\nZeroDivisionError: division by zero".to_string();
        let clean = "all good, no errors".to_string();
        let marks = gutter_marks(&[traceback, clean]);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].0, 27);
        assert!(marks[0].1.contains("ZeroDivisionError"));
        assert!(gutter_marks(&["clean".to_string()]).is_empty());
    }

    #[test]
    fn unknown_type_is_an_error() {
        let mut tree = rooted_tree();
        let root = tree.root().cloned().unwrap();
        let err = add_node(&mut tree, Some(root), "Nope", "x").unwrap_err();
        assert!(err.to_string().contains("Nope"), "unexpected: {err:#}");
    }
}
