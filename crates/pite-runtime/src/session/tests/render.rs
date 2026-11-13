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
}

impl Renderer2D for RecordingRenderer {
    fn begin_frame(&mut self) -> Result<()> {
        Ok(())
    }

    fn draw_sprite(&mut self, texture: &str, x: f64, y: f64, _rotation: f64) -> Result<()> {
        self.calls.push((texture.to_string(), None, (x, y)));
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
        .resolve_path("res://assets/sheet.atlas.json")
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
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"both\"\ntype = \"Sprite2D\"\nname = \"Both\"\nparent = \"root\"\n\n[node.props]\ntexture = \"res://a.png\"\natlas = \"res://sheet.atlas.json\"\nframe = \"player\"\n",
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
