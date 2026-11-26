use super::*;

fn fixture(dir: &Path, name: &str, text: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

fn test_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pite-py-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn vendor_wheels_lists_sorted_whls() {
    let dir = test_dir("wheels");
    assert!(vendor_wheels(&dir.join("missing")).is_empty());
    std::fs::write(dir.join("b-1.0-py3-none-any.whl"), b"w").unwrap();
    std::fs::write(dir.join("a-2.0-py3-none-any.whl"), b"w").unwrap();
    std::fs::write(dir.join("notes.txt"), b"w").unwrap();
    let got = vendor_wheels(&dir);
    assert_eq!(
        got.iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        vec![
            "a-2.0-py3-none-any.whl".to_string(),
            "b-1.0-py3-none-any.whl".to_string()
        ]
    );
    std::fs::remove_dir_all(&dir).ok();
}

const MOVER: &str = r#"
import pite

class Player(pite.Node2D):
    speed: float = 200.0

    def _ready(self):
        self.position = (100.0, 200.0)

    def _process(self, delta: float):
        x, y = self.position
        self.position = (x + self.speed * delta, y)
"#;

const BOOM: &str = r#"
import pite

class Boom(pite.Node2D):
    def _ready(self):
        raise RuntimeError("ready exploded")

    def _process(self, delta: float):
        raise RuntimeError("process exploded")
"#;

#[test]
fn python_moves_sprite_and_vec2_crosses() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let dir = test_dir("mover");
    let file = fixture(&dir, "player.py", MOVER);
    let mut tree = NodeTree::new();
    tree.insert(NodeDesc::new(
        NodeId::from("player".to_string()),
        "Sprite2D",
    ))
    .unwrap();
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut backend = Pyo3Backend::new("player");
    backend.set_host(host.clone());
    backend.load(&file, "Player").unwrap();
    host.set_prop("player", "position", pite_core::PropValue::Vec2(5.0, 6.0));
    backend.call_ready().unwrap();
    let p = host.get_vec2("player", "position");
    assert_eq!((p.x, p.y), (100.0, 200.0));
    backend.call_process(0.5).unwrap();
    let p = host.get_vec2("player", "position");
    assert_eq!((p.x, p.y), (200.0, 200.0));
    assert!(backend.last_error().is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn exception_is_reported_and_state_survives() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let dir = test_dir("boom");
    let file = fixture(&dir, "boom.py", BOOM);
    let mut tree = NodeTree::new();
    tree.insert(NodeDesc::new(NodeId::from("boom".to_string()), "Node2D"))
        .unwrap();
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut backend = Pyo3Backend::new("boom");
    backend.set_host(host.clone());
    backend.load(&file, "Boom").unwrap();
    host.set_prop("boom", "position", pite_core::PropValue::Vec2(1.0, 2.0));
    let err = backend.call_ready().unwrap_err();
    assert!(err.to_string().contains("ready exploded"));
    assert!(err.to_string().contains("boom.py"));
    let p = host.get_vec2("boom", "position");
    assert_eq!((p.x, p.y), (1.0, 2.0));
    assert!(backend.last_error().is_some());
    let err = backend.call_process(0.016).unwrap_err();
    assert!(err.to_string().contains("process exploded"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_method_is_noop() {
    let dir = test_dir("plain");
    let file = fixture(
        &dir,
        "plain_noop.py",
        "import pite\n\nclass Plain(pite.Node):\n    pass\n",
    );
    let mut backend = Pyo3Backend::new("/root/Plain");
    backend.load(&file, "Plain").unwrap();
    backend.call_ready().unwrap();
    backend.call_process(0.016).unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

const VICTIM: &str = r#"
import pite

class Victim(pite.Node2D):
    def _ready(self):
        self.hp = 3

    def take_damage(self, amount):
        self.hp = self.hp - amount
"#;

const ATTACKER: &str = r#"
import pite

class Attacker(pite.Node2D):
    def _process(self, delta):
        if not getattr(self, "_hit", False):
            self._hit = True
            self.get_node("../Victim").take_damage(1)
"#;

#[test]
fn get_node_direct_call_hits_sibling() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let dir = test_dir("getnode");
    let victim_file = fixture(&dir, "victim.py", VICTIM);
    let attacker_file = fixture(&dir, "attacker.py", ATTACKER);

    let mut tree = NodeTree::new();
    let mut root = NodeDesc::new(NodeId::from("root".to_string()), "Node");
    root.name = "Root".to_string();
    tree.insert(root).unwrap();
    for (id, name) in [("a", "Attacker"), ("v", "Victim")] {
        let mut d = NodeDesc::new(NodeId::from(id.to_string()), "Node2D");
        d.name = name.to_string();
        d.parent = Some(NodeId::from("root".to_string()));
        d.props
            .insert("position".to_string(), pite_core::PropValue::Vec2(0.0, 0.0));
        tree.insert(d).unwrap();
    }
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut victim = Pyo3Backend::new("v");
    victim.set_host(host.clone());
    victim.load(&victim_file, "Victim").unwrap();
    victim.call_ready().unwrap();

    let mut attacker = Pyo3Backend::new("a");
    attacker.set_host(host.clone());
    attacker.load(&attacker_file, "Attacker").unwrap();
    attacker.call_process(0.016).unwrap();

    Python::attach(|py| {
        let inst = host.lookup_instance(py, "v").unwrap();
        let hp: i64 = inst.bind(py).getattr("hp").unwrap().extract().unwrap();
        assert_eq!(hp, 2);
    });
    std::fs::remove_dir_all(&dir).ok();
}

const LABELER: &str = r#"
import pite

class Hud(pite.Label):
    def _ready(self):
        self.text = "HP: 3"

    def _process(self, delta):
        self.text = "HP: 2"
"#;

#[test]
fn label_text_syncs_both_ways() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let dir = test_dir("label");
    let file = fixture(&dir, "hud.py", LABELER);
    let mut tree = NodeTree::new();
    tree.insert(NodeDesc::new(NodeId::from("hud".to_string()), "Label"))
        .unwrap();
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut backend = Pyo3Backend::new("hud");
    backend.set_host(host.clone());
    backend.load(&file, "Hud").unwrap();
    backend.call_ready().unwrap();
    assert_eq!(host.get_string("hud", "text"), "HP: 3");
    backend.call_process(0.016).unwrap();
    assert_eq!(host.get_string("hud", "text"), "HP: 2");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn label_inherits_node2d_position_and_rotation() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let dir = test_dir("label_pos");
    let file = fixture(
        &dir,
        "hud_pos.py",
        r#"
import pite

class HudPos(pite.Label):
    def _ready(self):
        self.position = (100.0, 200.0)
        self.rotation = 1.57
"#,
    );
    let mut tree = NodeTree::new();
    tree.insert(NodeDesc::new(NodeId::from("hud".to_string()), "Label"))
        .unwrap();
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut backend = Pyo3Backend::new("hud");
    backend.set_host(host.clone());
    backend.load(&file, "HudPos").unwrap();
    backend.call_ready().unwrap();
    assert_eq!(
        host.get_vec2("hud", "position"),
        pite_core::Vec2 { x: 100.0, y: 200.0 }
    );
    assert_eq!(host.get_f64("hud", "rotation"), 1.57);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn plain_node_has_no_text_channel() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let dir = test_dir("notetext");
    let file = fixture(
        &dir,
        "plain_notetext.py",
        "import pite\n\nclass Plain(pite.Node):\n    pass\n",
    );
    let mut tree = NodeTree::new();
    tree.insert(NodeDesc::new(NodeId::from("plain".to_string()), "Node"))
        .unwrap();
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut backend = Pyo3Backend::new("plain");
    backend.set_host(host.clone());
    backend.load(&file, "Plain").unwrap();
    backend.call_process(0.016).unwrap();
    Python::attach(|py| {
        use pyo3::types::PyAnyMethods;
        let inst = host.lookup_instance(py, "plain").unwrap();
        assert!(
            !inst.bind(py).hasattr("text").unwrap_or(true),
            "plain Node must not expose a `text` attribute"
        );
    });
    std::fs::remove_dir_all(&dir).ok();
}

const SIZER: &str = r#"
import pite

class Sizer(pite.Button):
    def _ready(self):
        self.size = (120.0, 40.0)

    def _process(self, delta):
        w, h = self.size
        self.size = (w + 1.0, h)
"#;

#[test]
fn button_size_syncs_both_ways() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let dir = test_dir("size");
    let file = fixture(&dir, "sizer.py", SIZER);
    let mut tree = NodeTree::new();
    tree.insert(NodeDesc::new(NodeId::from("sizer".to_string()), "Button"))
        .unwrap();
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut backend = Pyo3Backend::new("sizer");
    backend.set_host(host.clone());
    backend.load(&file, "Sizer").unwrap();
    host.set_prop("sizer", "size", pite_core::PropValue::Vec2(10.0, 20.0));
    backend.call_ready().unwrap();
    let s = host.get_vec2("sizer", "size");
    assert_eq!((s.x, s.y), (120.0, 40.0));
    backend.call_process(0.016).unwrap();
    let s = host.get_vec2("sizer", "size");
    assert_eq!((s.x, s.y), (121.0, 40.0));
    std::fs::remove_dir_all(&dir).ok();
}

const SENSOR: &str = r#"
import pite

class Sensor(pite.Node2D):
    tripped = pite.signal(int, "str")

    def _process(self, delta):
        if pite.pressed.space:
            self.tripped.emit(7, "hi")
"#;

const LISTENER: &str = r#"
import pite

class Listener(pite.Node2D):
    def _ready(self):
        self.get_node("../Sensor").tripped.connect(self.on_tripped)

    def on_tripped(self, n, msg):
        self.position = (float(n), 0.0)
        self.last_msg = msg
"#;

const BAD_DECL: &str = "import pite\n\nclass Bad(pite.Node2D):\n    bad = pite.signal(\"wat\")\n";

#[test]
fn signals_declare_connect_emit_drop() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let dir = test_dir("signals");
    let sensor_file = fixture(&dir, "sensor.py", SENSOR);
    let listener_file = fixture(&dir, "listener.py", LISTENER);
    let bad_file = fixture(&dir, "bad.py", BAD_DECL);

    let mut tree = NodeTree::new();
    let mut root = NodeDesc::new(NodeId::from("root".to_string()), "Node");
    root.name = "Root".to_string();
    tree.insert(root).unwrap();
    for (id, name) in [("s", "Sensor"), ("l", "Listener")] {
        let mut d = NodeDesc::new(NodeId::from(id.to_string()), "Node2D");
        d.name = name.to_string();
        d.parent = Some(NodeId::from("root".to_string()));
        d.props
            .insert("position".to_string(), pite_core::PropValue::Vec2(0.0, 0.0));
        tree.insert(d).unwrap();
    }
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut bad = Pyo3Backend::new("bad");
    bad.set_host(host.clone());
    let err = bad.load(&bad_file, "Bad").unwrap_err();
    assert!(err.to_string().contains("unknown payload type"));

    let mut sensor = Pyo3Backend::new("s");
    sensor.set_host(host.clone());
    sensor.load(&sensor_file, "Sensor").unwrap();
    let mut listener = Pyo3Backend::new("l");
    listener.set_host(host.clone());
    listener.load(&listener_file, "Listener").unwrap();
    listener.call_ready().unwrap();

    crate::input_clear();
    crate::input_begin_frame();
    crate::input_set_key("Space", true);
    sensor.call_process(0.016).unwrap();
    let p = host.get_vec2("l", "position");
    assert_eq!((p.x, p.y), (7.0, 0.0));
    Python::attach(|py| {
        let inst = host.lookup_instance(py, "l").unwrap();
        let msg: String = inst
            .bind(py)
            .getattr("last_msg")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(msg, "hi");
    });

    Python::attach(|py| {
        let wrong = pyo3::types::PyTuple::new(
            py,
            vec![
                "nope".into_pyobject(py).unwrap(),
                "hi".into_pyobject(py).unwrap(),
            ],
        )
        .unwrap();
        let err = host.emit(py, "s", "tripped", &wrong).unwrap_err();
        assert!(err.to_string().contains("wants int"));
        let empty = pyo3::types::PyTuple::empty(py);
        let err = host.emit(py, "s", "missing", &empty).unwrap_err();
        assert!(err.to_string().contains("unknown signal"));
        let err = host.connect("s", "missing", "l", "on_tripped").unwrap_err();
        assert!(err.to_string().contains("unknown signal"));
    });

    host.disconnect_node("l");
    crate::input_clear();
    crate::input_begin_frame();
    crate::input_set_key("Space", true);
    sensor.call_process(0.016).unwrap();
    let p = host.get_vec2("l", "position");
    assert_eq!((p.x, p.y), (7.0, 0.0));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn api_redesign_vec2_input_and_descriptors() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    crate::python::ensure_init();

    pyo3::Python::attach(|py| {
        // 1. Vec2 math and operations
        let v = crate::vec2::Vec2::new(10.0, 20.0);
        assert_eq!(v.x(), 10.0);
        assert_eq!(v.y(), 20.0);

        let v_obj = pyo3::Bound::new(py, v).unwrap();
        let sum: crate::vec2::Vec2 = v_obj
            .call_method1("__add__", ((5.0, 5.0),))
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(sum.x(), 15.0);
        assert_eq!(sum.y(), 25.0);

        // Unpacking x, y = vec
        let iter = v_obj.try_iter().unwrap();
        let items: Vec<f64> = iter
            .map(|item| item.unwrap().extract::<f64>().unwrap())
            .collect();
        assert_eq!(items, vec![10.0, 20.0]);

        // 2. Input queries (held, pressed, released)
        crate::input_begin_frame();
        crate::input_set_key("Space", true);

        let held_query = pyo3::Bound::new(
            py,
            PyInputQuery {
                kind: InputQueryKind::Held,
            },
        )
        .unwrap();
        let is_space_held: bool = held_query.getattr("space").unwrap().extract().unwrap();
        assert!(is_space_held);

        // Calling pite.held("Space") raises TypeError
        let err = held_query.call1(("Space",)).unwrap_err();
        assert!(err.is_instance_of::<pyo3::exceptions::PyTypeError>(py));
        assert!(err.to_string().contains("attribute-based accessor"));

        let pressed_query = pyo3::Bound::new(
            py,
            PyInputQuery {
                kind: InputQueryKind::Pressed,
            },
        )
        .unwrap();
        let is_space_pressed: bool = pressed_query.getattr("space").unwrap().extract().unwrap();
        assert!(is_space_pressed);
    });
}

#[test]
fn missing_node_returns_default_not_panic() {
    // A scripted node can be removed from the tree while its slot still
    // holds a Python instance (the runtime keeps slots until rebuild).
    // Reading its props must return the type default, not panic — that
    // is the contract `dropped_button_disconnects_silently` in
    // pite-runtime exercises end-to-end.
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let mut tree = NodeTree::new();
    let mut root = NodeDesc::new(NodeId::from("root".to_string()), "Node2D");
    root.name = "Root".to_string();
    tree.insert(root).unwrap();
    let host = Arc::new(crate::ScriptHost::new(tree));

    // Both getters must return the zero default with the node absent.
    assert_eq!(host.get_f64("ghost", "wait_time"), 0.0);
    assert!(!host.get_bool("ghost", "disabled"));
    assert_eq!(host.get_i64("ghost", "collision_layer"), 0);
    assert_eq!(host.get_string("ghost", "text"), "");
    let v = host.get_vec2("ghost", "position");
    assert_eq!((v.x, v.y), (0.0, 0.0));

    // Writing to a missing node is a silent no-op (already true before
    // commit 4; the schema-defaults work must not change it).
    host.set_prop("ghost", "position", pite_core::PropValue::Vec2(9.0, 9.0));
}

/// `load_base` constructs the engine's own pyclass for a node with no
/// user script attached. The instance must be registered with the host,
/// its `node_path` set, and its lifecycle flags probed — a base
/// `Sprite2D` has none of `_ready` / `_process` / `_physics_process`,
/// so a future runtime can skip calling into it entirely.
#[test]
fn load_base_instantiates_pyclass_without_script() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let mut tree = NodeTree::new();
    tree.insert(NodeDesc::new(NodeId::from("hero".to_string()), "Sprite2D"))
        .unwrap();
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut backend = Pyo3Backend::new("hero");
    backend.set_host(host.clone());
    backend.load_base("Sprite2D").unwrap();

    // Registered with the host so signal connections and get_node
    // work through the base pyclass.
    Python::attach(|py| {
        assert!(host.lookup_instance(py, "hero").is_some());
        let inst = host.lookup_instance(py, "hero").unwrap();
        let node_path: String = inst
            .bind(py)
            .getattr("node_path")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(node_path, "hero");
    });

    // Base pyclass has no lifecycle methods; all three flags false.
    assert!(!backend.has_ready());
    assert!(!backend.has_process());
    assert!(!backend.has_physics_process());

    // `call_*` on a base instance is a silent no-op (no method).
    backend.call_ready().unwrap();
    backend.call_process(0.016).unwrap();
    backend.call_physics_process(0.016).unwrap();

    // Writing through the base pyclass reaches the tree.
    host.set_prop("hero", "text", pite_core::PropValue::Str("ignored".into()));
    assert_eq!(host.get_string("hero", "text"), "ignored");

    std::fs::remove_dir_all(
        std::env::temp_dir().join(format!("pite-py-loadbase-{}", std::process::id())),
    )
    .ok();
}

