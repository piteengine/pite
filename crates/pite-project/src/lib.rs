// SPDX-License-Identifier: MIT OR Apache-2.0
//! Project manifest management, `res://` resource resolution, and project templates for Pite.
//!
//! Provides:
//! - [`PiteManifest`]: Strongly-typed parsing and serialization of `pite.toml` manifests.
//! - Resource path resolution: converting between filesystem paths and `res://` project-relative URI strings.
//! - Validation rules: schema checks, deprecation warnings, and unknown property diagnostics.
//! - Project scaffolding: [`create_project`] for generating starter scenes, scripts, and configurations.

#![forbid(unsafe_code)]

mod error;
mod plugins;
mod trust;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use error::{ProjectError, Result};
pub use plugins::{
    is_pite_version_compatible, is_valid_plugin_id, load_enabled_plugins, load_plugin_manifest,
    plugin_dir, save_plugin_manifest, LoadedPlugin, LoadedPlugins, PluginEntry, PluginLoadError,
    PluginManifest, PluginNodeDef, PluginNodePropDef, PluginSignalDef, PLUGINS_DIR,
    PLUGIN_MANIFEST_FILE,
};
pub use trust::{
    config_dir, is_workspace_trusted, is_workspace_trusted_in, load_trusted_workspaces_from,
    normalize_workspace_path, save_trusted_workspaces_to, trust_store_path, trust_workspace,
    trust_workspace_in, untrust_workspace_in, workspace_root, TrustedWorkspaces, TRUST_STORE_FILE,
};

/// Standard manifest filename located at the root of every Pite project.
pub const MANIFEST_FILE: &str = "pite.toml";

/// Complete configuration document represented by `pite.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiteManifest {
    #[serde(default)]
    pub project: ProjectMeta,
    #[serde(default)]
    pub export: ExportConfig,
    /// Pinned third-party packages (`pite vendor add`). The table is the
    /// lock: committed `vendor/` bytes must match it (`pite vendor verify`).
    #[serde(default)]
    pub vendor: HashMap<String, VendorEntry>,
    /// Project-level custom prop declarations. Each entry is `name = "type"`
    /// with `type` one of `string`, `int`, `number`, `bool`, `vec2`, `color`
    /// (aliases accepted). Props declared here are readable/writable from
    /// scripts via `get_custom_property` / `set_custom_property`, survive
    /// scene saves, and no longer trip `pite check` "unknown prop" warnings.
    /// Deleting a declaration is safe: unread nodes ignore it, and scripts
    /// that write it fail loudly at the next `set_custom_property`.
    #[serde(default)]
    pub props: HashMap<String, String>,
    /// Enabled/installed project plugins (`<id> = true|false` or `{ enabled, source, rev }`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugins: BTreeMap<String, PluginEntry>,
    #[serde(flatten, default)]
    pub extra: HashMap<String, toml::Value>,
}

/// Metadata and windowing configuration in `[project]`.
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
    "0.4.0".to_string()
}
fn default_main_scene() -> String {
    "res://scenes/main.ptscn".to_string()
}
fn default_python() -> String {
    "3.12".to_string()
}
fn default_window_width() -> u32 {
    1280
}
fn default_window_height() -> u32 {
    720
}

/// Export settings. Everything defaults. `platforms` was removed in 0.1.1
/// (export never read it); manifests that still carry it parse untouched
/// and warn via `validate_manifest`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportConfig {
    #[serde(default)]
    pub binary_name: Option<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(flatten, default)]
    pub extra: HashMap<String, toml::Value>,
}

/// One pinned package in `[vendor]`. `sha256` covers the stored artifact:
/// the wheel file when zipped, a canonical tree hash when unpacked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorEntry {
    pub version: String,
    pub sha256: String,
    /// False for transitive deps pulled in by `add`; true for asked-for ones.
    #[serde(default)]
    pub direct: bool,
}

