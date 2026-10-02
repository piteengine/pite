//! `pite-project`: `pite.toml` manifest, `res://` paths, templates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "pite.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiteManifest {
    #[serde(default)]
    pub project: ProjectMeta,
    #[serde(default)]
    pub export: ExportConfig,
    #[serde(flatten, default)]
    pub extra: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_version")]
    pub pite_version: String,
    #[serde(default = "default_main_scene")]
    pub main_scene: String,
    #[serde(default = "default_python")]
    pub python: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    #[serde(default = "default_window_height")]
    pub window_height: u32,
    #[serde(flatten, default)]
    pub extra: HashMap<String, toml::Value>,
}

impl Default for ProjectMeta {
    fn default() -> Self {
        Self {
            name: default_name(),
            pite_version: default_version(),
            main_scene: default_main_scene(),
            python: default_python(),
            author: String::new(),
            icon: String::new(),
            window_width: default_window_width(),
            window_height: default_window_height(),
            extra: HashMap::new(),
        }
    }
}

fn default_name() -> String {
    "my-game".to_string()
}
fn default_version() -> String {
    "0.1".to_string()
}
fn default_main_scene() -> String {
    "res://scenes/main.pitescene".to_string()
}
fn default_python() -> String {
    "3.12".to_string()
}
fn default_window_width() -> u32 {
    800
}
fn default_window_height() -> u32 {
    600
}

fn default_platforms() -> Vec<String> {
    vec!["linux".to_string()]
}

/// Export settings. Everything defaults; old projects without this
/// table export with `platforms = ["linux"]` under the project name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    #[serde(default = "default_platforms")]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub binary_name: Option<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(flatten, default)]
    pub extra: HashMap<String, toml::Value>,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            platforms: default_platforms(),
            binary_name: None,
            include: Vec::new(),
            extra: HashMap::new(),
        }
    }
}

pub fn load_manifest(dir: &Path) -> Result<PiteManifest> {
    let path = dir.join(MANIFEST_FILE);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
}

/// Replacement for a deprecated manifest key, if any. Empty today: the
/// registry exists so a future rename warns instead of silently changing meaning.
pub fn deprecated_replacement(field: &str) -> Option<&'static str> {
    let _ = field;
    None
}

/// Pure manifest validation: unknown/deprecated keys warn (sorted, stable),
/// impossible values error. Filesystem checks (icon exists) live in `pite check`.
pub fn validate_manifest(manifest: &PiteManifest) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut unknown: Vec<String> = manifest.extra.keys().cloned().collect();
    for key in manifest.project.extra.keys() {
        unknown.push(format!("project.{key}"));
    }
    for key in manifest.export.extra.keys() {
        unknown.push(format!("export.{key}"));
    }
    unknown.sort();
    for field in unknown {
        match deprecated_replacement(&field) {
            Some(use_instead) => warnings
                .push(format!("manifest: {field:?} is deprecated, use {use_instead:?}")),
            None => warnings.push(format!("manifest: unknown field {field:?}")),
        }
    }
    if manifest.project.python.is_empty() {
        errors.push("manifest: project.python must not be empty".to_string());
    }
    if manifest.project.window_width == 0 || manifest.project.window_height == 0 {
        errors.push(format!(
            "manifest: window is {}x{}, dimensions must be positive",
            manifest.project.window_width, manifest.project.window_height
        ));
    }
    if !manifest.project.icon.is_empty() && !manifest.project.icon.starts_with("res://") {
        errors.push(format!(
            "manifest: icon {:?} must be res:// or empty (absolute paths are forbidden)",
            manifest.project.icon
        ));
    }
    (errors, warnings)
}

pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if dir.join(MANIFEST_FILE).is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn resolve_res(project_dir: &Path, res_path: &str) -> Option<PathBuf> {
    res_path
        .strip_prefix("res://")
        .map(|rel| project_dir.join(rel))
}

pub fn to_res_path(project_dir: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(project_dir).ok().map(|rel| {
        format!(
            "res://{}",
            rel.to_string_lossy().replace('\\', "/")
        )
    })
}

pub const TEMPLATE_PITE_TOML: &str = r#"[project]
name = "TEMPLATE_NAME"
pite_version = "0.1"
main_scene = "res://scenes/main.pitescene"
python = "3.12"
author = ""
icon = ""
window_width = 800
window_height = 600
"#;

pub const TEMPLATE_SCENE: &str = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "player"
type = "Sprite2D"
name = "Player"
parent = "root"

[node.props]
texture = "res://assets/player.png"

[node.script]
path = "res://scripts/player.py"
class = "Player"

[[node]]
id = "cam"
type = "Camera2D"
name = "Camera"
parent = "root"

[node.props]
position = [100.0, 200.0]
"#;

pub const TEMPLATE_SCRIPT: &str = r#"import pite


