// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use pite_script::pyo3::{self, types::PyAnyMethods};

type RectDraw = ((f64, f64), (f64, f64), [u8; 4], i32);
type LineDraw = ((f64, f64), (f64, f64), [u8; 4], f64, i32);
type TextDraw = (String, (f64, f64), f32, [u8; 4], i32);

#[derive(Default)]
struct RecordingRenderer {
    rects: Vec<RectDraw>,
    lines: Vec<LineDraw>,
    texts: Vec<TextDraw>,
}

impl Renderer2D for RecordingRenderer {
    fn begin_frame(&mut self) -> Result<()> {
        Ok(())
    }
    fn draw_sprite(&mut self, _texture: &str, _pos: (f64, f64), _rotation: f64) -> Result<()> {
        Ok(())
    }
    fn draw_rect(&mut self, pos: (f64, f64), size: (f64, f64), color: [u8; 4]) -> Result<()> {
        self.rects.push((pos, size, color, 0));
        Ok(())
    }
    fn draw_rect_layered(
        &mut self,
        pos: (f64, f64),
        size: (f64, f64),
        color: [u8; 4],
        z_index: i32,
    ) -> Result<()> {
        self.rects.push((pos, size, color, z_index));
        Ok(())
    }
    fn draw_line_layered(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        color: [u8; 4],
        thickness: f64,
        z_index: i32,
    ) -> Result<()> {
        self.lines.push((from, to, color, thickness, z_index));
        Ok(())
    }
    fn draw_text(&mut self, text: &str, pos: (f64, f64), size: f32, color: [u8; 4]) -> Result<()> {
        self.texts.push((text.to_string(), pos, size, color, 0));
        Ok(())
    }
    fn draw_text_layered(
        &mut self,
        text: &str,
        pos: (f64, f64),
        size: f32,
        color: [u8; 4],
        z_index: i32,
    ) -> Result<()> {
        self.texts
            .push((text.to_string(), pos, size, color, z_index));
        Ok(())
    }
    fn end_frame(&mut self) -> Result<()> {
        Ok(())
    }
}

