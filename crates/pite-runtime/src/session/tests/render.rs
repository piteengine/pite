// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

fn dogfood_scene() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples")
        .join("minimal-2d")
        .join("scenes")
        .join("main.ptscn")
}

#[derive(Default)]
struct RecordingRenderer {
    calls: Vec<(String, Option<String>, (f64, f64))>,
    layered_calls: Vec<(String, Option<String>, (f64, f64), i32)>,
    tinted_calls: Vec<(String, Option<String>, (f64, f64), i32, [u8; 4])>,
    uv_calls: Vec<(String, [f32; 4], (f32, f32), (f64, f64), i32)>,
}

impl Renderer2D for RecordingRenderer {
    fn begin_frame(&mut self) -> Result<()> {
        Ok(())
    }

    fn draw_sprite(&mut self, texture: &str, x: f64, y: f64, _rotation: f64) -> Result<()> {
        self.calls.push((texture.to_string(), None, (x, y)));
        self.layered_calls
            .push((texture.to_string(), None, (x, y), 0));
        self.tinted_calls
            .push((texture.to_string(), None, (x, y), 0, [255, 255, 255, 255]));
        Ok(())
    }

    fn draw_sprite_frame(
        &mut self,
        texture: &str,
        frame: Option<&str>,
        x: f64,
        y: f64,
        _rotation: f64,
    ) -> Result<()> {
        self.calls
            .push((texture.to_string(), frame.map(str::to_string), (x, y)));
        self.layered_calls
            .push((texture.to_string(), frame.map(str::to_string), (x, y), 0));
        self.tinted_calls.push((
            texture.to_string(),
            frame.map(str::to_string),
            (x, y),
            0,
            [255, 255, 255, 255],
        ));
        Ok(())
    }

    fn draw_sprite_layered(
        &mut self,
        texture: &str,
        frame: Option<&str>,
        x: f64,
        y: f64,
        _rotation: f64,
        z_index: i32,
    ) -> Result<()> {
        self.calls
            .push((texture.to_string(), frame.map(str::to_string), (x, y)));
        self.layered_calls.push((
            texture.to_string(),
            frame.map(str::to_string),
            (x, y),
            z_index,
        ));
        self.tinted_calls.push((
            texture.to_string(),
            frame.map(str::to_string),
            (x, y),
            z_index,
            [255, 255, 255, 255],
        ));
        Ok(())
    }

    fn draw_sprite_tinted(
        &mut self,
        texture: &str,
        frame: Option<&str>,
        x: f64,
        y: f64,
        _rotation: f64,
        z_index: i32,
        color: [u8; 4],
    ) -> Result<()> {
        self.calls
            .push((texture.to_string(), frame.map(str::to_string), (x, y)));
        self.layered_calls.push((
            texture.to_string(),
            frame.map(str::to_string),
            (x, y),
            z_index,
        ));
        self.tinted_calls.push((
            texture.to_string(),
            frame.map(str::to_string),
            (x, y),
            z_index,
            color,
        ));
        Ok(())
    }

    fn draw_sprite_uv(
        &mut self,
        texture: &str,
        uv: [f32; 4],
        size_px: (f32, f32),
        x: f64,
        y: f64,
        rotation: f64,
        z_index: i32,
    ) -> Result<()> {
        self.uv_calls
            .push((texture.to_string(), uv, size_px, (x, y), z_index));
        self.draw_sprite_uv_tinted(
            texture,
            uv,
            size_px,
            x,
            y,
            rotation,
            z_index,
            [255, 255, 255, 255],
        )
    }

    fn draw_sprite_uv_tinted(
        &mut self,
        texture: &str,
        _uv: [f32; 4],
        _size_px: (f32, f32),
        x: f64,
        y: f64,
        _rotation: f64,
        z_index: i32,
        color: [u8; 4],
    ) -> Result<()> {
        self.calls.push((texture.to_string(), None, (x, y)));
        self.layered_calls
            .push((texture.to_string(), None, (x, y), z_index));
        self.tinted_calls
            .push((texture.to_string(), None, (x, y), z_index, color));
        Ok(())
    }

