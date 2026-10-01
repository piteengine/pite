// SPDX-License-Identifier: MIT OR Apache-2.0
//! Headless editor operations: pure tree/scene/error logic with no egui.
//!
//! `app.rs` calls these from UI callbacks and routes every `Err` to the
//! console; nothing here touches the UI, so it is all unit-testable.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use pite_core::{NodeDesc, NodeId, NodeTree, NodeTypeRegistry, PropValue, Props};
use pite_scene::{SceneDoc, SceneInstance, SceneNode, SceneScript, FORMAT_VERSION};

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

/// Place `id` under `new_parent` at child `index` (clamped to the end).
/// Unlike `move_node` this controls sibling order: index 0 makes it first.
/// Refuses unknown nodes, root moves, and cycles (under itself or a
/// descendant); the node keeps its subtree.
pub fn place_node(
    tree: &mut NodeTree,
    id: &NodeId,
    new_parent: &NodeId,
    index: usize,
) -> Result<()> {
    if tree.root() == Some(id) {
        anyhow::bail!("cannot move the root node `{id}`");
    }
    let node = tree
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` does not exist"))?;
    if !tree.contains(new_parent) {
        anyhow::bail!("parent `{new_parent}` does not exist");
    }
    if new_parent == id {
        anyhow::bail!("cannot place `{id}` under itself");
    }
    let mut cursor = Some(new_parent.clone());
    while let Some(current) = cursor {
        if &current == id {
            anyhow::bail!("cannot place `{id}` under its descendant `{new_parent}`");
        }
        cursor = tree.get(&current).and_then(|n| n.parent.clone());
    }
    let old_parent = node.parent.clone();
    if let Some(old) = &old_parent {
        if let Some(n) = tree.get_mut(old) {
            n.children.retain(|c| c != id);
        }
    }
    let node_parent = tree
        .get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` does not exist"))?;
    node_parent.parent = Some(new_parent.clone());
    let host = tree
        .get_mut(new_parent)
        .ok_or_else(|| anyhow::anyhow!("parent `{new_parent}` does not exist"))?;
    let at = index.min(host.children.len());
    host.children.insert(at, id.clone());
    Ok(())
}

/// Shift `id` by `delta` slots among its siblings (negative moves up).
/// Out-of-range shifts clamp to the first/last slot; parentless nodes and
/// the root are errors.
pub fn move_sibling(tree: &mut NodeTree, id: &NodeId, delta: i32) -> Result<()> {
    let node = tree
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` does not exist"))?;
    let parent = node
        .parent
        .clone()
        .ok_or_else(|| anyhow::anyhow!("node `{id}` has no parent to reorder in"))?;
    let siblings = tree.children_of(&parent);
    let pos = siblings
        .iter()
        .position(|s| s == id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` is not listed under its parent"))?;
    let len = siblings.len() as i32;
    let at = (pos as i32 + delta).clamp(0, len - 1) as usize;
    place_node(tree, id, &parent, at)
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
    }
}

/// Paths needed to resolve `[[instance]]` scene references while saving.
#[derive(Clone, Copy)]
pub struct InstanceRefs<'a> {
    pub scene_dir: &'a Path,
    pub project_dir: Option<&'a Path>,
}