#[test]
fn load_base_animated_sprite2d() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    use pite_core::{NodeDesc, NodeId, NodeTree};
    use std::sync::Arc;

    let mut tree = NodeTree::new();
    tree.insert(NodeDesc::new(
        NodeId::from("anim".to_string()),
        "AnimatedSprite2D",
    ))
    .unwrap();
    let host = Arc::new(crate::ScriptHost::new(tree));
    crate::set_current_host(host.clone());

    let mut backend = Pyo3Backend::new("anim");
    backend.set_host(host.clone());
    backend.load_base("AnimatedSprite2D").unwrap();

    Python::attach(|py| {
        let inst = host.lookup_instance(py, "anim").unwrap();
        let b = inst.bind(py);

        // Initially not playing
        let playing: bool = b.call_method0("is_playing").unwrap().extract().unwrap();
        assert!(!playing);

        // Calling play("run")
        b.call_method1("play", ("run",)).unwrap();
        assert_eq!(host.get_string("anim", "animation"), "run");
        assert!(host.get_bool("anim", "playing"));

        // Calling pause()
        b.call_method0("pause").unwrap();
        assert!(!host.get_bool("anim", "playing"));

        // Calling play() resumes
        b.call_method0("play").unwrap();
        assert!(host.get_bool("anim", "playing"));

        // Calling stop() resets frame to 0 and stops playing
        host.set_prop("anim", "frame", pite_core::PropValue::Int(3));
        b.call_method0("stop").unwrap();
        assert!(!host.get_bool("anim", "playing"));
        assert_eq!(host.get_i64("anim", "frame"), 0);
    });
}

