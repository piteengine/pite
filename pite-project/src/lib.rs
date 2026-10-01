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
}

impl Default for ProjectMeta {
    fn default() -> Self {
        Self {
            name: default_name(),
            pite_version: default_version(),
            main_scene: default_main_scene(),
            python: default_python(),
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
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            platforms: default_platforms(),
            binary_name: None,
            include: Vec::new(),
        }
    }
}

pub fn load_manifest(dir: &Path) -> Result<PiteManifest> {
    let path = dir.join(MANIFEST_FILE);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
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
        anyhow::bail!("unknown template {template:?} (only \"minimal-2d\" exists in M0)");
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