/// Build a saveable [`SceneDoc`] from the live tree, in tree order.
///
/// `source` is the scene file as it exists on disk and stays authoritative:
/// its `[[instance]]` entries are re-attached instead of being flattened into
/// plain nodes (a live tree cannot tell instantiated nodes from authored
/// ones). Prop edits inside an instantiated subtree are written back as
/// `overrides`, so a save round-trips exactly; edits the format cannot
/// express (renaming, retyping, reparenting or deleting inside a subtree,
/// removing an inherited prop) fail loudly instead of being dropped.
pub fn build_doc(
    tree: &NodeTree,
    source: Option<&SceneDoc>,
    refs: Option<InstanceRefs<'_>>,
) -> Result<SceneDoc> {
    if let Some(source) = source {
        if !source.instance.is_empty() && refs.is_none() {
            anyhow::bail!(
                "cannot save: {} instance(s) need scene/project paths to resolve",
                source.instance.len()
            );
        }
    }
    let root = tree.root().map(ToString::to_string).unwrap_or_default();
    let mut owned: HashSet<String> = HashSet::new();
    let mut instance: Vec<SceneInstance> = Vec::new();
    if let (Some(source), Some(refs)) = (source, refs) {
        for inst in &source.instance {
            let plan = plan_instance(inst, tree, &root, refs)
                .with_context(|| format!("cannot save instance of {}", inst.scene))?;
            owned.extend(plan.owned);
            instance.push(plan.entry);
        }
    }
    let mut node = Vec::new();
    for n in tree.iter() {
        if owned.contains(n.id.as_str()) {
            continue;
        }
        if let Some(parent) = &n.parent {
            if owned.contains(parent.as_str()) {
                anyhow::bail!(
                    "node {:?} is parented inside an instanced subtree; the scene format \
                     cannot express that",
                    n.id
                );
            }
        }
        node.push(SceneNode {
            id: n.id.to_string(),
            type_name: n.type_name.clone(),
            name: n.name.clone(),
            parent: n.parent.clone().map(|p| p.to_string()),
            props: props_to_toml(&n.props),
            script: n.script.as_ref().map(|s| SceneScript {
                path: s.path.clone(),
                class: s.class_name.clone(),
            }),
        });
    }
    if let Some(source) = source {
        reject_foreign_nodes(tree, source, &owned)?;
    }
    Ok(SceneDoc {
        format_version: FORMAT_VERSION,
        root,
        node,
        instance,
    })
}

fn props_to_toml(props: &Props) -> BTreeMap<String, toml::Value> {
    let mut out = BTreeMap::new();
    for (k, v) in &props.0 {
        out.insert(k.clone(), prop_to_toml(v));
    }
    out
}

/// A re-attached instance entry plus the live ids it owns.
struct InstancePlan {
    entry: SceneInstance,
    owned: HashSet<String>,
}

fn resolve_instance_path(scene_ref: &str, refs: InstanceRefs<'_>) -> Result<PathBuf> {
    if let Some(rel) = scene_ref.strip_prefix("res://") {
        let root = refs
            .project_dir
            .context("instance uses res:// but no project root is known")?;
        Ok(root.join(rel))
    } else {
        let p = Path::new(scene_ref);
        if p.is_absolute() {
            Ok(p.to_path_buf())
        } else {
            Ok(refs.scene_dir.join(p))
        }
    }
}

/// Re-derive one `[[instance]]` entry from the live tree: the subtree must
/// still match the referenced scene node-for-node, and every prop that differs
/// from that scene becomes an override.
fn plan_instance(
    inst: &SceneInstance,
    tree: &NodeTree,
    doc_root: &str,
    refs: InstanceRefs<'_>,
) -> Result<InstancePlan> {
    let path = resolve_instance_path(&inst.scene, refs)?;
    let ref_doc = pite_scene::load_scene(&path)?;
    let mut owned = HashSet::new();
    let mut overrides: BTreeMap<String, toml::Value> = BTreeMap::new();
    for ref_node in &ref_doc.node {
        let live_id = format!("{}{}", inst.prefix, ref_node.id);
        let live = tree
            .get(&NodeId::from(live_id.clone()))
            .with_context(|| format!("live tree is missing `{live_id}`"))?;
        owned.insert(live_id.clone());
        if live.type_name != ref_node.type_name || live.name != ref_node.name {
            anyhow::bail!(
                "`{live_id}` was renamed or retyped; overrides carry props only"
            );
        }
        let live_script = live
            .script
            .as_ref()
            .map(|s| (s.path.as_str(), s.class_name.as_str()));
        let ref_script = ref_node
            .script
            .as_ref()
            .map(|s| (s.path.as_str(), s.class.as_str()));
        if live_script != ref_script {
            anyhow::bail!("script on `{live_id}` cannot be overridden per instance");
        }
        if ref_node.id != ref_doc.root {
            let expected = ref_node.parent.as_ref().map(|p| format!("{}{p}", inst.prefix));
            if live.parent.as_ref().map(ToString::to_string) != expected {
                anyhow::bail!("`{live_id}` was reparented inside the instance");
            }
        }
        for key in ref_node.props.keys() {
            if !live.props.0.contains_key(key) {
                anyhow::bail!(
                    "prop `{key}` was removed from `{live_id}`; an instance cannot \
                     unset an inherited prop"
                );
            }
        }
        for (key, value) in props_to_toml(&live.props) {
            if ref_node.props.get(&key) != Some(&value) {
                overrides.insert(format!("{}.{}", ref_node.id, key), value);
            }
        }
    }
    let subtree_root = format!("{}{}", inst.prefix, ref_doc.root);
    let attach = tree
        .get(&NodeId::from(subtree_root.clone()))
        .and_then(|n| n.parent.clone())
        .map(|p| p.to_string());
    let parent = match (&inst.parent, attach.as_deref()) {
        (Some(original), Some(current)) if original == current => Some(original.clone()),
        (None, Some(current)) if current == doc_root => None,
        (_, Some(current)) => Some(current.to_string()),
        (_, None) => None,
    };
    Ok(InstancePlan {
        entry: SceneInstance {
            scene: inst.scene.clone(),
            parent,
            prefix: inst.prefix.clone(),
            overrides,
        },
        owned,
    })
}