#[test]
fn plugin_autoload_ticks_after_nodes_and_exposes_lookup() {
    let _g = SERIAL.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("pite-plug-rt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("plugins/my_plugin")).unwrap();

    std::fs::write(
        dir.join("pite.toml"),
        "[project]\nname = \"t\"\n\n[plugins]\nmy_plugin = { enabled = true }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("plugins/my_plugin/plugin.toml"),
        r#"
id = "my_plugin"
name = "My Plugin"
version = "0.1.0"
pite_version = ">=0.4.0"
runtime = "my_plugin.rt:Runtime"

[autoloads]
Tracker = "my_plugin.rt:Tracker"
"#,
    )
    .unwrap();
    std::fs::write(dir.join("plugins/my_plugin/__init__.py"), "").unwrap();
    std::fs::write(
        dir.join("plugins/my_plugin/rt.py"),
        r#"
import pite

class Runtime:
    def on_ready(self):
        self.tag = "ok"

class Tracker:
    def on_ready(self):
        self.log = []
        self.scenes = []

    def on_scene_loaded(self, path):
        self.scenes.append(path)

    def _process(self, dt):
        self.log.append("autoload_process")
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("scripts/node_a.py"),
        r#"
import pite

class NodeA(pite.Node):
    def _process(self, dt):
        tracker = pite.autoload("Tracker")
        rt = pite.plugin("my_plugin")
        tracker.log.append(f"node_process:{rt.tag}")
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/main.ptscn"),
        r#"
format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node"
name = "Root"

[node.script]
path = "res://scripts/node_a.py"
class = "NodeA"
"#,
    )
    .unwrap();

    let mut session = GameSession::open(&dir.join("scenes/main.ptscn"), true).unwrap();
    session.update(0.016);

    let (log, scenes_len): (Vec<String>, usize) = pyo3::Python::attach(|py| {
        let tracker = session
            .plugin_host
            .get_autoload(py, "Tracker")
            .expect("Tracker autoload loaded");
        let log: Vec<String> = tracker.bind(py).getattr("log").unwrap().extract().unwrap();
        let scenes: Vec<String> = tracker
            .bind(py)
            .getattr("scenes")
            .unwrap()
            .extract()
            .unwrap();
        (log, scenes.len())
    });

    assert_eq!(scenes_len, 1);
    assert_eq!(log, vec!["node_process:ok", "autoload_process"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plugin_node_autobinds_default_class_and_live_props() {
    let _g = SERIAL.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("pite-plug-node-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("plugins/ui_plug")).unwrap();

    std::fs::write(
        dir.join("pite.toml"),
        "[project]\nname = \"t\"\n\n[plugins]\nui_plug = { enabled = true }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("plugins/ui_plug/plugin.toml"),
        r#"
id = "ui_plug"
name = "UI Plugin"
version = "0.1.0"
pite_version = ">=0.4.0"

[[nodes]]
type_name = "HealthBar"
inherits = "Node2D"
script = "ui_plug.hb:HealthBar"

[[nodes.props]]
key = "max_hp"
type = "int"
default = 100
"#,
    )
    .unwrap();
    std::fs::write(dir.join("plugins/ui_plug/__init__.py"), "").unwrap();
    std::fs::write(
        dir.join("plugins/ui_plug/hb.py"),
        r#"
import pite

class HealthBar(pite.Node2D):
    def _ready(self):
        assert self.max_hp == 100
        self.max_hp = 250
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("scripts/reader.py"),
        r#"
import pite

class Reader(pite.Node):
    def _ready(self):
        bar = self.get_node("../Bar")
        assert bar.max_hp == 250
        bar.max_hp = 300
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/main.ptscn"),
        r#"
format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Root"

[[node]]
id = "bar"
type = "HealthBar"
name = "Bar"
parent = "root"

[[node]]
id = "reader"
type = "Node"
name = "Reader"
parent = "root"

[node.script]
path = "res://scripts/reader.py"
class = "Reader"
"#,
    )
    .unwrap();

    let session = GameSession::open(&dir.join("scenes/main.ptscn"), true).unwrap();
    let hp = session.host().with_tree(|tree| {
        tree.get(&pite_core::NodeId::from("bar"))
            .and_then(|n| n.props.get("max_hp").cloned())
    });
    assert_eq!(hp, Some(pite_core::PropValue::Int(300)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn per_node_draw_emits_local_and_world_commands() {
    let _g = SERIAL.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("pite-plug-draw-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();

    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scripts/gizmo.py"),
        r##"
import pite

class Gizmo2D(pite.Node2D):
    def _draw(self, canvas):
        canvas.draw_rect((5, 6), (20, 10), "#ff0000")
        canvas.draw_line((0, 0), (10, 0), "#00ff00", width=2.0)

class HudNode(pite.Node):
    def _draw(self, canvas):
        canvas.draw_text("hi", (1, 2))
"##,
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes/main.ptscn"),
        r#"
format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Root"

[[node]]
id = "gizmo"
type = "Node2D"
name = "Gizmo"
parent = "root"
[node.props]
position = [100.0, 50.0]
z_index = 3
[node.script]
path = "res://scripts/gizmo.py"
class = "Gizmo2D"

[[node]]
id = "hud"
type = "Node"
name = "Hud"
parent = "root"
[node.script]
path = "res://scripts/gizmo.py"
class = "HudNode"
"#,
    )
    .unwrap();

    let mut session = GameSession::open(&dir.join("scenes/main.ptscn"), true).unwrap();
    let mut rec = RecordingRenderer::default();
    session.draw_into(&mut rec).unwrap();

    assert_eq!(
        rec.rects,
        vec![((105.0, 56.0), (20.0, 10.0), [255, 0, 0, 255], 3)]
    );
    assert_eq!(
        rec.lines,
        vec![((100.0, 50.0), (110.0, 50.0), [0, 255, 0, 255], 2.0, 3)]
    );
    assert_eq!(
        rec.texts,
        vec![("hi".to_string(), (1.0, 2.0), 16.0, [255, 255, 255, 255], 0)]
    );
    let _ = std::fs::remove_dir_all(&dir);
}