#[test]
fn tilemap2d_python_api_methods() {
    let _guard = crate::host::HOST_SERIAL.lock().unwrap();
    crate::python::ensure_init();

    pyo3::Python::attach(|py| {
        let host = std::sync::Arc::new(crate::host::ScriptHost::new(pite_core::NodeTree::new()));
        crate::host::set_current_host(host.clone());

        let mut doc = pite_scene::tilemap::TileMapDoc::new("res://tiles.ptatlas", 16, 16);
        let layer = pite_scene::tilemap::TileMapLayer::new("ground");
        doc.layers.push(layer);
        doc.set_cell(0, 1, 2, "grass");
        host.set_tilemap_doc("Root/Map", doc);

        let pite_mod = py.import("pite").unwrap();
        let tilemap_cls = pite_mod.getattr("TileMap2D").unwrap();
        let node = tilemap_cls.call0().unwrap();
        node.setattr("node_path", "Root/Map").unwrap();

        // tile_size
        let ts: (u32, u32) = node.getattr("tile_size").unwrap().extract().unwrap();
        assert_eq!(ts, (16, 16));

        // layers
        let layers: Vec<String> = node.getattr("layers").unwrap().extract().unwrap();
        assert_eq!(layers, vec!["ground"]);

        // get_cell
        let cell: Option<String> = node
            .call_method1("get_cell", (1, 2))
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(cell, Some("grass".to_string()));

        // set_cell
        node.call_method1("set_cell", (3, 4, "stone")).unwrap();
        let cell2: Option<String> = node
            .call_method1("get_cell", (3, 4))
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(cell2, Some("stone".to_string()));

        // get_used_cells
        let used: Vec<(i32, i32)> = node
            .call_method1("get_used_cells", (0,))
            .unwrap()
            .extract()
            .unwrap();
        assert!(used.contains(&(1, 2)));
        assert!(used.contains(&(3, 4)));

        // erase_cell
        let erased: bool = node
            .call_method1("erase_cell", (1, 2))
            .unwrap()
            .extract()
            .unwrap();
        assert!(erased);
        let cell_erased: Option<String> = node
            .call_method1("get_cell", (1, 2))
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(cell_erased, None);

        // world_to_map / map_to_world
        let coord: (i32, i32) = node
            .call_method1("world_to_map", (32.0, 48.0))
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(coord, (2, 3));
        let wpos: pyo3::Bound<'_, pyo3::PyAny> = node.call_method1("map_to_world", (2, 3)).unwrap();
        let wx: f32 = wpos.getattr("x").unwrap().extract().unwrap();
        let wy: f32 = wpos.getattr("y").unwrap().extract().unwrap();
        assert_eq!((wx, wy), (40.0, 56.0));

        // clear
        node.call_method0("clear").unwrap();
        let used_after: Vec<(i32, i32)> = node
            .call_method1("get_used_cells", (0,))
            .unwrap()
            .extract()
            .unwrap();
        assert!(used_after.is_empty());
    });
}

