// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plugin manifest (`plugin.toml`), `[plugins]` entries in `pite.toml`, and
//! declarative schema loading for Pite project plugins.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use pite_core::{
    NodeSchema, PropDef, PropHintSpec, PropValue, SchemaError, SchemaRegistry, SignalSchema,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::{ProjectError, Result};
use crate::PiteManifest;

/// Standard plugin manifest filename located inside `res://plugins/<id>/`.
pub const PLUGIN_MANIFEST_FILE: &str = "plugin.toml";

/// Directory name under the project root where plugins live.
pub const PLUGINS_DIR: &str = "plugins";

const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// Entry under `[plugins]` in `pite.toml`.
///
/// Supports both concise booleans (`dialogue = true`) and detailed tables
/// (`[plugins.dialogue] enabled = false, source = "https://..."`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginEntry {
    Enabled(bool),
    Detailed {
        #[serde(default)]
        enabled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rev: Option<String>,
    },
}

impl PluginEntry {
    /// Returns `true` if this plugin is enabled.
    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Enabled(b) => *b,
            Self::Detailed { enabled, .. } => *enabled,
        }
    }

    /// Updates the enabled flag while preserving `source` and `rev` metadata if present.
    pub fn set_enabled(&mut self, enabled_val: bool) {
        match self {
            Self::Enabled(b) => *b = enabled_val,
            Self::Detailed { enabled, .. } => *enabled = enabled_val,
        }
    }

    /// Returns the recorded install source, if any.
    pub fn source(&self) -> Option<&str> {
        match self {
            Self::Enabled(_) => None,
            Self::Detailed { source, .. } => source.as_deref(),
        }
    }

    /// Returns the recorded git revision/tag/branch, if any.
    pub fn rev(&self) -> Option<&str> {
        match self {
            Self::Enabled(_) => None,
            Self::Detailed { rev, .. } => rev.as_deref(),
        }
    }
}

fn default_plugin_version() -> String {
    "0.1.0".to_string()
}

/// Complete manifest document parsed from `res://plugins/<id>/plugin.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_plugin_version")]
    pub version: String,
    #[serde(default)]
    pub pite_version: Option<String>,
    #[serde(default)]
    pub description: String,
    /// Runtime plugin entrypoint (`module:Class`), executed in both editor Play and `pite-player`.
    #[serde(default)]
    pub runtime: Option<String>,
    /// Editor-only plugin entrypoint (`module:Class`), executed only in trusted `pite-editor` workspaces.
    #[serde(default)]
    pub editor: Option<String>,
    /// Autoload singletons (`Name = "module:Class"`) managed by `PluginHost`.
    #[serde(default)]
    pub autoloads: BTreeMap<String, String>,
    /// Custom node types contributed by this plugin.
    #[serde(default)]
    pub nodes: Vec<PluginNodeDef>,
    /// Project-wide custom properties contributed by this plugin.
    #[serde(default)]
    pub props: HashMap<String, String>,
    /// Third-party Python package dependencies (`pkg = "==ver"`), verified against `vendor/`.
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

/// Custom node type declared in `[[nodes]]` inside `plugin.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginNodeDef {
    pub type_name: String,
    pub inherits: String,
    #[serde(default)]
    pub description: String,
    /// Optional default Python class (`module:Class`, relative to the plugin package).
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub props: Vec<PluginNodePropDef>,
    #[serde(default)]
    pub signals: Vec<PluginSignalDef>,
}

/// Property declaration inside `[[nodes.props]]` in `plugin.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginNodePropDef {
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub doc: String,
    #[serde(rename = "type")]
    pub prop_type: String,
    #[serde(default)]
    pub default: Option<toml::Value>,
    #[serde(default)]
    pub enum_values: Vec<String>,
    #[serde(default)]
    pub asset_filter: Vec<String>,
    #[serde(default)]
    pub read_only: bool,
}

/// Signal declaration inside `[[nodes.signals]]` in `plugin.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSignalDef {
    pub name: String,
    #[serde(default)]
    pub payload: Vec<String>,
}