/// A node that belongs to no source entry and hangs under an instanced node
/// was added inside an instance — unrepresentable, so refuse the save.
fn reject_foreign_nodes(
    tree: &NodeTree,
    source: &SceneDoc,
    owned: &HashSet<String>,
) -> Result<()> {
    let authored: HashSet<&str> = source.node.iter().map(|n| n.id.as_str()).collect();
    for n in tree.iter() {
        let id = n.id.as_str();
        if authored.contains(id) || owned.contains(id) {
            continue;
        }
        let mut cursor = n.parent.clone();
        while let Some(parent) = cursor {
            if owned.contains(parent.as_str()) {
                anyhow::bail!(
                    "node `{id}` was added inside an instanced subtree; the scene \
                     format cannot express that"
                );
            }
            cursor = tree.get(&parent).and_then(|p| p.parent.clone());
        }
    }
    Ok(())
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

    const ENEMY_SCENE: &str = r#"format_version = 1
root = "enemy"

[[node]]
id = "enemy"
type = "Node2D"
name = "Enemy"

[[node]]
id = "sprite"
type = "Sprite2D"
name = "Sprite"
parent = "enemy"

[node.props]
position = [0.0, 0.0]
"#;

    const HOST_SCENE: &str = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "player"
type = "Sprite2D"
name = "Player"
parent = "root"

[node.props]
texture = "res://assets/player.png"

[[instance]]
scene = "res://scenes/enemy.pitescene"
parent = "root"
prefix = "e1_"

[instance.overrides]
"sprite.position" = [300.0, 120.0]
"#;

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        /// Project with `host.pitescene` instantiating `enemy.pitescene`.
        fn with_instances(tag: &str) -> Self {
            let n = FIXTURE_COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "pite-ops-inst-{tag}-{}-{n}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("scenes")).expect("create scenes dir");
            std::fs::write(dir.join("scenes").join("enemy.pitescene"), ENEMY_SCENE)
                .expect("write enemy scene");
            std::fs::write(dir.join("scenes").join("host.pitescene"), HOST_SCENE)
                .expect("write host scene");
            Self { dir }
        }

        fn host_path(&self) -> PathBuf {
            self.dir.join("scenes").join("host.pitescene")
        }

        fn scene_dir(&self) -> PathBuf {
            self.dir.join("scenes")
        }

        /// Load the host scene into a live tree, exactly like the editor does.
        fn live_tree(&self) -> (SceneDoc, NodeTree) {
            let doc = pite_scene::load_scene(&self.host_path()).expect("host parses");
            let tree = pite_scene::build_tree(&doc, &self.scene_dir(), Some(&self.dir))
                .expect("tree builds");
            (doc, tree)
        }

        /// The editor's save step: live tree + on-disk source -> document.
        fn save(&self, tree: &NodeTree, source: &SceneDoc) -> Result<SceneDoc> {
            build_doc(
                tree,
                Some(source),
                Some(InstanceRefs {
                    scene_dir: &self.scene_dir(),
                    project_dir: Some(&self.dir),
                }),
            )
        }

        fn save_text(&self, tree: &NodeTree, source: &SceneDoc) -> String {
            pite_scene::save_scene(&self.save(tree, source).expect("save builds")).expect("serializes")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    #[test]
    fn instance_entries_are_not_flattened_into_nodes() {
        let f = Fixture::with_instances("keep");
        let (source, tree) = f.live_tree();
        assert!(tree.contains(&NodeId::from("e1_enemy".to_string())));
        let doc = f.save(&tree, &source).unwrap();

        assert_eq!(doc.instance.len(), 1, "instance entry must survive save");
        let inst = &doc.instance[0];
        assert_eq!(inst.scene, "res://scenes/enemy.pitescene");
        assert_eq!(inst.prefix, "e1_");
        assert_eq!(inst.parent.as_deref(), Some("root"));
        let ids: Vec<&str> = doc.node.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec!["root", "player"], "instance nodes must not be inlined");
        let text = pite_scene::save_scene(&doc).unwrap();
        assert!(text.contains("[[instance]]"), "serialized doc lost its instance");
        assert!(!text.contains("e1_enemy"), "instance subtree leaked into nodes");
    }

    #[test]
    fn overrides_survive_and_edits_become_overrides() {
        let f = Fixture::with_instances("override");
        let (source, mut tree) = f.live_tree();
        let doc = f.save(&tree, &source).unwrap();
        assert_eq!(
            doc.instance[0].overrides.get("sprite.position"),
            Some(&toml::Value::Array(vec![
                toml::Value::Float(300.0),
                toml::Value::Float(120.0)
            ])),
            "original override must be preserved verbatim"
        );

        set_prop(
            &mut tree,
            &NodeId::from("e1_sprite".to_string()),
            "position",
            PropValue::Vec2(1.0, 2.0),
        )
        .unwrap();
        set_prop(
            &mut tree,
            &NodeId::from("player".to_string()),
            "position",
            PropValue::Vec2(9.0, 9.0),
        )
        .unwrap();
        let doc = f.save(&tree, &source).unwrap();
        assert_eq!(
            doc.instance[0].overrides.get("sprite.position"),
            Some(&toml::Value::Array(vec![
                toml::Value::Float(1.0),
                toml::Value::Float(2.0)
            ])),
            "edited instance prop must be written back as an override"
        );
        let player = doc.node.iter().find(|n| n.id == "player").unwrap();
        assert!(
            player.props.contains_key("position"),
            "authored prop edits belong to the node, not to overrides"
        );
    }

    #[test]
    fn instance_save_is_byte_identical_on_reload() {
        let f = Fixture::with_instances("stable");
        let (source, tree) = f.live_tree();
        let first = f.save_text(&tree, &source);

        let reparsed = pite_scene::parse_scene_str(&first).unwrap();
        let tree2 = pite_scene::build_tree(&reparsed, &f.scene_dir(), Some(&f.dir)).unwrap();
        let second = f.save_text(&tree2, &reparsed);
        assert_eq!(first, second, "save -> load -> save must be byte-identical");
    }

    #[test]
    fn plain_scene_save_is_byte_identical_and_unaffected() {
        let mut tree = rooted_tree();
        let root = tree.root().cloned().unwrap();
        for name in ["alpha", "beta", "gamma"] {
            set_prop(&mut tree, &root, name, PropValue::Str(format!("v{name}"))).unwrap();
        }
        add_node(&mut tree, Some(root.clone()), "Label", "Score").unwrap();
        let doc = build_doc(&tree, None, None).unwrap();
        assert!(doc.instance.is_empty());
        let first = pite_scene::save_scene(&doc).unwrap();
        let reparsed = pite_scene::parse_scene_str(&first).unwrap();
        let again = pite_scene::save_scene(&reparsed).unwrap();
        assert_eq!(first, again, "plain scenes must serialize deterministically");
    }

    #[test]
    fn structural_edits_inside_an_instance_fail_loudly() {
        let f = Fixture::with_instances("drift");
        let (source, tree) = f.live_tree();
        let enemy = NodeId::from("e1_enemy".to_string());

        let mut added = tree;
        add_node(&mut added, Some(enemy.clone()), "Timer", "Extra").unwrap();
        let err = f.save(&added, &source).unwrap_err();
        assert!(
            format!("{err:#}").contains("instanced subtree"),
            "unexpected: {err:#}"
        );

        let mut removed = f.live_tree().1;
        remove_node(&mut removed, &NodeId::from("e1_sprite".to_string())).unwrap();
        let err = f.save(&removed, &source).unwrap_err();
        assert!(
            format!("{err:#}").contains("missing"),
            "unexpected: {err:#}"
        );

        let mut renamed = f.live_tree().1;
        renamed.get_mut(&enemy).unwrap().name = "Boss".to_string();
        let err = f.save(&renamed, &source).unwrap_err();
        assert!(
            format!("{err:#}").contains("renamed"),
            "unexpected: {err:#}"
        );
    }

    #[test]
    fn dogfood_scene_save_keeps_its_instance() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("examples")
            .join("minimal-2d");
        let scene = project.join("scenes").join("main.pitescene");
        let source = pite_scene::load_scene(&scene).expect("dogfood scene parses");
        assert_eq!(
            source.instance.len(),
            1,
            "dogfood fixture must keep an instance for this guard to mean anything"
        );
        let tree = pite_scene::build_tree(&source, &project.join("scenes"), Some(&project))
            .expect("dogfood tree builds");
        let refs = InstanceRefs {
            scene_dir: &project.join("scenes"),
            project_dir: Some(&project),
        };
        let doc = build_doc(&tree, Some(&source), Some(refs)).expect("save builds");
        assert_eq!(doc.instance.len(), 1);
        assert_eq!(doc.instance[0].prefix, "e1_");
        assert!(
            doc.instance[0]
                .overrides
                .contains_key("sprite.position"),
            "dogfood override must survive"
        );
        assert!(
            !doc.node.iter().any(|n| n.id.starts_with("e1_")),
            "instance subtree must not be inlined into nodes"
        );
        let first = pite_scene::save_scene(&doc).unwrap();
        let reparsed = pite_scene::parse_scene_str(&first).unwrap();
        let tree2 =
            pite_scene::build_tree(&reparsed, &project.join("scenes"), Some(&project)).unwrap();
        let second = pite_scene::save_scene(
            &build_doc(&tree2, Some(&reparsed), Some(refs)).expect("second save builds"),
        )
        .unwrap();
        assert_eq!(first, second, "dogfood save -> load -> save must be identical");
    }

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
        let doc = build_doc(&tree, None, None).unwrap();
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

    fn children_of(tree: &NodeTree, id: &str) -> Vec<String> {
        tree.children_of(&NodeId::from(id.to_string()))
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn three_kids() -> NodeTree {
        let mut tree = rooted_tree();
        let root = tree.root().cloned().unwrap();
        for name in ["a", "b", "c"] {
            add_node(&mut tree, Some(root.clone()), "Node2D", name).unwrap();
        }
        tree
    }

    #[test]
    fn place_node_reorders_and_moves_across_parents() {
        let mut tree = three_kids();
        let root = tree.root().cloned().unwrap();
        place_node(&mut tree, &NodeId::from("c".to_string()), &root, 0).unwrap();
        assert_eq!(children_of(&tree, "main"), vec!["c", "a", "b"]);
        move_sibling(&mut tree, &NodeId::from("a".to_string()), 1).unwrap();
        assert_eq!(children_of(&tree, "main"), vec!["c", "b", "a"]);
        move_sibling(&mut tree, &NodeId::from("c".to_string()), -5).unwrap();
        assert_eq!(children_of(&tree, "main"), vec!["c", "b", "a"]);
        let sub = add_node(&mut tree, Some(root.clone()), "Node2D", "sub").unwrap();
        place_node(&mut tree, &NodeId::from("a".to_string()), &sub, 0).unwrap();
        assert_eq!(children_of(&tree, "main"), vec!["c", "b", "sub"]);
        assert_eq!(children_of(&tree, &sub.to_string()), vec!["a"]);
    }

    #[test]
    fn place_node_refuses_root_cycles_and_unknowns() {
        let mut tree = three_kids();
        let root = tree.root().cloned().unwrap();
        assert!(place_node(&mut tree, &root, &NodeId::from("a".to_string()), 0).is_err());
        assert!(place_node(
            &mut tree,
            &NodeId::from("a".to_string()),
            &NodeId::from("a".to_string()),
            0
        )
        .is_err());
        place_node(&mut tree, &NodeId::from("b".to_string()), &NodeId::from("a".to_string()), 0)
            .unwrap();
        assert!(place_node(
            &mut tree,
            &NodeId::from("a".to_string()),
            &NodeId::from("b".to_string()),
            0
        )
        .is_err());
        assert!(place_node(
            &mut tree,
            &NodeId::from("nope".to_string()),
            &root,
            0
        )
        .is_err());
    }

    #[test]
    fn unknown_type_is_an_error() {
        let mut tree = rooted_tree();
        let root = tree.root().cloned().unwrap();
        let err = add_node(&mut tree, Some(root), "Nope", "x").unwrap_err();
        assert!(err.to_string().contains("Nope"), "unexpected: {err:#}");
    }
}