#[test]
fn noop_pass_process_is_skipped_and_inheritance_works() {
    let dir = std::env::temp_dir().join(format!("pite-noop-skip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("inherit.py");
    std::fs::write(
        &script,
        r#"
import pite

class BasePass(pite.Node2D):
    def _process(self, delta: float) -> None:
        pass

class ChildPass(BasePass):
    pass

class ChildOverride(BasePass):
    def _process(self, delta: float) -> None:
        self.position.x += delta

class BaseActive(pite.Node2D):
    def _process(self, delta: float) -> None:
        self.position.x += delta

class ChildInheritActive(BaseActive):
    pass
"#,
    )
    .unwrap();

    let mut base_pass = Pyo3Backend::new("a");
    base_pass
        .load(script.to_str().unwrap(), "BasePass")
        .unwrap();
    assert!(!base_pass.has_process());

    let mut child_pass = Pyo3Backend::new("b");
    child_pass
        .load(script.to_str().unwrap(), "ChildPass")
        .unwrap();
    assert!(!child_pass.has_process());

    let mut child_override = Pyo3Backend::new("c");
    child_override
        .load(script.to_str().unwrap(), "ChildOverride")
        .unwrap();
    assert!(child_override.has_process());

    let mut child_inherit_active = Pyo3Backend::new("d");
    child_inherit_active
        .load(script.to_str().unwrap(), "ChildInheritActive")
        .unwrap();
    assert!(child_inherit_active.has_process());

    let _ = std::fs::remove_dir_all(&dir);
}
