// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behavior tests for the scene walk (the rendering lives in `ui`/`report`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::orphan_findings;
use super::walk::{script_imports, Walker};
use crate::diagnostics::{Kind, Severity};

const FIXTURE_SCENE: &str = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "sprite"
type = "Sprite2D"
name = "Sprite"
parent = "root"

[node.props]
texture = "res://assets/gone.png"
"#;

fn fixture_root(n: u32) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pite-check-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).expect("create scenes dir");
    std::fs::create_dir_all(dir.join("assets")).expect("create assets dir");
    std::fs::write(dir.join("scenes").join("main.ptscn"), FIXTURE_SCENE)
        .expect("write fixture scene");
    dir
}

struct WalkResult {
    errors: Vec<String>,
    warnings: Vec<String>,
    cache: Vec<String>,
    referenced: HashSet<String>,
    kinds: Vec<Kind>,
}

fn run_ref(
    root: &Path,
    registry: &pite_assets::UidRegistry,
    by_hash: &HashMap<&str, &str>,
) -> WalkResult {
    run_ref_with_props(root, registry, by_hash, &HashMap::new())
}

fn run_ref_with_props(
    root: &Path,
    registry: &pite_assets::UidRegistry,
    by_hash: &HashMap<&str, &str>,
    custom_props: &HashMap<String, String>,
) -> WalkResult {
    let mut walker = Walker::new(root, custom_props, registry, by_hash);
    walker.walk("res://scenes/main.ptscn");
    let errors = walker
        .findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .map(|finding| finding.message.to_string())
        .collect();
    let warnings = walker
        .findings
        .iter()
        .filter(|finding| finding.severity == Severity::Warning)
        .map(|finding| finding.message.to_string())
        .collect();
    let kinds = walker.findings.iter().map(|finding| finding.kind).collect();
    WalkResult {
        errors,
        warnings,
        cache: walker
            .cache
            .iter()
            .map(|entry| format!("{} {}", entry.status, entry.file))
            .collect(),
        referenced: walker.referenced,
        kinds,
    }
}

