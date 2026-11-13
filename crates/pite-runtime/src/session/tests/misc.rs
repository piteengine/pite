// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::resolve_default_scene;

use super::*;

fn node_x(session: &GameSession, id: &str) -> f64 {
    session
        .host
        .with_tree(|t| match t.get(&NodeId::from(id.to_string())) {
            Some(n) => match n.props.get("position") {
                Some(PropValue::Vec2(x, _)) => *x,
                _ => f64::NAN,
            },
            None => f64::NAN,
        })
}

#[test]
fn default_scene_resolves_main_scene_and_fails_outside_projects() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-defscene-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("pite.toml"),
        "[project]\nname = \"t\"\nmain_scene = \"res://scenes/main.ptscn\"\n",
    )
    .unwrap();
    let prev = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();
    let got = resolve_default_scene().unwrap();
    assert_eq!(got, dir.join("scenes").join("main.ptscn"));
    let bare = std::env::temp_dir().join(format!("pite-noproj-{}", std::process::id()));
    std::fs::create_dir_all(&bare).unwrap();
    std::env::set_current_dir(&bare).unwrap();
    let err = resolve_default_scene().expect_err("outside a project must fail loudly");
    assert!(err.to_string().contains("pite.toml"), "got {err:#}");
    std::env::set_current_dir(&prev).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&bare).ok();
}

#[test]
fn bundle_probe_child() {
    if std::env::var("PITE_BUNDLE_PROBE").is_err() {
        eprintln!("skipped: set PITE_BUNDLE_PROBE=1 in the child process");
        return;
    }
    let dir = std::env::temp_dir().join("pite-bundle-probe-project");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"probe\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"probe\"\ntype = \"Label\"\nname = \"Probe\"\nparent = \"root\"\n\n[node.props]\ntext = \"before\"\n\n[node.script]\npath = \"res://scripts/probe.py\"\nclass = \"Probe\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scripts").join("probe.py"),
        "import pite\n\n\nclass Probe(pite.Label):\n    def _process(self, delta):\n        self.text = \"bundled\"\n",
    )
    .unwrap();
    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    session.update(0.016);
    let text = session
        .host
        .with_tree(|t| match t.get(&NodeId::from("probe".to_string())) {
            Some(n) => match n.props.get("text") {
                Some(PropValue::Str(s)) => s.clone(),
                _ => String::new(),
            },
            None => String::new(),
        });
    assert_eq!(text, "bundled", "script did not run under this interpreter");
    println!("BUNDLE_PROBE_OK");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dogfood_player_hit_reaches_enemy() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    use pite_script::{input_begin_frame, input_set_key};

    let scene = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples")
        .join("minimal-2d")
        .join("scenes")
        .join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    assert_eq!(session.scripted_count(), 3);
    // Every node gets a slot, scripted or not. The dogfood scene has 5
    // nodes in main.ptscn + 2 from the instanced enemy.ptscn = 7.
    assert_eq!(session.instance_count(), 7);
    input_begin_frame();
    input_set_key("Space", true);
    session.update(0.016);
    input_set_key("Space", false);
    let after = node_x(&session, "e1_enemy");
    assert!(
        (after + 5.0).abs() < 1e-6,
        "enemy should recoil to x=-5, got {after}"
    );
}

/// Sanity-check that every tree node reaches `update()` via an
/// instance slot. A scriptless Sprite2D costs zero Python calls
/// (`has_process = false`), but its slot still exists so future
/// engine-owned behaviors can attach to it.
#[test]
fn scriptless_nodes_still_get_instances() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-sess-scriptless-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"spr\"\ntype = \"Sprite2D\"\nname = \"Spr\"\nparent = \"root\"\n\n[[node]]\nid = \"lbl\"\ntype = \"Label\"\nname = \"Lbl\"\nparent = \"root\"\n",
    )
    .unwrap();
    let session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    assert_eq!(session.scripted_count(), 0);
    assert_eq!(session.instance_count(), 3);
    std::fs::remove_dir_all(&dir).ok();
}