    fn draw_text(
        &mut self,
        _text: &str,
        _x: f64,
        _y: f64,
        _size: f32,
        _color: [u8; 4],
    ) -> Result<()> {
        Ok(())
    }

    fn draw_rect(&mut self, _x: f64, _y: f64, _w: f64, _h: f64, _color: [u8; 4]) -> Result<()> {
        Ok(())
    }

    fn end_frame(&mut self) -> Result<()> {
        Ok(())
    }
}

#[test]
fn dogfood_sprites_draw_from_one_sheet() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut session = GameSession::open(&dogfood_scene(), true).unwrap();
    let mut recorder = RecordingRenderer::default();
    session.draw_into(&mut recorder).unwrap();

    let sheet = session
        .resolve_path("res://assets/sheet.ptatlas")
        .to_string_lossy()
        .into_owned();
    let sheet = PathBuf::from(&sheet)
        .with_file_name("sheet.png")
        .to_string_lossy()
        .into_owned();
    let frames: Vec<Option<&str>> = recorder
        .calls
        .iter()
        .filter(|(tex, _, _)| tex == &sheet)
        .map(|(_, frame, _)| frame.as_deref())
        .collect();
    assert!(
        frames.contains(&Some("player")),
        "player must draw from the sheet, got {:?}",
        recorder.calls
    );
    assert!(
        frames.contains(&Some("enemy")),
        "instanced enemy must draw from the sheet, got {:?}",
        recorder.calls
    );
    let sheets: std::collections::HashSet<&String> =
        recorder.calls.iter().map(|(tex, _, _)| tex).collect();
    assert_eq!(
        sheets.len(),
        1,
        "all frames share one texture, so they batch into one draw"
    );
}

#[test]
fn texture_and_atlas_together_fail_loudly() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-atlas-draw-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"both\"\ntype = \"Sprite2D\"\nname = \"Both\"\nparent = \"root\"\n\n[node.props]\ntexture = \"res://a.png\"\natlas = \"res://sheet.ptatlas\"\nframe = \"player\"\n",
    )
    .unwrap();
    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    let mut recorder = RecordingRenderer::default();
    let err = session
        .draw_into(&mut recorder)
        .expect_err("both texture and atlas must fail loudly");
    assert!(
        err.to_string().contains("mutually exclusive"),
        "unexpected: {err:#}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn plain_texture_sprite_draws_without_mutual_exclusion_error() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-plain-texture-draw-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"sprite\"\ntype = \"Sprite2D\"\nname = \"Sprite\"\nparent = \"root\"\n\n[node.props]\ntexture = \"res://assets/player.png\"\n",
    )
    .unwrap();
    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    let mut recorder = RecordingRenderer::default();
    session
        .draw_into(&mut recorder)
        .expect("plain texture sprite must draw cleanly without atlas collision");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sprite_z_index_passes_to_renderer() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-z-index-draw-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "bg"
type = "Sprite2D"
name = "Background"
parent = "root"

[node.props]
texture = "res://assets/bg.png"
z_index = -10

[[node]]
id = "fg"
type = "Sprite2D"
name = "Foreground"
parent = "root"