class Player(pite.Node2D):
    speed: float = 200.0

    def _ready(self):
        self.position = (100.0, 200.0)

    def _process(self, delta: float):
        x, y = self.position
        if pite.held("ArrowRight"):
            x += self.speed * delta
        if pite.held("ArrowLeft"):
            x -= self.speed * delta
        if pite.held("ArrowDown"):
            y += self.speed * delta
        if pite.held("ArrowUp"):
            y -= self.speed * delta
        if pite.pressed("Space"):
            x += 10.0
        self.position = (x, y)
"#;

/// Valid 1x1 RGBA PNG backing the `res://assets/player.png` reference in
/// [`TEMPLATE_SCENE`], so fresh scaffolds pass `pite check`.
const TEMPLATE_PLAYER_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 255, 255, 255, 127, 0,
    9, 251, 3, 253, 42, 134, 227, 138, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

pub fn create_project(dest: &Path, name: &str, template: &str) -> Result<PathBuf> {
    if template != "minimal-2d" {
        anyhow::bail!("unknown template {template:?} (only \"minimal-2d\" exists)");
    }
    let dir = if dest.file_name().map(|n| n == name).unwrap_or(false) {
        dest.to_path_buf()
    } else {
        dest.join(name)
    };
    if dir.exists() {
        anyhow::bail!("{} already exists", dir.display());
    }
    std::fs::create_dir_all(dir.join("scenes")).with_context(|| "cannot create project dirs")?;
    std::fs::create_dir_all(dir.join("scripts"))?;
    std::fs::create_dir_all(dir.join("assets"))?;
    std::fs::write(
        dir.join(MANIFEST_FILE),
        TEMPLATE_PITE_TOML.replace("TEMPLATE_NAME", name),
    )?;
    std::fs::write(dir.join("scenes").join("main.pitescene"), TEMPLATE_SCENE)?;
    std::fs::write(dir.join("scripts").join("player.py"), TEMPLATE_SCRIPT)?;
    std::fs::write(dir.join("assets").join("player.png"), TEMPLATE_PLAYER_PNG)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmpdir(tag: &str) -> PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join(format!("pite-manifest-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn load_text(text: &str) -> PiteManifest {
        let dir = tmpdir("parse");
        std::fs::write(dir.join(MANIFEST_FILE), text).unwrap();
        let manifest = load_manifest(&dir).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        manifest
    }

    #[test]
    fn old_manifest_parses_untouched() {
        let manifest = load_text("[project]\nname = \"old\"\n");
        assert_eq!(manifest.project.name, "old");
        assert_eq!(manifest.project.pite_version, "0.1");
        assert_eq!(manifest.project.main_scene, "res://scenes/main.pitescene");
        assert_eq!(manifest.project.python, "3.12");
        assert_eq!(manifest.project.author, "");
        assert_eq!(manifest.project.icon, "");
        assert_eq!(manifest.project.window_width, 800);
        assert_eq!(manifest.project.window_height, 600);
        assert!(manifest.project.extra.is_empty());
        let (errors, warnings) = validate_manifest(&manifest);
        assert!(errors.is_empty(), "got {errors:?}");
        assert!(warnings.is_empty(), "got {warnings:?}");
    }

    #[test]
    fn bad_type_fails_loudly() {
        let dir = tmpdir("bad-type");
        std::fs::write(dir.join(MANIFEST_FILE), "[project]\nname = 5\n").unwrap();
        let err = load_manifest(&dir).unwrap_err();
        assert!(err.to_string().contains("cannot parse"), "got {err:#}");
        std::fs::write(
            dir.join(MANIFEST_FILE),
            "[project]\nname = \"x\"\nwindow_width = \"wide\"\n",
        )
        .unwrap();
        let err = load_manifest(&dir).unwrap_err();
        assert!(err.to_string().contains("cannot parse"), "got {err:#}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_keys_survive_round_trip() {
        let manifest = load_text(
            "custom = 1\n\n[project]\nname = \"x\"\nnickname = \"y\"\n\n[export]\nbundle = true\n",
        );
        assert_eq!(
            manifest.project.extra.get("nickname"),
            Some(&toml::Value::String("y".to_string()))
        );
        let again: PiteManifest = toml::from_str(&toml::to_string(&manifest).unwrap()).unwrap();
        assert_eq!(manifest.extra, again.extra);
        assert_eq!(manifest.project.extra, again.project.extra);
        assert_eq!(manifest.export.extra, again.export.extra);
        let (_, warnings) = validate_manifest(&again);
        for field in ["custom", "project.nickname", "export.bundle"] {
            assert!(
                warnings.iter().any(|w| w.contains(field)),
                "missing warning for {field}, got: {warnings:?}"
            );
        }
    }

    #[test]
    fn impossible_values_are_errors() {
        let manifest = load_text(
            "[project]\nname = \"x\"\npython = \"\"\nwindow_width = 0\nicon = \"/abs/icon.png\"\n",
        );
        let (errors, _) = validate_manifest(&manifest);
        assert!(errors.iter().any(|e| e.contains("python")), "got {errors:?}");
        assert!(errors.iter().any(|e| e.contains("0x")), "got {errors:?}");
        assert!(errors.iter().any(|e| e.contains("icon")), "got {errors:?}");
    }
}
