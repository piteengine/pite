// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

#[test]
fn physics_character_and_raycast_e2e() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let dir = std::env::temp_dir().join("pite-physics-e2e");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(
        dir.join("pite.toml"),
        "[project]\nname = \"physics-test\"\n",
    )
    .unwrap();

    let scene = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Root"

[[node]]
id = "floor"
type = "StaticBody2D"
name = "Floor"
parent = "root"

[node.props]
position = [0.0, 100.0]

[[node]]
id = "floor_shape"
type = "CollisionShape2D"
name = "Shape"
parent = "floor"

[node.props]
shape = "box"
size = [200.0, 20.0]

[[node]]
id = "player"
type = "CharacterBody2D"
name = "Player"
parent = "root"

[node.props]
position = [0.0, 0.0]

[node.script]
path = "res://scripts/player.py"
class = "Player"

[[node]]
id = "player_shape"
type = "CollisionShape2D"
name = "Shape"
parent = "player"

[node.props]
shape = "box"
size = [20.0, 20.0]
"#;
    std::fs::write(dir.join("scenes").join("main.ptscn"), scene).unwrap();

    let script = r#"import pite

class Player(pite.CharacterBody2D):
    def _physics_process(self, dt):
        self.velocity = (0.0, 300.0)
        self.move_and_slide()
        hit = pite.raycast((0.0, 0.0), (0.0, 150.0))
        if hit is not None and hit.get("body") == "floor":
            pass
"#;
    std::fs::write(dir.join("scripts").join("player.py"), script).unwrap();

    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    for _ in 0..30 {
        session.update(0.016);
    }

    let player_y = session
        .host
        .with_tree(|t| match t.get(&NodeId::from("player".to_string())) {
            Some(n) => match n.props.get("position") {
                Some(PropValue::Vec2(_, y)) => *y,
                _ => f64::NAN,
            },
            None => f64::NAN,
        });

    assert!(
        (player_y - 80.0).abs() < 1.0,
        "Player should rest on floor at y=80, got {player_y}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn physics_area2d_signal_e2e() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let dir = std::env::temp_dir().join("pite-physics-area-e2e");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"area-test\"\n").unwrap();

    let scene = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Root"

[[node]]
id = "zone"
type = "Area2D"
name = "Zone"
parent = "root"

[node.props]
position = [0.0, 60.0]

[node.script]
path = "res://scripts/zone.py"
class = "Zone"

[[node]]
id = "zone_shape"
type = "CollisionShape2D"
name = "Shape"
parent = "zone"

[node.props]
shape = "box"
size = [60.0, 60.0]

[[node]]
id = "player"
type = "CharacterBody2D"
name = "Player"
parent = "root"

[node.props]
position = [0.0, 0.0]

[node.script]
path = "res://scripts/player.py"
class = "Player"

[[node]]
id = "player_shape"
type = "CollisionShape2D"
name = "Shape"
parent = "player"

[node.props]
shape = "box"
size = [20.0, 20.0]
"#;
    std::fs::write(dir.join("scenes").join("main.ptscn"), scene).unwrap();

    let player_script = r#"import pite

class Player(pite.CharacterBody2D):
    def _physics_process(self, dt):
        self.velocity = (0.0, 200.0)
        self.move_and_slide()
"#;
    std::fs::write(dir.join("scripts").join("player.py"), player_script).unwrap();

    let zone_script = r#"import pite

class Zone(pite.Area2D):
    def _ready(self):
        self.body_entered.connect(self.on_body)

    def on_body(self, body_id):
        self.position = (999.0, 999.0)