[node.props]
texture = "res://assets/fg.png"
z_index = 25
"#,
    )
    .unwrap();
    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    let mut recorder = RecordingRenderer::default();
    session.draw_into(&mut recorder).expect("scene draws");

    assert_eq!(recorder.layered_calls.len(), 2);
    assert_eq!(recorder.layered_calls[0].3, -10);
    assert_eq!(recorder.layered_calls[1].3, 25);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sprite_modulate_passes_to_renderer() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-modulate-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(
        dir.join("pite.toml"),
        r#"[project]
name = "modulate-test"
version = "0.1.0"
main_scene = "res://scenes/main.ptscn"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        r##"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "bg"
type = "Sprite2D"
name = "Background"
parent = "root"

[node.props]
texture = "res://assets/bg.png"
modulate = "#ff000080"
"##,
    )
    .unwrap();
    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    let mut recorder = RecordingRenderer::default();
    session.draw_into(&mut recorder).expect("scene draws");

    assert_eq!(recorder.tinted_calls.len(), 1);
    assert_eq!(recorder.tinted_calls[0].4, [255, 0, 0, 128]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn tilemap_chunk_caching_and_rendering() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-tilemap-chunk-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::create_dir_all(dir.join("maps")).unwrap();

    std::fs::write(
        dir.join("pite.toml"),
        r#"[project]
name = "tilemap-test"
version = "0.1.0"
main_scene = "res://scenes/main.ptscn"
"#,
    )
    .unwrap();

    // 1x1 dummy png
    let png_bytes = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    std::fs::write(dir.join("assets").join("sheet.png"), png_bytes).unwrap();

    let atlas_toml = r#"format_version = 1
texture = "res://assets/sheet.png"
size = [64, 16]

[frames.grass]
x = 0
y = 0
w = 16
h = 16

[frames.stone]
x = 16
y = 0
w = 16
h = 16

[frames.dirt]
x = 32
y = 0
w = 16
h = 16

[frames.flower]
x = 48
y = 0
w = 16
h = 16
"#;
    std::fs::write(dir.join("assets").join("sheet.ptatlas"), atlas_toml).unwrap();

    let mut doc = pite_scene::TileMapDoc::new("res://assets/sheet.ptatlas", 16, 16);
    let mut ground = pite_scene::TileMapLayer::new("Ground");
    ground
        .tiles
        .push(pite_scene::TileCell(0, 0, "grass".to_string()));
    ground
        .tiles
        .push(pite_scene::TileCell(15, 15, "grass".to_string()));
    // Far-away tile: chunk (31, 31) -> culled by camera
    ground
        .tiles
        .push(pite_scene::TileCell(500, 500, "stone".to_string()));
    doc.layers.push(ground);

    let mut overlay = pite_scene::TileMapLayer::new("Overlay");
    overlay
        .tiles
        .push(pite_scene::TileCell(1, 1, "flower".to_string()));
    doc.layers.push(overlay);

    pite_scene::save_tilemap(&dir.join("maps").join("level.ptmap"), &doc).unwrap();

    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "map"
type = "TileMap2D"
name = "LevelMap"
parent = "root"

[node.props]
tilemap = "res://maps/level.ptmap"
z_index = 10
"#,
    )
    .unwrap();

    let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
    let mut recorder = RecordingRenderer::default();
    session.draw_into(&mut recorder).expect("tilemap renders");

    // Ground chunk (0, 0) has 2 visible tiles, Overlay chunk (0, 0) has 1 visible tile.
    // Tile (500, 500) is culled.
    assert_eq!(recorder.uv_calls.len(), 3);
    assert_eq!(recorder.uv_calls[0].4, 10); // Layer 0 z_index
    assert_eq!(recorder.uv_calls[1].4, 10); // Layer 0 z_index
    assert_eq!(recorder.uv_calls[2].4, 11); // Layer 1 z_index (10 + 1)
                                            // Verify UV rects are pre-resolved from the atlas
    assert_ne!(recorder.uv_calls[0].1, [0.0, 0.0, 0.0, 0.0]);

    // Now test dirty chunk dynamic modification: add "dirt" at (2, 2)
    session.host().set_tilemap_cell("map", 0, 2, 2, "dirt");
    let mut recorder2 = RecordingRenderer::default();
    session
        .draw_into(&mut recorder2)
        .expect("tilemap re-renders with dirty chunk");
    assert_eq!(recorder2.uv_calls.len(), 4);

    // Erase cell at (2, 2)
    assert!(session.host().erase_tilemap_cell("map", 0, 2, 2));
    let mut recorder3 = RecordingRenderer::default();
    session
        .draw_into(&mut recorder3)
        .expect("tilemap re-renders after erasing cell");
    assert_eq!(recorder3.uv_calls.len(), 3);

    std::fs::remove_dir_all(&dir).ok();
}
