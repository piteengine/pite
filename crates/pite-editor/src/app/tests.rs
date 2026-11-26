// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};

use eframe::egui;

use super::viewport::{
    fit_view, viewport_cursor, viewport_frame_rect, zoom_viewport_at, VIEWPORT_MAX_ZOOM,
    VIEWPORT_MIN_ZOOM,
};
use super::*;

#[test]
fn menu_separator_tracker_avoids_double_and_trailing_separators() {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let mut tracker = MenuSeparatorTracker::new(ui);
        tracker.start_section();
        assert!(!tracker.needs_separator);
        assert!(!tracker.has_previous_items);

        tracker.button("Item 1");
        assert!(tracker.has_previous_items);
        assert!(!tracker.needs_separator);

        tracker.start_section();
        assert!(tracker.needs_separator);
        tracker.start_section();
        assert!(tracker.needs_separator);

        tracker.button("Item 2");
        assert!(!tracker.needs_separator);

        tracker.start_section();
        assert!(tracker.needs_separator);
    });
    output.textures_delta.clear();
}

#[test]
fn cursor_maps_viewport_corners() {
    let rect =
        egui::Rect::from_min_size(egui::Pos2::new(10.0, 20.0), egui::Vec2::new(400.0, 300.0));
    assert_eq!(
        viewport_cursor(rect, (800, 600), egui::Pos2::new(10.0, 20.0)),
        Some((0.0, 0.0))
    );
    assert_eq!(
        viewport_cursor(rect, (800, 600), egui::Pos2::new(410.0, 320.0)),
        Some((800.0, 600.0))
    );
    assert_eq!(
        viewport_cursor(rect, (800, 600), egui::Pos2::new(210.0, 170.0)),
        Some((400.0, 300.0))
    );
}

#[test]
fn cursor_rejects_degenerate_viewport() {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(400.0, 300.0));
    assert_eq!(
        viewport_cursor(rect, (0, 600), egui::Pos2::new(10.0, 10.0)),
        None
    );
    let flat = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(0.0, 300.0));
    assert_eq!(
        viewport_cursor(flat, (800, 600), egui::Pos2::new(10.0, 10.0)),
        None
    );
}

#[test]
fn viewport_fits_available_space_on_both_axes() {
    let view = (800, 600);
    // Wide panel: height binds, width shrinks to match.
    let wide = fit_view(egui::Vec2::new(2000.0, 400.0), view);
    assert!((wide.y - 400.0).abs() < 0.01, "got {wide:?}");
    assert!((wide.x - 400.0 * 4.0 / 3.0).abs() < 0.01, "got {wide:?}");
    // Tall panel: width binds, height shrinks to match.
    let tall = fit_view(egui::Vec2::new(400.0, 2000.0), view);
    assert!((tall.x - 400.0).abs() < 0.01, "got {tall:?}");
    assert!((tall.y - 300.0).abs() < 0.01, "got {tall:?}");
    // Never exceeds the panel on either axis, at any panel shape.
    for (ax, ay) in [(2000.0, 400.0), (400.0, 2000.0), (800.0, 600.0), (1.0, 1.0)] {
        let f = fit_view(egui::Vec2::new(ax, ay), view);
        assert!(
            f.x <= ax + 0.01 && f.y <= ay + 0.01,
            "{f:?} exceeds {ax}x{ay}"
        );
    }
    // Degenerate input stays positive.
    let tiny = fit_view(egui::Vec2::ZERO, view);
    assert!(tiny.x > 0.0 && tiny.y > 0.0);
}

#[test]
fn viewport_is_centered_in_its_panel() {
    // 4:3 scene, panel wider than tall: fitted box is height-bound and must
    // sit centered horizontally, with equal margins on both sides.
    let avail = egui::Vec2::new(1000.0, 300.0);
    let fitted = fit_view(avail, (800, 600));
    let outer = egui::Rect::from_min_size(egui::Pos2::ZERO, avail);
    let view = egui::Rect::from_center_size(outer.center(), fitted);
    assert!((view.left() - (outer.right() - view.right())).abs() < 0.01);
    assert!(view.width() <= outer.width() + 0.01);
    assert!(view.height() <= outer.height() + 0.01);
}