"#;
    std::fs::write(dir.join("scripts").join("zone.py"), zone_script).unwrap();

    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    for _ in 0..30 {
        session.update(0.016);
        let zp = session
            .host
            .with_tree(|t| match t.get(&NodeId::from("zone".to_string())) {
                Some(n) => match n.props.get("position") {
                    Some(PropValue::Vec2(x, y)) => (*x, *y),
                    _ => (0.0, 0.0),
                },
                None => (0.0, 0.0),
            });
        if zp == (999.0, 999.0) {
            break;
        }
    }

    let zone_pos = session
        .host
        .with_tree(|t| match t.get(&NodeId::from("zone".to_string())) {
            Some(n) => match n.props.get("position") {
                Some(PropValue::Vec2(x, y)) => (*x, *y),
                _ => (0.0, 0.0),
            },
            None => (0.0, 0.0),
        });

    assert_eq!(
        zone_pos,
        (999.0, 999.0),
        "Area2D body_entered signal should move zone to (999, 999)"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn tilemap_loading_and_physics_e2e() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let dir = std::env::temp_dir().join(format!("pite-tilemap-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("maps")).unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(
        dir.join("pite.toml"),
        "[project]\nname = \"tilemap-test\"\n",
    )
    .unwrap();

    let atlas = r#"{
"texture": "res://assets/tiles.png",
"size": [64, 64],
"frames": {
"grass": { "x": 0, "y": 0, "w": 32, "h": 32 },
"wall": { "x": 32, "y": 0, "w": 32, "h": 32 }
}
}"#;
    std::fs::write(dir.join("assets").join("tiles.atlas.json"), atlas).unwrap();

    let ptmap = r#"atlas = "res://assets/tiles.atlas.json"
tile_size = [32, 32]

[[layer]]
name = "ground"
visible = true
collision_enabled = true
collision_layer = 1
collision_mask = 1
tiles = [
[-1, 0, "wall"],
[0, 0, "wall"],
[1, 0, "wall"],
]
"#;
    std::fs::write(dir.join("maps").join("level.ptmap"), ptmap).unwrap();

    let scene = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Root"

[[node]]
id = "tilemap"
type = "TileMap2D"
name = "Map"
parent = "root"

[node.props]
tilemap = "res://maps/level.ptmap"
position = [0.0, 100.0]

[node.script]
path = "res://scripts/map.py"
class = "MapScript"

[[node]]
id = "player"
type = "CharacterBody2D"
name = "Player"
parent = "root"

[node.props]
position = [0.0, 0.0]

[node.script]
path = "res://scripts/player.py"
class = "Player"

[[node]]
id = "player_shape"
type = "CollisionShape2D"
name = "Shape"
parent = "player"

[node.props]
shape = "box"
size = [20.0, 20.0]
"#;
    std::fs::write(dir.join("scenes").join("main.ptscn"), scene).unwrap();

    let player_script = r#"import pite

class Player(pite.CharacterBody2D):
    def _physics_process(self, dt):
        self.velocity = (0.0, 200.0)
        self.move_and_slide()
"#;
    std::fs::write(dir.join("scripts").join("player.py"), player_script).unwrap();

    let map_script = r#"import pite

class MapScript(pite.TileMap2D):
    def _ready(self):
        val = self.get_cell(0, 0)
        assert val == "wall", f"expected wall, got {val}"

        layers = self.layers
        assert layers == ["ground"], f"got layers {layers}"

        self.set_cell(2, 0, "grass")
        assert self.get_cell(2, 0) == "grass"

        col, row = self.world_to_map((0.0, 100.0))
        assert (col, row) == (0, 0), f"got ({col}, {row})"
        world = self.map_to_world(0, 0)
        assert world.x == 16.0 and world.y == 116.0, f"got {world.x}, {world.y}"
"#;
    std::fs::write(dir.join("scripts").join("map.py"), map_script).unwrap();

    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    for _ in 0..40 {
        session.update(0.016);
    }

    let player_y = session
        .host
        .with_tree(|t| match t.get(&NodeId::from("player".to_string())) {
            Some(n) => match n.props.get("position") {
                Some(PropValue::Vec2(_, y)) => *y,
                _ => f64::NAN,
            },
            None => f64::NAN,
        });

    assert!(
        (player_y - 90.0).abs() < 2.0,
        "Player should rest on tilemap floor at y=90, got {player_y}"
    );

    std::fs::remove_dir_all(&dir).ok();
}
