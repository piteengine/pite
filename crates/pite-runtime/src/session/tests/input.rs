// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

fn btn_text(session: &GameSession) -> String {
    session
        .host
        .with_tree(|t| match t.get(&NodeId::from("btn".to_string())) {
            Some(n) => match n.props.get("text") {
                Some(PropValue::Str(s)) => s.clone(),
                _ => String::new(),
            },
            None => String::from("<gone>"),
        })
}

fn click(session: &mut GameSession, down: (f64, f64), up: (f64, f64)) {
    use pite_script::{input_begin_frame, input_set_key, input_set_mouse};
    let (w, h) = session.design_size().unwrap_or((800, 600));
    session.set_viewport(w, h);
    input_set_mouse(down.0, down.1);
    input_set_key("MouseLeft", true);
    session.update(0.016);
    input_begin_frame();
    input_set_mouse(up.0, up.1);
    input_set_key("MouseLeft", false);
    session.update(0.016);
    input_begin_frame();
}

fn walker_x(session: &GameSession) -> f64 {
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

fn make_button_project(tag: &str) -> PathBuf {
    const COUNTER: &str = "import pite\n\nclass Counter(pite.Button):\n    pressed = pite.signal()\n    def _ready(self):\n        self.text = \"0\"\n        self.get_node(\".\").pressed.connect(self.on_pressed)\n    def on_pressed(self):\n        self.text = str(int(self.text) + 1)\n";

    make_button_project_with(tag, COUNTER)
}

fn make_button_project_with(tag: &str, script_src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pite-sess-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"btn\"\ntype = \"Button\"\nname = \"HitBtn\"\nparent = \"root\"\n\n[node.props]\nposition = [100.0, 100.0]\nsize = [120.0, 40.0]\ntext = \"0\"\n\n[node.script]\npath = \"res://scripts/counter.py\"\nclass = \"Counter\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("scripts").join("counter.py"), script_src).unwrap();
    dir
}

#[test]
fn click_inside_fires_once_per_click() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = make_button_project("btn-once");
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    assert_eq!(btn_text(&session), "0");
    click(&mut session, (740.0, 460.0), (740.0, 460.0));
    assert_eq!(btn_text(&session), "1");
    click(&mut session, (740.0, 460.0), (740.0, 460.0));
    assert_eq!(btn_text(&session), "2");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn button_pressed_needs_no_script_declaration() {
    const COUNTER_BUILTIN: &str = "import pite\n\nclass Counter(pite.Button):\n    def _ready(self):\n        self.text = \"0\"\n        self.get_node(\".\").pressed.connect(self.on_pressed)\n    def on_pressed(self):\n        self.text = str(int(self.text) + 1)\n";

    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = make_button_project_with("btn-builtin", COUNTER_BUILTIN);
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    assert!(session.host.has_signal("btn", "pressed"));
    assert_eq!(btn_text(&session), "0");
    click(&mut session, (740.0, 460.0), (740.0, 460.0));
    assert_eq!(btn_text(&session), "1");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn click_outside_is_ignored() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = make_button_project("btn-out");
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    click(&mut session, (100.0, 100.0), (100.0, 100.0));
    assert_eq!(btn_text(&session), "0");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn press_inside_release_outside_is_ignored() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = make_button_project("btn-drag");
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    click(&mut session, (740.0, 460.0), (100.0, 100.0));
    assert_eq!(btn_text(&session), "0");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dropped_button_disconnects_silently() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = make_button_project("btn-drop");
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    session.host.with_tree_mut(|t| {
        t.remove(&NodeId::from("btn".to_string())).unwrap();
    });
    session.host.disconnect_node("btn");
    assert!(!session.host.has_signal("btn", "pressed"));
    click(&mut session, (740.0, 460.0), (740.0, 460.0));
    assert_eq!(btn_text(&session), "<gone>");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn keypress_moves_node_through_real_input() {
    const WALKER: &str = "import pite\n\nclass Walker(pite.Node2D):\n    def _process(self, delta):\n        if pite.held.right:\n            self.position.x += 200.0 * delta\n        if pite.pressed.space:\n            self.position.x += 10.0\n";

    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    use pite_script::{input_begin_frame, input_set_key};

    let (dir, _) = make_project("input", "Walker", WALKER);
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();

    input_begin_frame();
    input_set_key("ArrowRight", true);
    session.update(0.5);
    assert!((walker_x(&session) - 100.0).abs() < 1e-6);

    input_begin_frame();
    input_set_key("Space", true);
    session.update(0.5);
    let after_step = walker_x(&session);
    assert!((after_step - 210.0).abs() < 1e-6);

    input_begin_frame();
    session.update(0.5);
    assert!((walker_x(&session) - 310.0).abs() < 1e-6);
    input_set_key("ArrowRight", false);
    input_set_key("Space", false);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn frame_delivers_production_order_edges_exactly_once() {
    const STEPPER: &str = "import pite\n\nclass Walker(pite.Node2D):\n    def _process(self, delta):\n        if pite.pressed.space:\n            self.position.x += 10.0\n";

    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    use pite_script::input_set_key;

    let (dir, _) = make_project("edgeorder", "Walker", STEPPER);
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();

    input_set_key("Space", true);
    session.frame(0.5);
    assert!((walker_x(&session) - 10.0).abs() < 1e-6);
    session.frame(0.5);
    assert!(
        (walker_x(&session) - 10.0).abs() < 1e-6,
        "edge must be consumed exactly once"
    );
    input_set_key("Space", false);
    pite_script::input_begin_frame();
    std::fs::remove_dir_all(&dir).ok();
}