/// Errors encountered while loading or validating an enabled plugin.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PluginLoadError {
    #[error("plugin id `{id}` must be a valid snake_case Python identifier")]
    InvalidId { id: String },

    #[error("plugin `{id}` manifest missing or unreadable at {path}: {reason}")]
    ManifestIo {
        id: String,
        path: PathBuf,
        reason: String,
    },

    #[error("plugin `{id}` manifest has invalid TOML at {path}: {reason}")]
    ManifestParse {
        id: String,
        path: PathBuf,
        reason: String,
    },

    #[error("plugin `{id}` manifest declares mismatched id `{declared}`")]
    IdMismatch { id: String, declared: String },

    #[error("plugin `{id}` requires pite_version `{required}`, but engine is `{engine}`")]
    IncompatibleEngineVersion {
        id: String,
        required: String,
        engine: String,
    },

    #[error("plugin `{id}` node `{type_name}` prop `{key}`: {reason}")]
    InvalidNodeProp {
        id: String,
        type_name: String,
        key: String,
        reason: String,
    },

    #[error("plugin `{id}` node schema error: {source}")]
    Schema {
        id: String,
        #[source]
        source: SchemaError,
    },

    #[error("plugin `{id}` autoload `{name}` collides with plugin `{existing_owner}`")]
    DuplicateAutoload {
        id: String,
        name: String,
        existing_owner: String,
    },

    #[error("plugin `{id}` custom prop `{key}` collides with `{existing_owner}`")]
    DuplicateCustomProp {
        id: String,
        key: String,
        existing_owner: String,
    },
}

/// A validated, enabled plugin ready for runtime/editor/check/export consumption.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub id: String,
    pub dir: PathBuf,
    pub manifest: PluginManifest,
}

/// Result of loading all enabled plugins for a project.
#[derive(Debug, Clone)]
pub struct LoadedPlugins {
    pub plugins: Vec<LoadedPlugin>,
    pub schemas: SchemaRegistry,
    pub custom_props: HashMap<String, String>,
    pub errors: Vec<PluginLoadError>,
}