/// Loads and parses a `pite.toml` manifest from the given directory.
pub fn load_manifest(dir: &Path) -> Result<PiteManifest> {
    let path = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|source| ProjectError::Io {
        path: path.clone(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| ProjectError::TomlParse { path, source })
}

/// Write the manifest back to `dir/pite.toml`. Serializes through
/// `toml::Value` (sorted tables) so saves are byte-stable even with unknown
/// `extra` keys aboard; known fields land alphabetically on disk.
pub fn save_manifest(dir: &Path, manifest: &PiteManifest) -> Result<()> {
    let path = dir.join(MANIFEST_FILE);
    let value = toml::Value::try_from(manifest)?;
    let mut text = toml::to_string_pretty(&value)?;
    text.push('\n');
    std::fs::write(&path, text).map_err(|source| ProjectError::Io { path, source })
}

/// Replacement for a deprecated manifest key, if any. Empty today: the
/// registry exists so a future rename warns instead of silently changing meaning.
pub fn deprecated_replacement(field: &str) -> Option<&'static str> {
    let _ = field;
    None
}

/// Structured manifest validation finding returned by [`validate_manifest_structured`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestIssue {
    DeprecatedIgnored {
        field: String,
        removed_in: &'static str,
    },
    DeprecatedReplacement {
        field: String,
        replacement: &'static str,
    },
    UnknownField {
        field: String,
    },
    EmptyPython,
    InvalidWindowDimensions {
        width: u32,
        height: u32,
    },
    InvalidIconPath {
        icon: String,
    },
    UnknownCustomPropType {
        name: String,
        ty: String,
    },
    CollidingCustomProp {
        name: String,
    },
}

impl std::fmt::Display for ManifestIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeprecatedIgnored { field, removed_in } => {
                write!(
                    f,
                    "manifest: {field} is deprecated and ignored (removed in {removed_in})"
                )
            }
            Self::DeprecatedReplacement { field, replacement } => {
                write!(f, "manifest: {field:?} is deprecated, use {replacement:?}")
            }
            Self::UnknownField { field } => write!(f, "manifest: unknown field {field:?}"),
            Self::EmptyPython => write!(f, "manifest: project.python must not be empty"),
            Self::InvalidWindowDimensions { width, height } => write!(
                f,
                "manifest: window is {width}x{height}, dimensions must be positive"
            ),
            Self::InvalidIconPath { icon } => write!(
                f,
                "manifest: icon {icon:?} must be res:// or empty (absolute paths are forbidden)"
            ),
            Self::UnknownCustomPropType { name, ty } => write!(
                f,
                "manifest: [props] {name:?} has unknown type {ty:?} (want one of: {})",
                pite_core::CUSTOM_PROP_TYPES.join(", ")
            ),
            Self::CollidingCustomProp { name } => write!(
                f,
                "manifest: [props] {name:?} collides with a schema prop name; pick a different key"
            ),
        }
    }
}

/// Pure manifest validation returning structured issues `(errors, warnings)`.
pub fn validate_manifest_structured(
    manifest: &PiteManifest,
) -> (Vec<ManifestIssue>, Vec<ManifestIssue>) {
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
        if field == "export.platforms" {
            warnings.push(ManifestIssue::DeprecatedIgnored {
                field,
                removed_in: "0.1.1",
            });
            continue;
        }
        match deprecated_replacement(&field) {
            Some(replacement) => {
                warnings.push(ManifestIssue::DeprecatedReplacement { field, replacement })
            }
            None => warnings.push(ManifestIssue::UnknownField { field }),
        }
    }
    if manifest.project.python.is_empty() {
        errors.push(ManifestIssue::EmptyPython);
    }
    if manifest.project.window_width == 0 || manifest.project.window_height == 0 {
        errors.push(ManifestIssue::InvalidWindowDimensions {
            width: manifest.project.window_width,
            height: manifest.project.window_height,
        });
    }
    if !manifest.project.icon.is_empty() && !manifest.project.icon.starts_with("res://") {
        errors.push(ManifestIssue::InvalidIconPath {
            icon: manifest.project.icon.clone(),
        });
    }
    // `[props]` type names must be recognized by `pite_core`, and no entry
    // may collide with a schema prop key (that would let `set_custom_property`
    // succeed on a `Node` but fail on a `Node2D` — confusing).
    let schema_keys: std::collections::HashSet<&str> = pite_core::NODE_SCHEMAS
        .iter()
        .flat_map(|s| s.props.iter().map(|p| p.key))
        .collect();
    let mut props_names: Vec<&String> = manifest.props.keys().collect();
    props_names.sort();
    for name in props_names {
        let ty = &manifest.props[name];
        if !pite_core::is_valid_custom_prop_type(ty) {
            errors.push(ManifestIssue::UnknownCustomPropType {
                name: name.clone(),
                ty: ty.clone(),
            });
        }
        if schema_keys.contains(name.as_str()) {
            errors.push(ManifestIssue::CollidingCustomProp { name: name.clone() });
        }
    }
    (errors, warnings)
}