#[test]
fn viewport_pan_and_zoom_transform_frame_and_pin_anchor() {
    let outer = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0));
    let view = (800, 600);

    // Identity pan/zoom matches fit_view centered in outer.
    let base = viewport_frame_rect(outer, view, egui::Vec2::ZERO, 1.0);
    assert!((base.width() - 800.0).abs() < 0.01);
    assert!((base.height() - 600.0).abs() < 0.01);
    assert!((base.center().x - 500.0).abs() < 0.01);

    // Zooming at an off-center anchor keeps the anchor's normalized UV fixed.
    let anchor = egui::pos2(650.0, 220.0);
    let u_before = (anchor.x - base.min.x) / base.width();
    let v_before = (anchor.y - base.min.y) / base.height();

    let (pan2, zoom2) = zoom_viewport_at(outer.center(), egui::Vec2::ZERO, 1.0, 2.0, anchor);
    assert!((zoom2 - 2.0).abs() < 0.001);
    let zoomed = viewport_frame_rect(outer, view, pan2, zoom2);
    let u_after = (anchor.x - zoomed.min.x) / zoomed.width();
    let v_after = (anchor.y - zoomed.min.y) / zoomed.height();
    assert!(
        (u_before - u_after).abs() < 0.001,
        "{u_before} != {u_after}"
    );
    assert!(
        (v_before - v_after).abs() < 0.001,
        "{v_before} != {v_after}"
    );

    // Zoom clamps cleanly to [VIEWPORT_MIN_ZOOM, VIEWPORT_MAX_ZOOM].
    let (_, min_z) = zoom_viewport_at(outer.center(), pan2, zoom2, 0.01, anchor);
    let (_, max_z) = zoom_viewport_at(outer.center(), pan2, zoom2, 99.0, anchor);
    assert!((min_z - VIEWPORT_MIN_ZOOM).abs() < 0.001);
    assert!((max_z - VIEWPORT_MAX_ZOOM).abs() < 0.001);
}

#[test]
fn editor_opens_gear_group_as_text() {
    for name in [
        "a.py", "p.toml", "c.cfg", "c.ini", "d.json", "e.yaml", "f.yml", "G.TOML",
    ] {
        assert!(EditorApp::editor_text_file(Path::new(name)), "{name}");
    }
    for name in ["a.png", "b.wav", "c.ptscn", "noext", "a.pyx"] {
        assert!(!EditorApp::editor_text_file(Path::new(name)), "{name}");
    }
}

