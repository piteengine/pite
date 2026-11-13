// SPDX-License-Identifier: MIT OR Apache-2.0

use pite_render::Renderer2D;

use super::*;

mod input;
mod misc;
mod physics;
mod reload;
mod render;
mod scripting;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn make_project(tag: &str, class: &str, script_src: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("pite-sess-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        format!("format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"mover\"\ntype = \"Sprite2D\"\nname = \"Mover\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/mover.py\"\nclass = \"{class}\"\n"),
    )
    .unwrap();
    let script = dir.join("scripts").join("mover.py");
    std::fs::write(&script, script_src).unwrap();
    (dir, script)
}