/// Pure manifest validation: unknown/deprecated keys warn (sorted, stable),
/// impossible values error. Filesystem checks (icon exists) live in `pite check`.
pub fn validate_manifest(manifest: &PiteManifest) -> (Vec<String>, Vec<String>) {
    let (errors, warnings) = validate_manifest_structured(manifest);
    (
        errors.into_iter().map(|i| i.to_string()).collect(),
        warnings.into_iter().map(|i| i.to_string()).collect(),
    )
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
    pite_core::resolve_res(project_dir, res_path).ok()
}

pub fn to_res_path(project_dir: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(project_dir)
        .ok()
        .map(|rel| format!("res://{}", rel.to_string_lossy().replace('\\', "/")))
}

pub const TEMPLATE_PITE_TOML: &str = r#"[project]
name = "TEMPLATE_NAME"
pite_version = "0.4.0"
main_scene = "res://scenes/main.ptscn"
python = "3.12"
author = ""
icon = ""
window_width = 1280
window_height = 720
"#;

const TEMPLATE_MINIMAL_2D_ZIP: &[u8] = include_bytes!("../templates/minimal-2d.zip");
const TEMPLATE_GEM_COLLECTOR_ZIP: &[u8] = include_bytes!("../templates/gem-collector.zip");

fn extract_zip_template(zip_bytes: &[u8], dest: &Path, project_name: &str) -> Result<()> {
    let reader = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(reader)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let Some(enclosed) = file.enclosed_name() else {
            continue;
        };
        let rel_path = enclosed.to_path_buf();
        let rel_str = rel_path.to_string_lossy();
        if rel_str.starts_with('.') || rel_str.contains("/.") || rel_str.contains("_cache") {
            continue;
        }
        let outpath = dest.join(&rel_path);
        if file.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|source| ProjectError::Io {
                path: outpath,
                source,
            })?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|source| ProjectError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            if rel_path.as_os_str() == MANIFEST_FILE {
                let mut contents = String::new();
                std::io::Read::read_to_string(&mut file, &mut contents).map_err(|source| {
                    ProjectError::Io {
                        path: outpath.clone(),
                        source,
                    }
                })?;
                let customized = if let Some(first_line) =
                    contents.lines().find(|l| l.trim().starts_with("name = "))
                {
                    contents.replacen(first_line, &format!("name = \"{project_name}\""), 1)
                } else {
                    contents
                };
                std::fs::write(&outpath, customized).map_err(|source| ProjectError::Io {
                    path: outpath,
                    source,
                })?;
            } else {
                let mut outfile =
                    std::fs::File::create(&outpath).map_err(|source| ProjectError::Io {
                        path: outpath.clone(),
                        source,
                    })?;
                std::io::copy(&mut file, &mut outfile).map_err(|source| ProjectError::Io {
                    path: outpath,
                    source,
                })?;
            }
        }
    }
    Ok(())
}

/// Minimal blank `main.ptscn` for projects created without a template.
pub fn blank_scene() -> String {
    format!(
        "format_version = {}\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n",
        pite_scene::FORMAT_VERSION
    )
}