#[test]
fn recent_list_round_trips_and_drops_missing() {
    let dir = std::env::temp_dir().join(format!("pite-recent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let present = dir.join("a.ptscn");
    std::fs::write(&present, "x").unwrap();
    let file = dir.join("recent.json");
    let recent = vec![present.clone(), dir.join("gone.ptscn")];
    EditorApp::write_recent_file(&file, &recent).unwrap();
    assert_eq!(EditorApp::read_recent_file(&file), vec![present]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn save_dialog_path_keeps_scene_extension() {
    assert_eq!(
        EditorApp::with_scene_extension(PathBuf::from("s/main")),
        PathBuf::from("s/main.ptscn")
    );
    assert_eq!(
        EditorApp::with_scene_extension(PathBuf::from("s/main.ptscn")),
        PathBuf::from("s/main.ptscn")
    );
}

#[test]
fn blank_scene_tracks_format_version() {
    let text = EditorApp::blank_scene_text();
    assert!(
        text.starts_with(&format!(
            "format_version = {}\n",
            pite_scene::FORMAT_VERSION
        )),
        "{text}"
    );
    assert!(text.contains("id = \"root\""));
}

#[test]
fn list_asset_dir_preserves_empty_folders_and_sorts_folders_first() {
    let dir = std::env::temp_dir().join(format!("pite-asset-scan-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("b_empty_folder")).unwrap();
    std::fs::create_dir_all(dir.join("a_folder")).unwrap();
    std::fs::write(dir.join("a_folder").join("nested.txt"), "hi").unwrap();
    std::fs::write(dir.join("z_file.py"), "# py").unwrap();
    std::fs::write(dir.join("a_file.txt"), "txt").unwrap();

    let (folders, files) = assets::list_asset_dir(&dir, "");
    assert_eq!(folders.len(), 2);
    assert_eq!(folders[0].name, "a_folder");
    assert_eq!(folders[1].name, "b_empty_folder");

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].name, "a_file.txt");
    assert_eq!(files[1].name, "z_file.py");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_unique_path_suffixes_collisions() {
    let dir = std::env::temp_dir().join(format!("pite-asset-uniq-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let p1 = assets::resolve_unique_path(&dir, "sprite.png");
    assert_eq!(p1, dir.join("sprite.png"));
    std::fs::write(&p1, "data").unwrap();

    let p2 = assets::resolve_unique_path(&dir, "sprite.png");
    assert_eq!(p2, dir.join("sprite (1).png"));
    std::fs::write(&p2, "data").unwrap();

    let p3 = assets::resolve_unique_path(&dir, "sprite.png");
    assert_eq!(p3, dir.join("sprite (2).png"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_duplicate_path_copies() {
    let dir = std::env::temp_dir().join(format!("pite-asset-dup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let original = dir.join("player.py");
    std::fs::write(&original, "code").unwrap();

    let d1 = assets::resolve_duplicate_path(&original);
    assert_eq!(d1, dir.join("player (copy).py"));
    std::fs::write(&d1, "code").unwrap();

    let d2 = assets::resolve_duplicate_path(&original);
    assert_eq!(d2, dir.join("player (copy 2).py"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn move_asset_rel_handles_files_and_cycles() {
    let dir = std::env::temp_dir().join(format!("pite-asset-move-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("hero.png"), "png").unwrap();

    // 1. Move file into subfolder
    let renames = assets::move_asset_rel(&dir, "hero.png", "sub").unwrap();
    assert_eq!(
        renames,
        vec![(
            "res://hero.png".to_string(),
            "res://sub/hero.png".to_string()
        )]
    );
    assert!(dir.join("sub").join("hero.png").exists());
    assert!(!dir.join("hero.png").exists());

    // 2. Reject cyclic move: folder into its own subfolder
    std::fs::create_dir_all(dir.join("parent").join("child")).unwrap();
    let cycle_err = assets::move_asset_rel(&dir, "parent", "parent/child").unwrap_err();
    assert!(format!("{cycle_err:#}").contains("Cannot move folder into itself"));

    // 3. Move file back to root
    let root_renames = assets::move_asset_rel(&dir, "sub/hero.png", "").unwrap();
    assert_eq!(
        root_renames,
        vec![(
            "res://sub/hero.png".to_string(),
            "res://hero.png".to_string()
        )]
    );
    assert!(dir.join("hero.png").exists());
    assert!(!dir.join("sub").join("hero.png").exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn rename_asset_rel_and_reference_pairs() {
    let dir = std::env::temp_dir().join(format!("pite-asset-ren-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("old_dir")).unwrap();
    std::fs::write(dir.join("old_dir").join("script.py"), "# code").unwrap();

    let renames = assets::rename_asset_rel(&dir, "old_dir", "new_dir").unwrap();
    assert_eq!(
        renames,
        vec![(
            "res://old_dir/script.py".to_string(),
            "res://new_dir/script.py".to_string()
        )]
    );
    assert!(dir.join("new_dir").join("script.py").exists());
    assert!(!dir.join("old_dir").exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_all_saves_both_scene_and_open_code() {
    let dir = std::env::temp_dir().join(format!("pite-save-all-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let scene_path = dir.join("main.ptscn");
    let script_path = dir.join("player.py");
    std::fs::write(&scene_path, EditorApp::blank_scene_text()).unwrap();
    std::fs::write(&script_path, "# initial script").unwrap();

    let mut app = EditorApp::new(&scene_path).expect("failed to open EditorApp");
    app.session.host().with_tree_mut(|tree| {
        let root = tree.root().unwrap().clone();
        ops::add_node(tree, Some(root), "Node2D", "child_node").unwrap();
    });

    app.code_file = Some(script_path.clone());
    app.code_text = "print('updated script')\n".to_string();

    app.save_all();

    let saved_scene = std::fs::read_to_string(&scene_path).unwrap();
    assert!(
        saved_scene.contains("child_node"),
        "scene was not saved: {saved_scene}"
    );

    let saved_script = std::fs::read_to_string(&script_path).unwrap();
    assert_eq!(saved_script, "print('updated script')\n");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_all_without_code_file_saves_scene_only() {
    let dir = std::env::temp_dir().join(format!("pite-save-all-no-code-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let scene_path = dir.join("main.ptscn");
    std::fs::write(&scene_path, EditorApp::blank_scene_text()).unwrap();

    let mut app = EditorApp::new(&scene_path).expect("failed to open EditorApp");
    app.session.host().with_tree_mut(|tree| {
        let root = tree.root().unwrap().clone();
        ops::add_node(tree, Some(root), "Node2D", "solo_node").unwrap();
    });
    assert!(app.code_file.is_none());

    app.save_all();

    let saved_scene = std::fs::read_to_string(&scene_path).unwrap();
    assert!(
        saved_scene.contains("solo_node"),
        "scene was not saved: {saved_scene}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn toggle_play_activates_viewport_tab() {
    let dir = std::env::temp_dir().join(format!("pite-toggle-play-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scene_path = dir.join("main.ptscn");
    std::fs::write(&scene_path, EditorApp::blank_scene_text()).unwrap();
    let store = dir.join("config").join(pite_project::TRUST_STORE_FILE);
    pite_project::trust_workspace_in(&store, &dir).unwrap();

    let mut app = EditorApp::new_with_trust_store(&scene_path, Some(store))
        .expect("failed to open EditorApp");
    app.tab = Tab::Code;
    assert_eq!(app.tab, Tab::Code);
    assert!(!app.playing);

    app.toggle_play();
    assert!(app.playing);
    assert_eq!(app.tab, Tab::Viewport);

    app.toggle_play();
    assert!(!app.playing);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn clear_console_clears_log_buffer() {
    let dir = std::env::temp_dir().join(format!("pite-clear-console-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let scene_path = dir.join("main.ptscn");
    std::fs::write(&scene_path, EditorApp::blank_scene_text()).unwrap();

    let mut app = EditorApp::new(&scene_path).expect("failed to open EditorApp");
    app.log("Test log entry 1");
    app.log("Test log entry 2");
    assert!(!app.console.is_empty());

    app.clear_console();
    assert!(app.console.is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn code_is_dirty_tracks_edits_and_clears_on_save() {
    let dir = std::env::temp_dir().join(format!("pite-code-dirty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let scene_path = dir.join("main.ptscn");
    let script_path = dir.join("player.py");
    std::fs::write(&scene_path, EditorApp::blank_scene_text()).unwrap();
    std::fs::write(&script_path, "x = 1\n").unwrap();

    let mut app = EditorApp::new(&scene_path).expect("failed to open EditorApp");
    assert!(!app.code_is_dirty());

    app.open_asset(&script_path);
    assert_eq!(app.tab, Tab::Code);
    assert!(!app.code_is_dirty());

    app.code_text.push_str("y = 2\n");
    assert!(app.code_is_dirty());

    app.save_code();
    assert!(!app.code_is_dirty());
    assert_eq!(
        std::fs::read_to_string(&script_path).unwrap(),
        "x = 1\ny = 2\n"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn open_asset_auto_saves_dirty_code_before_switching() {
    let dir = std::env::temp_dir().join(format!("pite-open-asset-dirty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let scene_path = dir.join("main.ptscn");
    let script1 = dir.join("first.py");
    let script2 = dir.join("second.py");
    std::fs::write(&scene_path, EditorApp::blank_scene_text()).unwrap();
    std::fs::write(&script1, "val = 1\n").unwrap();
    std::fs::write(&script2, "val = 2\n").unwrap();

    let mut app = EditorApp::new(&scene_path).expect("failed to open EditorApp");
    app.open_asset(&script1);
    app.code_text = "val = 99\n".to_string();
    assert!(app.code_is_dirty());

    // Re-opening the same asset must not overwrite unsaved edits
    app.open_asset(&script1);
    assert_eq!(app.code_text, "val = 99\n");

    // Opening another asset must auto-save first.py and load second.py
    app.open_asset(&script2);
    assert_eq!(std::fs::read_to_string(&script1).unwrap(), "val = 99\n");
    assert_eq!(app.code_file, Some(script2.clone()));
    assert_eq!(app.code_text, "val = 2\n");
    assert!(!app.code_is_dirty());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn atlas_editor_create_slice_rename_delete_and_open_asset() {
    let dir = std::env::temp_dir().join(format!("pite-atlas-editor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scene_path = dir.join("main.ptscn");
    std::fs::write(&scene_path, EditorApp::blank_scene_text()).unwrap();

    let png_path = dir.join("hero.png");
    let ptatlas_path = dir.join("hero.ptatlas");
    std::fs::write(&png_path, pite_runtime::DEFAULT_ICON_PNG).unwrap();
    let (sw, sh) = pite_render::atlas::sheet_dimensions(&png_path).unwrap();
    assert!(sw > 0 && sh > 0);

    // 1. Create atlas sliced by half width/height
    let tw = (sw / 2).max(1);
    let th = (sh / 2).max(1);
    let mut state = super::inspector::AtlasEditorState::create_from_png(
        &png_path,
        &ptatlas_path,
        "res://hero.ptatlas".to_string(),
        Some((tw, th)),
    )
    .unwrap();
    assert!(!state.dirty);
    assert!(ptatlas_path.is_file());
    assert!(!state.atlas.frames.is_empty());

    // 2. Append frame range to a new animation and rename a referenced frame
    let first = state.atlas.frames.keys().next().unwrap().clone();
    let last = state.atlas.frames.keys().last().unwrap().clone();
    state.atlas.animations.insert(
        "idle".to_string(),
        pite_render::atlas::AtlasAnimation {
            frames: Vec::new(),
            fps: 8.0,
            looping: true,
        },
    );
    let count = state.append_frame_range_to_anim("idle", &first, &last);
    assert_eq!(count, state.atlas.frames.len());
    assert!(state.rename_frame(&first, "hero_start"));
    assert!(state.atlas.frames.contains_key("hero_start"));
    assert_eq!(state.atlas.animations["idle"].frames[0], "hero_start");

    // 3. Save and open via EditorApp::open_asset
    state.save().unwrap();
    assert!(!state.dirty);
    assert!(state.rename_animation("idle", "walk"));
    assert!(state.atlas.animations.contains_key("walk"));

    let mut app = EditorApp::new(&scene_path).expect("failed to open EditorApp");
    app.open_asset(&ptatlas_path);
    assert!(app.atlas_editor.is_some());

    // 4. Delete frame prunes animation references
    let ed = app.atlas_editor.as_mut().unwrap();
    assert!(ed.delete_frame("hero_start"));
    assert!(!ed.atlas.frames.contains_key("hero_start"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn untrusted_workspace_opens_in_safe_mode_and_gates_play() {
    let dir = std::env::temp_dir().join(format!("pite-editor-trust-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let proj_untrusted = dir.join("untrusted");
    let proj_trusted = dir.join("trusted");
    let store = dir.join("config").join(pite_project::TRUST_STORE_FILE);
    let marker = proj_untrusted.join("ran.txt");

    for proj in [&proj_untrusted, &proj_trusted] {
        std::fs::create_dir_all(proj.join("scenes")).unwrap();
        std::fs::create_dir_all(proj.join("scripts")).unwrap();
        std::fs::write(proj.join("pite.toml"), "[project]\nname = \"test\"\n").unwrap();
        std::fs::write(
            proj.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"mover\"\ntype = \"Sprite2D\"\nname = \"Mover\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/mover.py\"\nclass = \"Mover\"\n",
        )
        .unwrap();
    }
    std::fs::write(
        proj_untrusted.join("scripts").join("mover.py"),
        format!(
            "import pathlib\nimport pite\n\nclass Mover(pite.Sprite2D):\n    def _ready(self):\n        pathlib.Path({:?}).write_text(\"ran\\n\")\n",
            marker.to_string_lossy()
        ),
    )
    .unwrap();
    std::fs::write(
        proj_trusted.join("scripts").join("mover.py"),
        "import pite\n\nclass Mover(pite.Sprite2D):\n    pass\n",
    )
    .unwrap();

    pite_project::trust_workspace_in(&store, &proj_trusted).unwrap();

    // Opening untrusted project enters Safe Mode and skips Python execution.
    let mut app = EditorApp::new_with_trust_store(
        &proj_untrusted.join("scenes").join("main.ptscn"),
        Some(store.clone()),
    )
    .unwrap();
    assert!(!app.workspace_trusted);
    assert!(app.session.is_safe_mode());
    assert!(!marker.exists());

    // Pressing Play in Safe Mode opens the trust prompt instead of playing.
    app.toggle_play();
    assert!(!app.playing);
    assert!(app.trust_modal_open);
    assert!(!marker.exists());

    // Trusting enables scripts, persists trust, and runs _ready().
    assert!(app.trust_current_workspace());
    assert!(app.workspace_trusted);
    assert!(!app.trust_modal_open);
    assert!(!app.session.is_safe_mode());
    assert!(pite_project::is_workspace_trusted_in(
        &store,
        &proj_untrusted
    ));
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "ran\n");

    // stop() preserves trusted state; toggle_play() now plays.
    app.toggle_play();
    assert!(app.playing);
    app.stop();
    assert!(!app.playing);
    assert!(!app.session.is_safe_mode());

    // Opening a trusted project stays trusted; untrusting then re-opening enters Safe Mode.
    pite_project::untrust_workspace_in(&store, &proj_untrusted).unwrap();
    app.open_scene_file(&proj_trusted.join("scenes").join("main.ptscn"));
    assert!(app.workspace_trusted);
    assert!(!app.session.is_safe_mode());

    let _ = std::fs::remove_file(&marker);
    app.open_scene_file(&proj_untrusted.join("scenes").join("main.ptscn"));
    assert!(!app.workspace_trusted);
    assert!(app.session.is_safe_mode());
    assert!(!marker.exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dynamic_schema_appears_in_node_hierarchy_and_inherits_icons() {
    use std::borrow::Cow;

    let mut schemas = pite_core::SchemaRegistry::new();
    schemas
        .register(pite_core::NodeSchema {
            type_name: Cow::Borrowed("DialogueBox"),
            description: Cow::Borrowed("Dialogue UI node"),
            inherits: Some(Cow::Borrowed("Control")),
            props: Vec::new(),
            signals: Vec::new(),
            abstract_class: false,
            plugin_id: Some("dialogue".to_string()),
        })
        .unwrap();

    // `Control` is abstract and inherits `Node2D`, so `DialogueBox` flattens under `Node2D`.
    let hierarchy = super::tree::build_node_hierarchy_from(&schemas);
    let node_branch = hierarchy
        .iter()
        .find(|n| n.type_name == "Node")
        .expect("root Node must exist");
    let node2d_branch = node_branch
        .children
        .iter()
        .find(|n| n.type_name == "Node2D")
        .expect("Node2D must exist under Node");
    assert!(
        node2d_branch
            .children
            .iter()
            .any(|c| c.type_name == "DialogueBox"),
        "DialogueBox should appear under nearest non-abstract ancestor Node2D"
    );

    // Inherits Control's icon and UI green color.
    assert!(
        icons::node_type_icon_in(&schemas, "DialogueBox") == icons::Icon::Control,
        "DialogueBox should inherit Control icon"
    );
    assert_eq!(
        icons::node_type_color_in(&schemas, "DialogueBox"),
        icons::node_type_color_in(&schemas, "Control")
    );
}

#[test]
fn editor_ui_stale_handle_fails_loudly_outside_scope() {
    use eframe::egui;
    use pite_script::pyo3::{self, types::PyAnyMethods};
    let ctx = egui::Context::default();
    let mut leaked_ui: Option<crate::plugins::PyEditorUI> = None;
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let (_scope, py_ui) = crate::plugins::EditorUiScope::enter(ui);
        leaked_ui = Some(py_ui);
    });
    out.textures_delta.clear();
    let leaked = leaked_ui.expect("EditorUiScope produced handle");
    pite_script::ensure_python_init();
    pyo3::Python::attach(|py| {
        let obj = pyo3::Bound::new(py, leaked).unwrap();
        let err = obj
            .call_method1("label", ("stale",))
            .expect_err("calling EditorUI outside scope must raise RuntimeError");
        assert!(err.to_string().contains("outside an active editor UI"));
    });
}

#[test]
fn editor_plugins_gate_on_trust_and_isolate_errors() {
    let dir = std::env::temp_dir().join(format!("pite-ed-plug-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("plugins/ed_plug")).unwrap();
    std::fs::create_dir_all(dir.join("plugins/bad_plug")).unwrap();

    std::fs::write(
        dir.join("pite.toml"),
        "[project]\nname = \"t\"\n\n[plugins]\ned_plug = { enabled = true }\nbad_plug = { enabled = true }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("plugins/ed_plug/plugin.toml"),
        "id = \"ed_plug\"\nname = \"Ed Plug\"\nversion = \"0.1.0\"\neditor = \"ed_plug.ed:EdPlugin\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("plugins/ed_plug/__init__.py"), "").unwrap();
    std::fs::write(
        dir.join("plugins/ed_plug/ed.py"),
        r#"
class EdPlugin:
    def on_enable(self, ctx):
        ctx.log("ed_plug enabled")

    def menu_items(self):
        return ["Bump Root"]

    def on_menu_action(self, label, ctx):
        if label == "Bump Root":
            ctx.set_prop("root", "text", "from_plugin")

    def on_bottom_panel(self, ui, ctx):
        ui.label("hi")
"#,
    )
    .unwrap();

    std::fs::write(
        dir.join("plugins/bad_plug/plugin.toml"),
        "id = \"bad_plug\"\nname = \"Bad Plug\"\nversion = \"0.1.0\"\neditor = \"bad_plug.missing:Nope\"\n",
    )
    .unwrap();

    let scene = dir.join("scenes/main.ptscn");
    std::fs::write(
        &scene,
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node\"\nname = \"Root\"\n",
    )
    .unwrap();

    let trust_store = dir.join("trust.toml");
    let mut app = EditorApp::new_with_trust_store(&scene, Some(trust_store)).unwrap();
    assert!(!app.workspace_trusted);
    assert!(app.editor_plugins.bottom_panel_tabs().is_empty());
    assert!(app.editor_plugins.menu_actions().is_empty());

    assert!(app.trust_current_workspace());
    assert!(app.workspace_trusted);
    assert_eq!(
        app.editor_plugins.bottom_panel_tabs(),
        vec![("ed_plug".to_string(), "Ed Plug".to_string())]
    );
    assert_eq!(
        app.editor_plugins.menu_actions(),
        vec![("ed_plug".to_string(), "Bump Root".to_string())]
    );
    assert!(app.console.iter().any(|l| l.contains("ed_plug enabled")));
    assert!(app.console.iter().any(|l| l.contains("bad_plug")));

    let snapshot = app.editor_snapshot();
    let (errs, acts) = app
        .editor_plugins
        .trigger_menu_action("ed_plug", "Bump Root", snapshot);
    app.apply_plugin_feedback(errs, acts);
    let prop = app.session.host().with_tree(|t| {
        t.get(&pite_core::NodeId::from("root"))
            .and_then(|n| n.props.get("text").cloned())
    });
    assert_eq!(prop, Some(pite_core::PropValue::Str("from_plugin".into())));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn undo_redo_covers_tree_props_coalescing_and_saved_index() {
    let dir = std::env::temp_dir().join(format!("pite-undo-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    let scene = dir.join("scenes/main.ptscn");
    std::fs::write(
        &scene,
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\nname = \"root\"\ntype = \"Node2D\"\n",
    )
    .unwrap();

    let mut app = EditorApp::new_with_trust_store(&scene, None).unwrap();
    assert!(!app.undo_history.is_dirty());

    // 1. Add child node
    let child_id = app
        .session
        .host()
        .with_tree_mut(|t| ops::add_node(t, Some(NodeId::from("root")), "Sprite2D", "hero"))
        .unwrap();
    app.record_inserted_subtree(&child_id);
    assert!(app.undo_history.is_dirty());

    // 2. Coalesced drag prop updates collapse into 1 undo step
    app.undo_history.begin_coalesce("prop:hero:position");
    app.set_prop_recorded(
        &child_id,
        "position",
        pite_core::PropValue::Vec2(10.0, 0.0),
        Some("prop:hero:position".to_string()),
    )
    .unwrap();
    app.set_prop_recorded(
        &child_id,
        "position",
        pite_core::PropValue::Vec2(25.0, 5.0),
        Some("prop:hero:position".to_string()),
    )
    .unwrap();
    app.undo_history.end_coalesce();

    // 3. Rename + Save (history preserved across save)
    app.rename_node_recorded(&child_id, "player").unwrap();
    app.save_scene();
    assert!(!app.undo_history.is_dirty());
    assert!(app.undo_history.can_undo());

    // Undo rename -> back to "hero", dirty again
    // Undo rename -> display name back to "hero", dirty again
    app.undo();
    assert!(app.undo_history.is_dirty());
    let name_after_undo = app
        .session
        .host()
        .with_tree(|t| t.get(&child_id).map(|n| n.name.clone()));
    assert_eq!(name_after_undo.as_deref(), Some("hero"));

    // Undo coalesced position drag -> single step restores default Vec2(0, 0)
    app.undo();
    let pos = app.session.host().with_tree(|t| {
        t.get(&child_id)
            .and_then(|n| n.props.get("position").cloned())
    });
    assert_eq!(pos, Some(pite_core::PropValue::Vec2(0.0, 0.0)));

    // Undo add -> node removed
    app.undo();
    assert!(!app.session.host().with_tree(|t| t.contains(&child_id)));
    assert!(!app.undo_history.can_undo());

    // Redo all 3 steps -> back to saved state (clean)
    app.redo();
    app.redo();
    app.redo();
    assert!(!app.undo_history.is_dirty());
    let (final_name, final_pos) = app.session.host().with_tree(|t| {
        let n = t.get(&child_id).unwrap();
        (n.name.clone(), n.props.get("position").cloned())
    });
    assert_eq!(final_name, "player");
    assert_eq!(final_pos, Some(pite_core::PropValue::Vec2(25.0, 5.0)));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn undo_redo_coalesces_tilemap_strokes() {
    use super::undo::{TileCellChange, UndoCommand};

    let mut hist = super::undo::UndoHistory::default();
    hist.begin_coalesce("tilemap:map:0");
    hist.push(UndoCommand::TileMapStroke {
        node_id: "map".to_string(),
        path: PathBuf::from("map.ptmap"),
        layer: 0,
        cells: vec![TileCellChange {
            x: 1,
            y: 2,
            old: None,
            new: Some("grass".to_string()),
        }],
        coalesce_key: Some("tilemap:map:0".to_string()),
    });
    hist.push(UndoCommand::TileMapStroke {
        node_id: "map".to_string(),
        path: PathBuf::from("map.ptmap"),
        layer: 0,
        cells: vec![
            TileCellChange {
                x: 1,
                y: 2,
                old: Some("grass".to_string()),
                new: Some("stone".to_string()),
            },
            TileCellChange {
                x: 2,
                y: 2,
                old: None,
                new: Some("stone".to_string()),
            },
        ],
        coalesce_key: Some("tilemap:map:0".to_string()),
    });
    hist.end_coalesce();

    let cmd = hist.pop_undo().expect("one coalesced stroke");
    assert!(!hist.can_undo());
    match cmd {
        UndoCommand::TileMapStroke { cells, .. } => {
            assert_eq!(cells.len(), 2);
            assert_eq!(cells[0].old, None);
            assert_eq!(cells[0].new.as_deref(), Some("stone"));
            assert_eq!(cells[1].new.as_deref(), Some("stone"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}