#[test]
fn missing_asset_is_error() {
    let root = fixture_root(1);
    let registry = pite_assets::UidRegistry::default();
    let by_hash: HashMap<&str, &str> = HashMap::new();
    let result = run_ref(&root, &registry, &by_hash);
    assert!(
        result.errors.iter().any(|e| e.contains("does not exist")),
        "expected a missing-asset error, got: {:?}",
        result.errors
    );
    assert!(
        result.kinds.contains(&Kind::Asset),
        "missing asset must be typed as an asset finding, got: {:?}",
        result.kinds
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn present_asset_is_clean() {
    let root = fixture_root(2);
    std::fs::write(root.join("assets").join("gone.png"), b"fake-png-bytes")
        .expect("write fixture asset");
    let registry = pite_assets::UidRegistry::default();
    let by_hash: HashMap<&str, &str> = HashMap::new();
    let result = run_ref(&root, &registry, &by_hash);
    assert!(
        result.errors.is_empty(),
        "expected zero errors, got: {:?}",
        result.errors
    );
    assert_eq!(
        result.cache.len(),
        1,
        "check must report cache status, got: {:?}",
        result.cache
    );
    assert!(
        result.cache[0].starts_with("miss "),
        "first run populates the cache, got: {:?}",
        result.cache
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn instanced_scene_appears_once_in_the_cache() {
    let root = fixture_root(11);
    std::fs::write(
        root.join("scenes").join("child.ptscn"),
        "format_version = 1\nroot = \"child\"\n\n[[node]]\nid = \"child\"\ntype = \"Node2D\"\nname = \"Child\"\n",
    )
    .expect("write child scene");
    let main = format!(
        "{FIXTURE_SCENE}\n[[instance]]\nscene = \"res://scenes/child.ptscn\"\nparent = \"root\"\n"
    );
    std::fs::write(root.join("scenes").join("main.ptscn"), main).expect("write main scene");
    let registry = pite_assets::UidRegistry::default();
    let by_hash: HashMap<&str, &str> = HashMap::new();
    let result = run_ref(&root, &registry, &by_hash);
    let files: HashSet<&str> = result
        .cache
        .iter()
        .map(|entry| entry.split_once(' ').map(|(_, file)| file).unwrap_or(entry))
        .collect();
    assert_eq!(
        files.len(),
        result.cache.len(),
        "each scene file must appear once in the cache, got: {:?}",
        result.cache
    );
    assert_eq!(result.cache.len(), 2, "got: {:?}", result.cache);
    let _ = std::fs::remove_dir_all(&root);
}

fn scanned(paths: &[&str]) -> Vec<pite_assets::ScannedFile> {
    paths
        .iter()
        .map(|path| pite_assets::ScannedFile {
            res_path: path.to_string(),
            fs_path: PathBuf::from(&path["res://".len()..]),
            hash: String::new(),
            size: 0,
        })
        .collect()
}

#[test]
fn prop_references_leave_no_orphan() {
    let root = fixture_root(3);
    let files = scanned(&["res://assets/gone.png"]);
    let by_path: HashMap<&str, &pite_assets::ScannedFile> = files
        .iter()
        .map(|file| (file.res_path.as_str(), file))
        .collect();
    let registry = pite_assets::UidRegistry::default();
    let by_hash: HashMap<&str, &str> = HashMap::new();
    let result = run_ref(&root, &registry, &by_hash);
    assert!(
        result.referenced.contains("res://assets/gone.png"),
        "got: {:?}",
        result.referenced
    );
    assert!(orphan_findings(&by_path, &result.referenced).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unreferenced_assets_warn_sorted() {
    let files = scanned(&[
        "res://assets/zebra.png",
        "res://assets/apple.png",
        "res://assets/used.wav",
    ]);
    let by_path: HashMap<&str, &pite_assets::ScannedFile> = files
        .iter()
        .map(|file| (file.res_path.as_str(), file))
        .collect();
    let mut referenced = HashSet::new();
    referenced.insert("res://assets/used.wav".to_string());
    let messages: Vec<String> = orphan_findings(&by_path, &referenced)
        .into_iter()
        .map(|finding| finding.message.to_string())
        .collect();
    assert_eq!(
        messages,
        vec![
            "Asset res://assets/apple.png is never referenced by any reachable scene.".to_string(),
            "Asset res://assets/zebra.png is never referenced by any reachable scene.".to_string(),
        ]
    );
}

#[test]
fn played_literals_count_as_references() {
    let root = fixture_root(4);
    std::fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
    std::fs::write(
        root.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"sfx\"\ntype = \"Node\"\nname = \"Sfx\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/sfx.py\"\nclass = \"Sfx\"\n",
    )
    .expect("write scene");
    std::fs::write(
        root.join("scripts").join("sfx.py"),
        "import pite\n\nclass Sfx(pite.Node):\n    def _ready(self):\n        pite.play(\"res://assets/hit.wav\")\n",
    )
    .expect("write script");
    let files = scanned(&["res://assets/hit.wav"]);
    let by_path: HashMap<&str, &pite_assets::ScannedFile> = files
        .iter()
        .map(|file| (file.res_path.as_str(), file))
        .collect();
    let by_hash: HashMap<&str, &str> = HashMap::new();
    let registry = pite_assets::UidRegistry::default();
    let result = run_ref(&root, &registry, &by_hash);
    assert!(
        result.referenced.contains("res://assets/hit.wav"),
        "got: {:?}",
        result.referenced
    );
    assert!(orphan_findings(&by_path, &result.referenced).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn check_validates_audio_multiformat() {
    let root = fixture_root(42);
    std::fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
    std::fs::create_dir_all(root.join("assets")).expect("create assets dir");
    std::fs::write(
        root.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"sfx\"\ntype = \"Node\"\nname = \"Sfx\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/sfx.py\"\nclass = \"Sfx\"\n",
    )
    .expect("write scene");
    std::fs::write(
        root.join("scripts").join("sfx.py"),
        "import pite\n\nclass Sfx(pite.Node):\n    def _ready(self):\n        pite.play(\"res://assets/music.ogg\")\n        pite.play(\"res://assets/bad.mp3\")\n",
    )
    .expect("write script");
    std::fs::write(
        root.join("assets").join("music.ogg"),
        pite_audio::fixtures::FIXTURE_OGG,
    )
    .unwrap();
    std::fs::write(root.join("assets").join("bad.mp3"), b"ID3_NOT_A_VALID_MP3").unwrap();

    let by_hash: HashMap<&str, &str> = HashMap::new();
    let registry = pite_assets::UidRegistry::default();
    let result = run_ref(&root, &registry, &by_hash);
    assert!(result.referenced.contains("res://assets/music.ogg"));
    assert!(result.referenced.contains("res://assets/bad.mp3"));
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.contains("bad.mp3") && error.contains("cannot be decoded")),
        "expected an undecodable error for bad.mp3, got: {:?}",
        result.errors
    );
    assert!(
        !result
            .errors
            .iter()
            .any(|error| error.contains("music.ogg")),
        "music.ogg should decode cleanly, got: {:?}",
        result.errors
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn script_imports_extracts_top_levels() {
    let text = "import pite\nimport os, sys # comment\nfrom collections import OrderedDict\nfrom . import helper\nfrom pkg.sub import thing\nimport numpy as np\n# import commented_out\nx = \"import string_in_literal\"\n";
    assert_eq!(
        script_imports(text),
        vec!["pite", "os", "sys", "collections", "pkg", "numpy"]
    );
}

#[test]
fn third_party_import_warns_once() {
    let root = fixture_root(5);
    std::fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
    std::fs::write(
        root.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"sfx\"\ntype = \"Node\"\nname = \"Sfx\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/sfx.py\"\nclass = \"Sfx\"\n",
    )
    .expect("write scene");
    std::fs::write(
        root.join("scripts").join("sfx.py"),
        "import pite\nimport os\nimport numpy\nimport vpkg\nfrom collections import OrderedDict\nfrom . import helper\nimport helper\n",
    )
    .expect("write script");
    std::fs::write(root.join("scripts").join("helper.py"), "x = 1\n").expect("write sibling");
    std::fs::create_dir_all(root.join("vendor").join("vpkg")).expect("vendor dir");
    std::fs::write(
        root.join("vendor").join("vpkg").join("__init__.py"),
        "y = 2\n",
    )
    .expect("write vendored");
    let by_hash: HashMap<&str, &str> = HashMap::new();
    let registry = pite_assets::UidRegistry::default();
    let result = run_ref(&root, &registry, &by_hash);
    assert!(result.errors.is_empty(), "got: {:?}", result.errors);
    assert_eq!(result.warnings.len(), 1, "got: {:?}", result.warnings);
    assert!(
        result.warnings[0].contains("numpy") && result.warnings[0].contains("not vendored"),
        "got: {:?}",
        result.warnings
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn custom_props_are_validated() {
    let root = fixture_root(99);
    std::fs::write(
        root.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[node.props]\nspawn_group = \"warrior\"\nboss_hp = \"lots\"\ntypo_field = 42\n",
    )
    .unwrap();
    let mut custom = HashMap::new();
    custom.insert("spawn_group".to_string(), "string".to_string());
    custom.insert("boss_hp".to_string(), "int".to_string());

    let by_hash: HashMap<&str, &str> = HashMap::new();
    let result = run_ref_with_props(
        &root,
        &pite_assets::UidRegistry::default(),
        &by_hash,
        &custom,
    );
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.contains("boss_hp") && error.contains("wrong type")),
        "got errors: {:?}",
        result.errors
    );
    assert!(
        !result
            .errors
            .iter()
            .any(|error| error.contains("spawn_group")),
        "spawn_group must be silent, got errors: {:?}",
        result.errors
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.contains("typo_field")),
        "got warnings: {:?}",
        result.warnings
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn check_validates_tilemap_assets() {
    let root = fixture_root(6);
    std::fs::create_dir_all(root.join("maps")).expect("create maps dir");
    std::fs::write(
        root.join("scenes").join("main.ptscn"),
        r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "map"
type = "TileMap2D"
name = "TileMap"
parent = "root"

[node.props]
tilemap = "res://maps/level.ptmap"
"#,
    )
    .expect("write scene");

    std::fs::write(
        root.join("maps").join("level.ptmap"),
        r#"atlas = "res://tiles.atlas.json"
tile_size = [16, 16]

[[layer]]
name = "ground"
tiles = [[0, 0, "grass"], [1, 0, "dirt"]]
"#,
    )
    .expect("write tilemap");

    let by_hash: HashMap<&str, &str> = HashMap::new();
    let registry = pite_assets::UidRegistry::default();
    let custom_props = HashMap::new();
    let mut walker = Walker::new(&root, &custom_props, &registry, &by_hash);
    walker.walk("res://scenes/main.ptscn");

    assert!(walker.findings.is_empty(), "got: {:?}", walker.findings);
    assert!(walker.referenced.contains("res://maps/level.ptmap"));
    assert!(walker.referenced.contains("res://tiles.atlas.json"));
    assert_eq!(walker.atlas_uses.len(), 2);
    assert_eq!(walker.atlas_uses[0].frame, "grass");
    assert_eq!(walker.atlas_uses[1].frame, "dirt");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn animated_sprite_validation() {
    let root = fixture_root(77);
    std::fs::write(
        root.join("assets").join("hero.atlas.json"),
        r#"{"texture":"hero.png","size":[64,32],
            "frames":{"idle1":{"x":0,"y":0,"w":32,"h":32}},
            "animations":{"idle":{"frames":["idle1"],"fps":6.0}}}"#,
    )
    .unwrap();

    std::fs::write(
        root.join("scenes").join("main.ptscn"),
        r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "anim_spr"
type = "AnimatedSprite2D"
name = "Hero"
parent = "root"

[node.props]
atlas = "res://assets/hero.atlas.json"
animation = "idle"
"#,
    )
    .unwrap();

    let by_hash: HashMap<&str, &str> = HashMap::new();
    let registry = pite_assets::UidRegistry::default();
    let custom_props = HashMap::new();
    let mut walker = Walker::new(&root, &custom_props, &registry, &by_hash);
    walker.walk("res://scenes/main.ptscn");

    assert!(walker.findings.is_empty(), "got: {:?}", walker.findings);
    assert_eq!(walker.anim_uses.len(), 1);
    assert_eq!(walker.anim_uses[0].animation, "idle");
    assert!(walker.referenced.contains("res://assets/hero.atlas.json"));

    let (report_errors, report_warnings) =
        pite_assets::atlas_report(&root, &walker.atlas_uses, &walker.anim_uses);
    assert!(
        report_errors.is_empty(),
        "got report errors: {report_errors:?}"
    );
    assert!(
        report_warnings.is_empty(),
        "got report warnings: {report_warnings:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
