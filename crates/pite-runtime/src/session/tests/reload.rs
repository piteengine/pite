// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

const V1: &str = "import pite\n\nclass Mover(pite.Node2D):\n    def _process(self, delta):\n        x, y = self.position\n        self.position = (x + 100.0 * delta, y)\n";
const V2: &str = "import pite\n\nclass Mover(pite.Node2D):\n    def _process(self, delta):\n        x, y = self.position\n        self.position = (x + 400.0 * delta, y)\n";

fn tree_vec2(session: &GameSession, node_id: &str, key: &str) -> Option<(f64, f64)> {
    session.host.with_tree(|t| {
        t.get(&NodeId::from(node_id.to_string()))
            .and_then(|n| n.props.get(key))
            .and_then(|v| match v {
                PropValue::Vec2(x, y) => Some((*x, *y)),
                _ => None,
            })
    })
}

fn drive_x(session: &mut GameSession) -> f64 {
    session.update(0.1);
    session
        .host
        .with_tree(|t| match t.get(&NodeId::from("mover".to_string())) {
            Some(n) => match n.props.get("position") {
                Some(PropValue::Vec2(x, _)) => *x,
                _ => f64::NAN,
            },
            None => f64::NAN,
        })
}

#[test]
fn editing_script_changes_behavior_without_restart() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (dir, script) = make_project("reload", "Mover", V1);
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, false).unwrap();
    assert_eq!(session.scripted_count(), 1);
    let x1 = drive_x(&mut session);
    assert!((x1 - 10.0).abs() < 1e-6);

    std::fs::write(&script, V2).unwrap();
    let mut reloaded = false;
    for _ in 0..100 {
        session.poll_watch();
        let before = drive_x(&mut session);
        let after = drive_x(&mut session);
        if (after - before - 40.0).abs() < 1e-6 {
            reloaded = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(reloaded, "v2 behavior never took effect");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scene_edit_adds_nodes_live() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (dir, _) = make_project("scene", "Mover", V1);
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, false).unwrap();
    assert_eq!(session.tree_len(), 2);
    std::fs::write(
        &scene,
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"extra\"\ntype = \"Timer\"\nname = \"Extra\"\nparent = \"root\"\n",
    )
    .unwrap();
    let mut grown = false;
    for _ in 0..100 {
        session.poll_watch();
        if session.tree_len() == 2 && session.scripted_count() == 0 {
            grown = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(grown, "rebuilt tree never applied");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn no_reload_disables_watching() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (dir, _) = make_project("noreload", "Mover", V1);
    let scene = dir.join("scenes").join("main.ptscn");
    let session = GameSession::open(&scene, true).unwrap();
    assert!(session.watch_rx.is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unrelated_scene_edit_does_not_rebuild() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (dir, _) = make_project("unrelated", "Mover", V1);
    let scene = dir.join("scenes").join("main.ptscn");
    let other = dir.join("scenes").join("other.ptscn");
    std::fs::write(
        &other,
        "format_version = 1\nroot = \"other\"\n\n[[node]]\nid = \"other\"\ntype = \"Node2D\"\nname = \"Other\"\n",
    )
    .unwrap();
    let mut session = GameSession::open(&scene, false).unwrap();
    assert_eq!(session.tree_len(), 2);

    std::fs::write(
        &other,
        "format_version = 1\nroot = \"other2\"\n\n[[node]]\nid = \"other2\"\ntype = \"Node2D\"\nname = \"Other2\"\n",
    )
    .unwrap();
    for _ in 0..10 {
        session.poll_watch();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(session.tree_len(), 2);
    assert_eq!(session.scripted_count(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn script_reload_preserves_custom_attributes_and_velocity() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let src1 = "import pite\n\nclass Char(pite.CharacterBody2D):\n    def _ready(self):\n        self.score = 42\n        self.velocity = (123.0, 456.0)\n";
    let src2 = "import pite\n\nclass Char(pite.CharacterBody2D):\n    def _process(self, delta):\n        pass\n";
    let (dir, script) = make_project("char-preserve", "Char", src1);
    let scene = dir.join("scenes").join("main.ptscn");
    std::fs::write(
        &scene,
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"mover\"\ntype = \"CharacterBody2D\"\nname = \"Char\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/mover.py\"\nclass = \"Char\"\n",
    )
    .unwrap();
    let mut session = GameSession::open(&scene, false).unwrap();
    session.update(0.016);

    use pite_script::pyo3::{self, types::PyAnyMethods};
    pyo3::Python::attach(|py| {
        let inst = session.host.lookup_instance(py, "mover").unwrap();
        let score: i32 = inst.bind(py).getattr("score").unwrap().extract().unwrap();
        assert_eq!(score, 42);
    });
    assert_eq!(
        tree_vec2(&session, "mover", "velocity"),
        Some((123.0, 456.0))
    );

    std::fs::write(&script, src2).unwrap();
    let mut reloaded = false;
    for _ in 0..100 {
        if session.poll_watch() {
            reloaded = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(reloaded, "script reload never detected");

    pyo3::Python::attach(|py| {
        let inst = session.host.lookup_instance(py, "mover").unwrap();
        let score: i32 = inst.bind(py).getattr("score").unwrap().extract().unwrap();
        assert_eq!(score, 42);
    });
    assert_eq!(
        tree_vec2(&session, "mover", "velocity"),
        Some((123.0, 456.0))
    );
    std::fs::remove_dir_all(&dir).ok();
}
