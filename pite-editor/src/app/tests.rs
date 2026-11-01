// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};

use eframe::egui;

use super::viewport::{fit_view, viewport_cursor};
use super::*;

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