pub fn create_project(dest: &Path, name: &str, template: Option<&str>) -> Result<PathBuf> {
    if let Some(t) = template {
        if t != "minimal-2d" && t != "gem-collector" {
            return Err(ProjectError::UnknownTemplate {
                name: t.to_string(),
            });
        }
    }
    let dir = if dest.file_name().map(|n| n == name).unwrap_or(false) {
        dest.to_path_buf()
    } else {
        dest.join(name)
    };
    if dir.exists() {
        return Err(ProjectError::AlreadyExists { path: dir });
    }
    match template {
        Some("gem-collector") => {
            extract_zip_template(TEMPLATE_GEM_COLLECTOR_ZIP, &dir, name)?;
        }
        Some("minimal-2d") => {
            extract_zip_template(TEMPLATE_MINIMAL_2D_ZIP, &dir, name)?;
        }
        None => {
            for sub in ["scenes", "scripts", "assets"] {
                let sub_dir = dir.join(sub);
                std::fs::create_dir_all(&sub_dir).map_err(|source| ProjectError::Io {
                    path: sub_dir,
                    source,
                })?;
            }
            let manifest_path = dir.join(MANIFEST_FILE);
            std::fs::write(
                &manifest_path,
                TEMPLATE_PITE_TOML.replace("TEMPLATE_NAME", name),
            )
            .map_err(|source| ProjectError::Io {
                path: manifest_path,
                source,
            })?;
            let scene_path = dir.join("scenes").join("main.ptscn");
            std::fs::write(&scene_path, blank_scene()).map_err(|source| ProjectError::Io {
                path: scene_path,
                source,
            })?;
        }
        Some(_) => unreachable!(),
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmpdir(tag: &str) -> PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("pite-manifest-{tag}-{}-{n}", std::process::id()));
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
        assert_eq!(manifest.project.pite_version, "0.4.0");
        assert_eq!(manifest.project.main_scene, "res://scenes/main.ptscn");
        assert_eq!(manifest.project.python, "3.12");
        assert_eq!(manifest.project.author, "");
        assert_eq!(manifest.project.icon, "");
        assert_eq!(manifest.project.window_width, 1280);
        assert_eq!(manifest.project.window_height, 720);
        assert!(manifest.project.extra.is_empty());
        assert!(manifest.vendor.is_empty());
        let (errors, warnings) = validate_manifest(&manifest);
        assert!(errors.is_empty(), "got {errors:?}");
        assert!(warnings.is_empty(), "got {warnings:?}");
    }

    #[test]
    fn vendor_table_round_trips() {
        let dir = tmpdir("vendor");
        let mut manifest = load_text("[project]\nname = \"t\"\n");
        manifest.vendor.insert(
            "requests".to_string(),
            VendorEntry {
                version: "2.32.3".to_string(),
                sha256: "abc".to_string(),
                direct: true,
            },
        );
        manifest.vendor.insert(
            "urllib3".to_string(),
            VendorEntry {
                version: "2.2.2".to_string(),
                sha256: "def".to_string(),
                direct: false,
            },
        );
        save_manifest(&dir, &manifest).unwrap();
        let back = load_manifest(&dir).unwrap();
        assert_eq!(back.vendor.len(), 2);
        assert_eq!(back.vendor["requests"].version, "2.32.3");
        assert!(back.vendor["requests"].direct);
        assert!(!back.vendor["urllib3"].direct);
        std::fs::remove_dir_all(&dir).ok();
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
        assert!(
            errors.iter().any(|e| e.contains("python")),
            "got {errors:?}"
        );
        assert!(errors.iter().any(|e| e.contains("0x")), "got {errors:?}");
        assert!(errors.iter().any(|e| e.contains("icon")), "got {errors:?}");
    }

    #[test]
    fn create_project_blank_and_template() {
        let root = tmpdir("new-blank");
        let dir = create_project(&root, "blank_game", None).unwrap();
        assert!(dir.join(MANIFEST_FILE).is_file());
        assert!(dir.join("scenes").join("main.ptscn").is_file());
        assert!(!dir.join("scripts").join("player.py").exists());
        assert!(!dir.join("assets").join("player.png").exists());
        std::fs::remove_dir_all(&root).ok();

        let root2 = tmpdir("new-tmpl");
        let dir2 = create_project(&root2, "tmpl_game", Some("minimal-2d")).unwrap();
        assert!(dir2.join(MANIFEST_FILE).is_file());
        assert!(dir2.join("scenes").join("main.ptscn").is_file());
        assert!(dir2.join("scenes").join("enemy.ptscn").is_file());
        assert!(dir2.join("scripts").join("player.py").is_file());
        assert!(dir2.join("scripts").join("enemy.py").is_file());
        assert!(dir2.join("scripts").join("hitbutton.py").is_file());
        assert!(dir2.join("assets").join("sheet.png").is_file());
        assert!(dir2.join("assets").join("sheet.ptatlas").is_file());
        assert!(dir2.join("sfx").join("hit.wav").is_file());
        std::fs::remove_dir_all(&root2).ok();

        let root3 = tmpdir("new-gem");
        let dir3 = create_project(&root3, "gem_game", Some("gem-collector")).unwrap();
        assert!(dir3.join(MANIFEST_FILE).is_file());
        assert!(dir3.join("scenes").join("main.ptscn").is_file());
        assert!(dir3.join("scripts").join("player.py").is_file());
        assert!(dir3.join("scripts").join("gem.py").is_file());
        assert!(dir3.join("scripts").join("game_timer.py").is_file());
        assert!(dir3.join("assets").join("player.png").is_file());
        assert!(dir3.join("assets").join("gem.png").is_file());
        assert!(dir3.join("sfx").join("coin.wav").is_file());
        std::fs::remove_dir_all(&root3).ok();
    }

    #[test]
    fn save_keeps_unknown_keys_and_edits() {
        let dir = tmpdir("save");
        std::fs::write(
            dir.join(MANIFEST_FILE),
            "custom = 1\n\n[project]\nname = \"x\"\nnickname = \"y\"\n",
        )
        .unwrap();
        let mut manifest = load_manifest(&dir).unwrap();
        manifest.project.name = "renamed".to_string();
        save_manifest(&dir, &manifest).unwrap();
        let again = load_manifest(&dir).unwrap();
        assert_eq!(again.project.name, "renamed");
        assert_eq!(
            again.project.extra.get("nickname"),
            Some(&toml::Value::String("y".to_string()))
        );
        assert_eq!(again.extra.get("custom"), Some(&toml::Value::Integer(1)));
        let (errors, _) = validate_manifest(&again);
        assert!(errors.is_empty(), "got {errors:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_is_byte_stable() {
        let dir = tmpdir("save-stable");
        std::fs::write(
            dir.join(MANIFEST_FILE),
            "zeta = 1\nalpha = 2\n\n[project]\nname = \"x\"\n",
        )
        .unwrap();
        let manifest = load_manifest(&dir).unwrap();
        save_manifest(&dir, &manifest).unwrap();
        let first = std::fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
        let reloaded = load_manifest(&dir).unwrap();
        save_manifest(&dir, &reloaded).unwrap();
        let second = std::fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
        assert_eq!(first, second, "save churned:\n{first}\n---\n{second}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn custom_props_round_trip_and_validate() {
        let manifest = load_text(
            "[project]\nname = \"x\"\n\n[props]\nspawn_group = \"string\"\nboss_hp = \"int\"\ntint = \"color\"\n",
        );
        assert_eq!(manifest.props.len(), 3);
        assert_eq!(manifest.props["spawn_group"], "string");
        let (errors, warnings) = validate_manifest(&manifest);
        assert!(errors.is_empty(), "got {errors:?}");
        assert!(warnings.is_empty(), "got {warnings:?}");

        let bad = load_text("[props]\nfoo = \"bytes\"\nposition = \"number\"\n");
        let (errors, _) = validate_manifest(&bad);
        assert!(
            errors.iter().any(|e| e.contains("unknown type \"bytes\"")),
            "got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("collides with a schema prop")),
            "got {errors:?}"
        );
    }
}
