// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};

use eframe::egui;

use super::viewport::{fit_view, viewport_cursor};
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
    std::fs::create_dir_all(&dir).unwrap();
    let scene_path = dir.join("main.ptscn");
    std::fs::write(&scene_path, EditorApp::blank_scene_text()).unwrap();

    let mut app = EditorApp::new(&scene_path).expect("failed to open EditorApp");
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