/// End-to-end check that a bare (unscripted) `Timer` advances its
/// `current_elapsed` from `PyTimer::_process`, then resets on fire.
/// This exercises the whole new pipeline: base pyclass → `_process` →
/// prop update → wrap around. It would fail if either the slot
/// instantiation or the `_process` loop regressed.
#[test]
fn scriptless_timer_advances_elapsed() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-sess-bare-timer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"tick\"\ntype = \"Timer\"\nname = \"Tick\"\nparent = \"root\"\n\n[node.props]\nwait_time = 0.1\n",
    )
    .unwrap();
    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    assert_eq!(session.instance_count(), 2);
    assert_eq!(session.scripted_count(), 0);

    session.update(0.05);
    let e1 = session.host.with_tree(|t| {
        t.get(&NodeId::from("tick".to_string()))
            .and_then(|n| n.props.get("current_elapsed"))
            .and_then(|v| match v {
                PropValue::Num(f) => Some(*f),
                _ => None,
            })
            .unwrap_or(f64::NAN)
    });
    assert!((e1 - 0.05).abs() < 1e-6, "expected 0.05, got {e1}");

    session.update(0.06); // crosses 0.1 → resets
    let e2 = session.host.with_tree(|t| {
        t.get(&NodeId::from("tick".to_string()))
            .and_then(|n| n.props.get("current_elapsed"))
            .and_then(|v| match v {
                PropValue::Num(f) => Some(*f),
                _ => None,
            })
            .unwrap_or(f64::NAN)
    });
    assert!(e2.abs() < 1e-6, "expected 0 (reset), got {e2}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_gem_collector_e2e_mechanics() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-sess-gemcol-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();

    let scene = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "timelabel"
type = "Label"
name = "TimeLabel"
parent = "root"

[node.props]
text = "Time: 1"

[[node]]
id = "gametimer"
type = "Timer"
name = "GameTimer"
parent = "root"

[node.props]
wait_time = 0.03

[node.script]
path = "res://scripts/game_timer.py"
class = "GameTimer"

[[node]]
id = "player"
type = "CharacterBody2D"
name = "Player"
parent = "root"

[node.props]
position = [100.0, 100.0]

[node.script]
path = "res://scripts/player.py"
class = "Player"

[[node]]
id = "pcollider"
type = "CollisionShape2D"
name = "Collider"
parent = "player"

[node.props]
shape = "circle"
radius = 20.0

[[node]]
id = "gem"
type = "Area2D"
name = "Gem"
parent = "root"

[node.props]
position = [120.0, 100.0]

[node.script]
path = "res://scripts/gem.py"
class = "Gem"

[[node]]
id = "gcollider"
type = "CollisionShape2D"
name = "Collider"
parent = "gem"

[node.props]
shape = "circle"
radius = 20.0
"#;
    std::fs::write(dir.join("scenes").join("main.ptscn"), scene).unwrap();

    let player_script = r#"import pite

class Player(pite.CharacterBody2D):
    speed: float = 300.0

    def _process(self, delta: float):
        self.position.x += 1.0 * self.speed * delta
"#;
    std::fs::write(dir.join("scripts").join("player.py"), player_script).unwrap();

    let gem_script = r#"import pite

class Gem(pite.Area2D):
    collected: bool = False

    def _ready(self):
        self.body_entered.connect(self.on_body)

    def on_body(self, body_id):
        if body_id == "player":
            self.collected = True
"#;
    std::fs::write(dir.join("scripts").join("gem.py"), gem_script).unwrap();

    let timer_script = r##"import pite

class GameTimer(pite.Timer):
    def _ready(self):
        self.get_node(".").timeout.connect(self.on_tick)

    def on_tick(self):
        label = self.get_node("../TimeLabel")
        label.text = "Time's Up!"
        label.color = "#EF4444"
        player = self.get_node("../Player")
        player.speed = 0.0
"##;
    std::fs::write(dir.join("scripts").join("game_timer.py"), timer_script).unwrap();

    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    for _ in 0..10 {
        session.update(0.016);
    }

    let label_props = session.host.with_tree(|t| {
        let n = t.get(&NodeId::from("timelabel".to_string())).unwrap();
        (n.props.get("text").cloned(), n.props.get("color").cloned())
    });
    assert_eq!(
        label_props.0,
        Some(PropValue::Str("Time's Up!".to_string()))
    );
    assert_eq!(label_props.1, Some(PropValue::Color(239, 68, 68, 255)));

    use pite_script::pyo3::{self, types::PyAnyMethods};
    pyo3::Python::attach(|py| {
        let inst = session.host.lookup_instance(py, "player").unwrap();
        let speed: f64 = inst.bind(py).getattr("speed").unwrap().extract().unwrap();
        assert_eq!(speed, 0.0);
    });

    pyo3::Python::attach(|py| {
        let inst = session.host.lookup_instance(py, "gem").unwrap();
        let collected: bool = inst
            .bind(py)
            .getattr("collected")
            .unwrap()
            .extract()
            .unwrap();
        assert!(
            collected,
            "Gem collision should have fired for moving player"
        );
    });

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn animated_sprite_playback_and_signals_e2e() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let dir = std::env::temp_dir().join(format!("pite-anim-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"anim-test\"\n").unwrap();

    let atlas = r#"{
"texture": "res://assets/sheet.png",
"size": [64, 32],
"frames": {
"f1": { "x": 0, "y": 0, "w": 32, "h": 32 },
"f2": { "x": 32, "y": 0, "w": 32, "h": 32 }
},
"animations": {
"walk": { "frames": ["f1", "f2"], "fps": 10.0, "loop": true },
"attack": { "frames": ["f1", "f2"], "fps": 10.0, "loop": false }
}
}"#;
    std::fs::write(dir.join("assets").join("sheet.atlas.json"), atlas).unwrap();

    let script = r#"import pite

class Hero(pite.AnimatedSprite2D):
    def _ready(self):
        self.finished_name = ""
        self.looped_name = ""
        self.get_node(".").animation_finished.connect(self.on_finished)
        self.get_node(".").animation_looped.connect(self.on_looped)

    def on_finished(self, name):
        self.finished_name = name

    def on_looped(self, name):
        self.looped_name = name
"#;
    std::fs::write(dir.join("scripts").join("hero.py"), script).unwrap();

    let scene = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "hero"
type = "AnimatedSprite2D"
name = "Hero"
parent = "root"

[node.props]
atlas = "res://assets/sheet.atlas.json"
autoplay = "walk"

[node.script]
path = "res://scripts/hero.py"
class = "Hero"
"#;
    std::fs::write(dir.join("scenes").join("main.ptscn"), scene).unwrap();

    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();

    // Autoplay should have set animation = "walk" and playing = true
    let (anim, playing, frame) = session.host.with_tree(|t| {
        let n = t.get(&NodeId::from("hero".to_string())).unwrap();
        let anim = match n.props.get("animation") {
            Some(PropValue::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let playing = match n.props.get("playing") {
            Some(PropValue::Bool(b)) => *b,
            _ => false,
        };
        let frame = match n.props.get("frame") {
            Some(PropValue::Int(i)) => *i,
            _ => -1,
        };
        (anim, playing, frame)
    });
    assert_eq!(anim, "walk");
    assert!(playing);
    assert_eq!(frame, 0);

    // Update by 0.1s (1 frame duration at 10 FPS)
    session.update(0.1);
    let frame1 = session.host.with_tree(|t| {
        let n = t.get(&NodeId::from("hero".to_string())).unwrap();
        match n.props.get("frame") {
            Some(PropValue::Int(i)) => *i,
            _ => -1,
        }
    });
    assert_eq!(frame1, 1, "frame should advance to 1");

    // Update by 0.1s: walk is looping, so wraps to 0 and fires animation_looped
    session.update(0.1);
    let frame2 = session.host.with_tree(|t| {
        let n = t.get(&NodeId::from("hero".to_string())).unwrap();
        match n.props.get("frame") {
            Some(PropValue::Int(i)) => *i,
            _ => -1,
        }
    });
    assert_eq!(frame2, 0, "looping animation should wrap to 0");

    use pite_script::pyo3::{self, types::PyAnyMethods};
    pyo3::Python::attach(|py| {
        let inst = session.host.lookup_instance(py, "hero").unwrap();
        let looped_name: String = inst
            .bind(py)
            .getattr("looped_name")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(
            looped_name, "walk",
            "animation_looped signal should have fired with 'walk'"
        );

        // Test play("attack") via Python method call
        inst.bind(py).call_method1("play", ("attack",)).unwrap();
    });

    let (anim_attack, playing_attack, frame_attack) = session.host.with_tree(|t| {
        let n = t.get(&NodeId::from("hero".to_string())).unwrap();
        let anim = match n.props.get("animation") {
            Some(PropValue::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let playing = match n.props.get("playing") {
            Some(PropValue::Bool(b)) => *b,
            _ => false,
        };
        let frame = match n.props.get("frame") {
            Some(PropValue::Int(i)) => *i,
            _ => -1,
        };
        (anim, playing, frame)
    });
    assert_eq!(anim_attack, "attack");
    assert!(playing_attack);
    assert_eq!(frame_attack, 0);

    // Advance 0.1s -> frame 1
    session.update(0.1);
    // Advance 0.1s -> attack is non-looping, so stops on frame 1 and fires animation_finished
    session.update(0.1);

    let (playing_after, frame_after) = session.host.with_tree(|t| {
        let n = t.get(&NodeId::from("hero".to_string())).unwrap();
        let playing = match n.props.get("playing") {
            Some(PropValue::Bool(b)) => *b,
            _ => false,
        };
        let frame = match n.props.get("frame") {
            Some(PropValue::Int(i)) => *i,
            _ => -1,
        };
        (playing, frame)
    });
    assert!(!playing_after, "non-looping animation should stop playing");
    assert_eq!(frame_after, 1, "should remain on final frame");

    pyo3::Python::attach(|py| {
        let inst = session.host.lookup_instance(py, "hero").unwrap();
        let finished_name: String = inst
            .bind(py)
            .getattr("finished_name")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(
            finished_name, "attack",
            "animation_finished signal should have fired with 'attack'"
        );
        let is_p: bool = inst
            .bind(py)
            .call_method0("is_playing")
            .unwrap()
            .extract()
            .unwrap();
        assert!(!is_p, "is_playing() should be false");
    });

    std::fs::remove_dir_all(&dir).ok();
}