/// Validates that `id` is a `snake_case` Python identifier not colliding with `pite`.
pub fn is_valid_plugin_id(id: &str) -> bool {
    if id.is_empty() || id == "pite" || PYTHON_KEYWORDS.contains(&id) {
        return false;
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Returns the filesystem directory for plugin `id` (`<project_dir>/plugins/<id>`).
pub fn plugin_dir(project_dir: &Path, id: &str) -> PathBuf {
    project_dir.join(PLUGINS_DIR).join(id)
}

/// Checks whether `required` (`"0.4"` or `"0.4.0"`) is compatible with `engine` (`"0.4.0"`).
///
/// For `0.x` releases, major and minor must match, and engine patch must be `>=` required patch.
pub fn is_pite_version_compatible(required: &str, engine: &str) -> bool {
    let parse = |s: &str| -> Option<(u64, u64, u64)> {
        let trimmed = s
            .trim()
            .trim_start_matches(">=")
            .trim_start_matches('^')
            .trim()
            .trim_start_matches('v');
        let mut parts = trimmed.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some((major, minor, patch))
    };
    let (Some((rmaj, rmin, rpat)), Some((emaj, emin, epat))) = (parse(required), parse(engine))
    else {
        return false;
    };
    if rmaj == 0 {
        emaj == 0 && emin == rmin && epat >= rpat
    } else {
        emaj == rmaj && (emin > rmin || (emin == rmin && epat >= rpat))
    }
}

/// Loads and parses `plugin.toml` from `dir`.
///
/// # Errors
/// Returns [`ProjectError::Io`] or [`ProjectError::TomlParse`] on failure.
pub fn load_plugin_manifest(dir: &Path) -> Result<PluginManifest> {
    let path = dir.join(PLUGIN_MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|source| ProjectError::Io {
        path: path.clone(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| ProjectError::TomlParse { path, source })
}

/// Writes `manifest` as pretty-printed `plugin.toml` inside `dir`.
///
/// # Errors
/// Returns [`ProjectError::Io`] or [`ProjectError::TomlEncode`] on failure.
pub fn save_plugin_manifest(dir: &Path, manifest: &PluginManifest) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|source| ProjectError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let path = dir.join(PLUGIN_MANIFEST_FILE);
    let mut text = toml::to_string_pretty(manifest)?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    std::fs::write(&path, text).map_err(|source| ProjectError::Io { path, source })
}

fn prop_def_from_plugin(
    plugin_id: &str,
    type_name: &str,
    raw: &PluginNodePropDef,
) -> std::result::Result<PropDef, PluginLoadError> {
    let err = |reason: String| PluginLoadError::InvalidNodeProp {
        id: plugin_id.to_string(),
        type_name: type_name.to_string(),
        key: raw.key.clone(),
        reason,
    };
    if raw.key.is_empty() {
        return Err(err("property key must not be empty".to_string()));
    }
    let (default_value, hint) = match raw.prop_type.as_str() {
        "string" | "str" => {
            let def = match &raw.default {
                None => PropValue::Str(String::new()),
                Some(toml::Value::String(s)) => PropValue::Str(s.clone()),
                Some(other) => {
                    return Err(err(format!("expected string default, got {other}")));
                }
            };
            let hint = if !raw.enum_values.is_empty() {
                PropHintSpec::Enum(raw.enum_values.iter().cloned().map(Cow::Owned).collect())
            } else if !raw.asset_filter.is_empty() {
                PropHintSpec::Asset {
                    filter: raw.asset_filter.iter().cloned().map(Cow::Owned).collect(),
                }
            } else {
                PropHintSpec::Text
            };
            (def, hint)
        }
        "int" | "integer" => {
            let def = match &raw.default {
                None => PropValue::Int(0),
                Some(toml::Value::Integer(i)) => PropValue::Int(*i),
                Some(other) => {
                    return Err(err(format!("expected integer default, got {other}")));
                }
            };
            (
                def,
                PropHintSpec::Int {
                    speed: 1.0,
                    min: i64::MIN,
                    max: i64::MAX,
                },
            )
        }
        "number" | "num" | "float" => {
            let def = match &raw.default {
                None => PropValue::Num(0.0),
                Some(toml::Value::Float(f)) => PropValue::Num(*f),
                Some(toml::Value::Integer(i)) => PropValue::Num(*i as f64),
                Some(other) => {
                    return Err(err(format!("expected number default, got {other}")));
                }
            };
            (
                def,
                PropHintSpec::Num {
                    speed: 0.1,
                    min: f64::NEG_INFINITY,
                    max: f64::INFINITY,
                },
            )
        }
        "bool" | "boolean" => {
            let def = match &raw.default {
                None => PropValue::Bool(false),
                Some(toml::Value::Boolean(b)) => PropValue::Bool(*b),
                Some(other) => {
                    return Err(err(format!("expected boolean default, got {other}")));
                }
            };
            (def, PropHintSpec::Bool)
        }
        "vec2" => {
            let def = match &raw.default {
                None => PropValue::Vec2(0.0, 0.0),
                Some(v) => match pite_scene::toml_to_prop(v) {
                    PropValue::Vec2(x, y) => PropValue::Vec2(x, y),
                    _ => return Err(err(format!("expected [x, y] vec2 default, got {v}"))),
                },
            };
            (def, PropHintSpec::Vec2 { speed: 1.0 })
        }
        "color" => {
            let def = match &raw.default {
                None => PropValue::Color(255, 255, 255, 255),
                Some(v) => match pite_scene::toml_to_prop(v) {
                    PropValue::Color(r, g, b, a) => PropValue::Color(r, g, b, a),
                    _ => return Err(err(format!("expected hex color default, got {v}"))),
                },
            };
            (def, PropHintSpec::Color)
        }
        other => {
            return Err(err(format!(
                "unknown property type `{other}` (want string, int, number, bool, vec2, color)"
            )));
        }
    };

    let label = if raw.label.is_empty() {
        raw.key.clone()
    } else {
        raw.label.clone()
    };
    Ok(PropDef {
        key: Cow::Owned(raw.key.clone()),
        label: Cow::Owned(label),
        doc: Cow::Owned(raw.doc.clone()),
        default_value,
        hint,
        read_only: raw.read_only,
    })
}

/// Loads all enabled plugins declared in `manifest.plugins`, building a combined
/// [`SchemaRegistry`] and collecting any validation or collision errors.
pub fn load_enabled_plugins(project_dir: &Path, manifest: &PiteManifest) -> LoadedPlugins {
    let mut schemas = SchemaRegistry::new();
    let mut plugins = Vec::new();
    let mut errors = Vec::new();
    let mut autoload_owners: HashMap<String, String> = HashMap::new();
    let mut custom_props: HashMap<String, String> = manifest.props.clone();
    let mut prop_owners: HashMap<String, String> = manifest
        .props
        .keys()
        .map(|k| (k.clone(), "pite.toml [props]".to_string()))
        .collect();

    let engine_ver = env!("CARGO_PKG_VERSION");

    for (id, entry) in &manifest.plugins {
        if !entry.is_enabled() {
            continue;
        }
        if !is_valid_plugin_id(id) {
            errors.push(PluginLoadError::InvalidId { id: id.clone() });
            continue;
        }
        let dir = plugin_dir(project_dir, id);
        let manifest_path = dir.join(PLUGIN_MANIFEST_FILE);
        let text = match std::fs::read_to_string(&manifest_path) {
            Ok(t) => t,
            Err(e) => {
                errors.push(PluginLoadError::ManifestIo {
                    id: id.clone(),
                    path: manifest_path,
                    reason: e.to_string(),
                });
                continue;
            }
        };
        let plugin_manifest: PluginManifest = match toml::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                errors.push(PluginLoadError::ManifestParse {
                    id: id.clone(),
                    path: manifest_path,
                    reason: e.to_string(),
                });
                continue;
            }
        };
        if plugin_manifest.id != *id {
            errors.push(PluginLoadError::IdMismatch {
                id: id.clone(),
                declared: plugin_manifest.id,
            });
            continue;
        }
        if let Some(req) = &plugin_manifest.pite_version {
            if !is_pite_version_compatible(req, engine_ver) {
                errors.push(PluginLoadError::IncompatibleEngineVersion {
                    id: id.clone(),
                    required: req.clone(),
                    engine: engine_ver.to_string(),
                });
                continue;
            }
        }

        let mut plugin_failed = false;

        for autoload_name in plugin_manifest.autoloads.keys() {
            if let Some(existing_owner) = autoload_owners.get(autoload_name) {
                errors.push(PluginLoadError::DuplicateAutoload {
                    id: id.clone(),
                    name: autoload_name.clone(),
                    existing_owner: existing_owner.clone(),
                });
                plugin_failed = true;
            }
        }

        for (prop_key, prop_ty) in &plugin_manifest.props {
            if !pite_core::is_valid_custom_prop_type(prop_ty) {
                errors.push(PluginLoadError::InvalidNodeProp {
                    id: id.clone(),
                    type_name: "[props]".to_string(),
                    key: prop_key.clone(),
                    reason: format!("unknown custom prop type `{prop_ty}`"),
                });
                plugin_failed = true;
            } else if let Some(existing) = prop_owners.get(prop_key) {
                errors.push(PluginLoadError::DuplicateCustomProp {
                    id: id.clone(),
                    key: prop_key.clone(),
                    existing_owner: existing.clone(),
                });
                plugin_failed = true;
            }
        }

        let mut pending_nodes = Vec::with_capacity(plugin_manifest.nodes.len());
        for node_def in &plugin_manifest.nodes {
            let mut props = Vec::with_capacity(node_def.props.len());
            for prop in &node_def.props {
                match prop_def_from_plugin(id, &node_def.type_name, prop) {
                    Ok(def) => props.push(def),
                    Err(e) => {
                        errors.push(e);
                        plugin_failed = true;
                    }
                }
            }
            let signals = node_def
                .signals
                .iter()
                .map(|s| SignalSchema {
                    name: Cow::Owned(s.name.clone()),
                    payload: s.payload.iter().cloned().map(Cow::Owned).collect(),
                })
                .collect();
            pending_nodes.push(NodeSchema {
                type_name: Cow::Owned(node_def.type_name.clone()),
                description: Cow::Owned(if node_def.description.is_empty() {
                    format!("Plugin node from `{id}`.")
                } else {
                    node_def.description.clone()
                }),
                inherits: Some(Cow::Owned(node_def.inherits.clone())),
                props,
                signals,
                abstract_class: false,
                plugin_id: Some(id.clone()),
            });
        }

        if plugin_failed {
            continue;
        }

        let mut staged_schemas = schemas.clone();
        for node_schema in pending_nodes {
            if let Err(source) = staged_schemas.register(node_schema) {
                errors.push(PluginLoadError::Schema {
                    id: id.clone(),
                    source,
                });
                plugin_failed = true;
                break;
            }
        }
        if plugin_failed {
            continue;
        }

        schemas = staged_schemas;
        for autoload_name in plugin_manifest.autoloads.keys() {
            autoload_owners.insert(autoload_name.clone(), format!("plugin `{id}`"));
        }
        for (prop_key, prop_ty) in &plugin_manifest.props {
            prop_owners.insert(prop_key.clone(), format!("plugin `{id}`"));
            custom_props.insert(prop_key.clone(), prop_ty.clone());
        }
        plugins.push(LoadedPlugin {
            id: id.clone(),
            dir,
            manifest: plugin_manifest,
        });
    }

    LoadedPlugins {
        plugins,
        schemas,
        custom_props,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_manifest;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-plug-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn valid_plugin_ids_enforced() {
        assert!(is_valid_plugin_id("dialogue"));
        assert!(is_valid_plugin_id("tile_painter_2d"));
        assert!(!is_valid_plugin_id(""));
        assert!(!is_valid_plugin_id("pite"));
        assert!(!is_valid_plugin_id("Dialogue"));
        assert!(!is_valid_plugin_id("my-plugin"));
        assert!(!is_valid_plugin_id("2d_tools"));
        assert!(!is_valid_plugin_id("class"));
    }

    #[test]
    fn pite_version_compatibility_checks_minor_for_zero_x() {
        assert!(is_pite_version_compatible("0.4", "0.4.0"));
        assert!(is_pite_version_compatible("0.4.0", "0.4.2"));
        assert!(!is_pite_version_compatible("0.4.3", "0.4.0"));
        assert!(!is_pite_version_compatible("0.5.0", "0.4.0"));
    }

    #[test]
    fn loads_enabled_plugin_nodes_and_detects_collisions() {
        let root = tmpdir("load");
        std::fs::write(
            root.join("pite.toml"),
            "[project]\nname = \"g\"\n\n[plugins]\ndialogue = true\ndisabled_plug = false\n",
        )
        .unwrap();
        let d_dir = plugin_dir(&root, "dialogue");
        std::fs::create_dir_all(&d_dir).unwrap();
        std::fs::write(
            d_dir.join(PLUGIN_MANIFEST_FILE),
            r#"
id = "dialogue"
name = "Dialogue"
version = "0.1.0"
pite_version = "0.4"
runtime = "runtime:DialoguePlugin"
editor = "editor:DialogueEditor"

[autoloads]
Dialogue = "runtime:DialogueService"

[[nodes]]
type_name = "DialogueBox"
inherits = "Control"
description = "Typewriter dialogue box"
script = "nodes:DialogueBox"

[[nodes.props]]
key = "typing_speed"
type = "number"
default = 30.0

[[nodes.signals]]
name = "finished"
payload = ["str"]
"#,
        )
        .unwrap();

        let manifest = load_manifest(&root).unwrap();
        let loaded = load_enabled_plugins(&root, &manifest);
        assert!(loaded.errors.is_empty(), "errors: {:?}", loaded.errors);
        assert_eq!(loaded.plugins.len(), 1);
        assert!(loaded.schemas.contains("DialogueBox"));
        let props = loaded.schemas.all_props("DialogueBox");
        assert!(props.iter().any(|p| p.key == "typing_speed"));
        assert!(props.iter().any(|p| p.key == "position"));

        std::fs::remove_dir_all(&root).ok();
    }
}
