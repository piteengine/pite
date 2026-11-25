// SPDX-License-Identifier: MIT OR Apache-2.0
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
scene = "res://scenes/enemy.ptscn"
parent = "root"
prefix = "e1_"

[instance.overrides]
"sprite.position" = [300.0, 120.0]
"#;

struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    /// Project with `host.ptscn` instantiating `enemy.ptscn`.
    fn with_instances(tag: &str) -> Self {
        let n = FIXTURE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("pite-ops-inst-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).expect("create scenes dir");
        std::fs::write(dir.join("scenes").join("enemy.ptscn"), ENEMY_SCENE)
            .expect("write enemy scene");
        std::fs::write(dir.join("scenes").join("host.ptscn"), HOST_SCENE)
            .expect("write host scene");
        Self { dir }
    }

    fn host_path(&self) -> PathBuf {
        self.dir.join("scenes").join("host.ptscn")
    }

    fn scene_dir(&self) -> PathBuf {
        self.dir.join("scenes")
    }

    /// Load the host scene into a live tree, exactly like the editor does.
    fn live_tree(&self) -> (SceneDoc, NodeTree) {
        let doc = pite_scene::load_scene(&self.host_path()).expect("host parses");
        let tree =
            pite_scene::build_tree(&doc, &self.scene_dir(), Some(&self.dir)).expect("tree builds");
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
    assert_eq!(inst.scene, "res://scenes/enemy.ptscn");
    assert_eq!(inst.prefix, "e1_");
    assert_eq!(inst.parent.as_deref(), Some("root"));
    let ids: Vec<&str> = doc.node.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["root", "player"],
        "instance nodes must not be inlined"
    );
    let text = pite_scene::save_scene(&doc).unwrap();
    assert!(
        text.contains("[[instance]]"),
        "serialized doc lost its instance"
    );
    assert!(
        !text.contains("e1_enemy"),
        "instance subtree leaked into nodes"
    );
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
    assert_eq!(
        first, again,
        "plain scenes must serialize deterministically"
    );
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
        .join("../..")
        .join("examples")
        .join("minimal-2d");
    let scene = project.join("scenes").join("main.ptscn");
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
        doc.instance[0].overrides.contains_key("sprite.position"),
        "dogfood override must survive"
    );
    assert!(
        !doc.node.iter().any(|n| n.id.starts_with("e1_")),
        "instance subtree must not be inlined into nodes"
    );
    let first = pite_scene::save_scene(&doc).unwrap();
    let reparsed = pite_scene::parse_scene_str(&first).unwrap();
    let tree2 = pite_scene::build_tree(&reparsed, &project.join("scenes"), Some(&project)).unwrap();
    let second = pite_scene::save_scene(
        &build_doc(&tree2, Some(&reparsed), Some(refs)).expect("second save builds"),
    )
    .unwrap();
    assert_eq!(
        first, second,
        "dogfood save -> load -> save must be identical"
    );
}

#[test]
fn gem_collector_scene_save_is_byte_identical() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples")
        .join("gem-collector");
    let scene = project.join("scenes").join("main.ptscn");
    let source = pite_scene::load_scene(&scene).expect("gem-collector scene parses");
    let tree = pite_scene::build_tree(&source, &project.join("scenes"), Some(&project))
        .expect("tree builds");
    let refs = InstanceRefs {
        scene_dir: &project.join("scenes"),
        project_dir: Some(&project),
    };
    let doc = build_doc(&tree, Some(&source), Some(refs)).expect("save builds");
    let first = pite_scene::save_scene(&doc).unwrap();
    let on_disk = std::fs::read_to_string(&scene).unwrap();
    assert_eq!(first, on_disk, "gem-collector scene save must match disk");
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
    set_prop(&mut tree, &root, "title", PropValue::Str("hi".to_string())).unwrap();
    let node = tree.get(&root).unwrap();
    assert_eq!(node.props.get("speed"), Some(&PropValue::Num(3.5)));
    assert_eq!(
        node.props.get("title"),
        Some(&PropValue::Str("hi".to_string()))
    );
    remove_prop(&mut tree, &root, "speed").unwrap();
    let node2 = tree.get(&root).unwrap();
    assert_eq!(node2.props.get("speed"), None);
}

#[test]
fn build_save_parse_round_trip_preserves_nodes_and_props() {
    let mut tree = rooted_tree();
    let root = tree.root().cloned().unwrap();
    set_prop(&mut tree, &root, "position", PropValue::Vec2(100.0, 200.0)).unwrap();
    let child = add_node(&mut tree, Some(root), "Label", "Score").unwrap();
    set_prop(&mut tree, &child, "text", PropValue::Str("0".to_string())).unwrap();
    let doc = build_doc(&tree, None, None).unwrap();
    let text = pite_scene::save_scene(&doc).unwrap();
    let again = pite_scene::parse_scene_str(&text).unwrap();
    assert_eq!(again.node.len(), 2);
    assert_eq!(again.root, doc.root);
    let player = again
        .node
        .iter()
        .find(|n| n.id == child.to_string())
        .unwrap();
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
    assert_eq!(children_of(&tree, sub.as_ref()), vec!["a"]);
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
    place_node(
        &mut tree,
        &NodeId::from("b".to_string()),
        &NodeId::from("a".to_string()),
        0,
    )
    .unwrap();
    assert!(place_node(
        &mut tree,
        &NodeId::from("a".to_string()),
        &NodeId::from("b".to_string()),
        0
    )
    .is_err());
    assert!(place_node(&mut tree, &NodeId::from("nope".to_string()), &root, 0).is_err());
}

#[test]
fn unknown_type_is_an_error() {
    let mut tree = rooted_tree();
    let root = tree.root().cloned().unwrap();
    let err = add_node(&mut tree, Some(root), "Nope", "x").unwrap_err();
    assert!(err.to_string().contains("Nope"), "unexpected: {err:#}");
}

#[test]
fn rename_node_updates_name() {
    let mut tree = rooted_tree();
    let root = tree.root().cloned().unwrap();
    let child = add_node(&mut tree, Some(root), "Sprite2D", "Player").unwrap();
    rename_node(&mut tree, &child, "Hero").unwrap();
    assert_eq!(tree.get(&child).unwrap().name, "Hero");
    assert!(rename_node(&mut tree, &child, "  ").is_err());
}

#[test]
fn duplicate_node_clones_subtree_and_places_sibling() {
    let mut tree = rooted_tree();
    let root = tree.root().cloned().unwrap();
    let parent_node = add_node(&mut tree, Some(root.clone()), "Node2D", "Group").unwrap();
    let child_node = add_node(&mut tree, Some(parent_node.clone()), "Sprite2D", "Coin").unwrap();
    set_prop(&mut tree, &child_node, "value", PropValue::Int(10)).unwrap();

    let dup_id = duplicate_node(&mut tree, &parent_node).unwrap();
    assert_ne!(dup_id, parent_node);
    assert_eq!(
        tree.children_of(&root),
        vec![parent_node.clone(), dup_id.clone()]
    );
    let dup_children = tree.children_of(&dup_id);
    assert_eq!(dup_children.len(), 1);
    let dup_child = &dup_children[0];
    assert_eq!(
        tree.get(dup_child).unwrap().props.get("value"),
        Some(&PropValue::Int(10))
    );
}
