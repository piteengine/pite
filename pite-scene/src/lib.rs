// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-scene`: TOML schema, instantiation, validation, migration.
//!
//! Depends on `pite-core` only. TOML is canonical; binary is a later
//! export cache, not a source format.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use pite_core::{NodeDesc, NodeId, NodeTree, PropValue, Props, ScriptRef};
use serde::{Deserialize, Serialize};

pub mod cache;

pub use cache::{CacheStatus, CACHE_VERSION};

/// Current scene format version. Bump = migrate or error loudly, never silent.
pub const FORMAT_VERSION: u32 = 1;

/// Canonical `.pitescene` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneDoc {
    pub format_version: u32,
    pub root: String,
    #[serde(default)]
    pub node: Vec<SceneNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instance: Vec<SceneInstance>,
}

/// One flat node entry. `props` preserves unknown keys (forward compat).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneNode {
    pub id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, toml::Value>,
    pub script: Option<SceneScript>,
}

/// Attached script reference (project-relative path + class).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneScript {
    pub path: String,
    pub class: String,
}

/// An instantiation entry: load the referenced file, clone its subtree
/// under `parent`, remap ids with `prefix`, apply prop overrides.
/// Overrides are keyed `"node_id.prop"`; unknown targets fail loudly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneInstance {
    pub scene: String,
    pub parent: Option<String>,
    #[serde(default)]
    pub prefix: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, toml::Value>,
}

/// Format migration hook (§12). New fields are additive; breaking
/// change = version bump, never silent reinterpretation.
pub fn migrate(from: u32, _doc: &toml::Value) -> Result<()> {
    if from == FORMAT_VERSION {
        return Ok(());
    }
    anyhow::bail!("unsupported scene format_version {from} (engine supports {FORMAT_VERSION})");
}

pub fn parse_scene_str(text: &str) -> Result<SceneDoc> {
    let raw: toml::Value = toml::from_str(text).context("scene is not valid TOML")?;
    let version = raw
        .get("format_version")
        .and_then(toml::Value::as_integer)
        .unwrap_or(0);
    migrate(version as u32, &raw)?;
    let doc: SceneDoc = raw.try_into().context("scene does not match schema")?;
    Ok(doc)
}

pub fn load_scene(path: &Path) -> Result<SceneDoc> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read scene {}", path.display()))?;
    parse_scene_str(&text).with_context(|| format!("cannot parse scene {}", path.display()))
}

/// Load with the binary cache: a valid cache wins; a missing cache parses
/// TOML and writes the cache; a corrupt or version-mismatched cache parses
/// TOML, rewrites the cache, and reports the fallback loudly via the
/// returned warning — never silently. TOML stays source of truth.
pub fn load_cached(path: &Path) -> Result<(SceneDoc, CacheStatus, Option<String>)> {
    let bytes =
        std::fs::read(path).with_context(|| format!("cannot read scene {}", path.display()))?;
    let source_hash = cache::fnv1a_u64(&bytes);
    let source_hex = format!("{source_hash:016x}");
    let cache_path = cache::cache_path_for(path, &source_hex);

    let mut rebuilt_reason: Option<String> = None;
    if cache_path.is_file() {
        match std::fs::read(&cache_path) {
            Ok(cached) => match cache::decode(&cached) {
                Ok((doc, header))
                    if header.source_hash == source_hash
                        && header.source_len == bytes.len() as u64 =>
                {
                    cache::prune_stale(path, &source_hex);
                    return Ok((doc, CacheStatus::Hit, None));
                }
                Ok(_) => {
                    rebuilt_reason = Some(format!(
                        "scene cache {} does not match {}",
                        cache_path.display(),
                        path.display()
                    ));
                }
                Err(e) => {
                    rebuilt_reason = Some(format!(
                        "scene cache {} ignored ({e:#}); rebuilding from TOML",
                        cache_path.display()
                    ));
                }
            },
            Err(e) => {
                rebuilt_reason = Some(format!(
                    "scene cache {} unreadable ({e:#}); rebuilding from TOML",
                    cache_path.display()
                ));
            }
        }
    }

    let text = std::str::from_utf8(&bytes)
        .with_context(|| format!("cannot parse scene {}", path.display()))?;
    let doc =
        parse_scene_str(text).with_context(|| format!("cannot parse scene {}", path.display()))?;

    let payload = cache::encode(&doc, source_hash, bytes.len() as u64);
    let mut warning = rebuilt_reason.clone();
    if let Some(dir) = cache_path.parent() {
        if let Err(e) =
            std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&cache_path, &payload))
        {
            let write_warn = format!("cannot write scene cache {} ({e:#})", cache_path.display());
            warning = Some(match warning {
                Some(w) => format!("{w}; {write_warn}"),
                None => write_warn,
            });
        } else {
            cache::prune_stale(path, &source_hex);
        }
    }

    let status = match rebuilt_reason {
        Some(reason) => CacheStatus::Rebuilt(reason),
        None => CacheStatus::Miss,
    };
    Ok((doc, status, warning))
}

/// Preferred entry point: [`load_cached`] with the fallback warning
/// surfaced loudly on stderr. Corruption anywhere falls back to TOML,
/// never silently.
pub fn load_scene_cached(path: &Path) -> Result<SceneDoc> {
    let (doc, _, warning) = load_cached(path)?;
    if let Some(w) = warning {
        tracing::warn!("{w}");
    }
    Ok(doc)
}

pub fn save_scene(doc: &SceneDoc) -> Result<String> {
    toml::to_string(doc).context("cannot serialize scene")
}

/// Structural validation. Missing script = placeholder, never a load
/// failure; unknown props warn and are preserved on save.
pub fn validate(doc: &SceneDoc) -> Vec<String> {
    let mut issues = Vec::new();
    if doc.format_version != FORMAT_VERSION {
        issues.push(format!(
            "format_version {} != supported {FORMAT_VERSION}",
            doc.format_version
        ));
    }
    let ids: std::collections::HashSet<&str> = doc.node.iter().map(|n| n.id.as_str()).collect();
    if !ids.contains(doc.root.as_str()) {
        issues.push(format!("root {:?} not found in node list", doc.root));
    }
    for n in &doc.node {
        if let Some(parent) = &n.parent {
            if !ids.contains(parent.as_str()) {
                issues.push(format!("node {:?} has unknown parent {parent:?}", n.id));
            }
        }
    }
    issues
}

fn toml_to_prop(value: &toml::Value) -> PropValue {
    match value {
        toml::Value::String(s) => PropValue::Str(s.clone()),
        toml::Value::Integer(i) => PropValue::Int(*i),
        toml::Value::Float(f) => PropValue::Num(*f),
        toml::Value::Boolean(b) => PropValue::Bool(*b),
        toml::Value::Array(items) => {
            let nums: Vec<f64> = items
                .iter()
                .map(|v| match v {
                    toml::Value::Integer(i) => *i as f64,
                    toml::Value::Float(f) => *f,
                    _ => f64::NAN,
                })
                .collect();
            if nums.len() == 2 && nums.iter().all(|n| !n.is_nan()) {
                PropValue::Vec2(nums[0], nums[1])
            } else {
                PropValue::Str(value.to_string())
            }
        }
        _ => PropValue::Str(value.to_string()),
    }
}

/// Convert a scene document into a runtime [`NodeTree`].
/// Missing scripts are placeholders here, never a load failure.
pub fn to_node_tree(doc: &SceneDoc) -> Result<NodeTree> {
    let mut tree = NodeTree::new();
    for n in &doc.node {
        let mut props = Props::default();
        for (k, v) in &n.props {
            props.insert(k.clone(), toml_to_prop(v));
        }
        tree.insert(NodeDesc {
            id: NodeId::from(n.id.clone()),
            type_name: n.type_name.clone(),
            name: n.name.clone(),
            parent: n.parent.clone().map(NodeId::from),
            props,
            script: n.script.as_ref().map(|s| ScriptRef {
                path: s.path.clone(),
                class_name: s.class.clone(),
            }),
        })
        .map_err(|e| anyhow::anyhow!("node {:?}: {e}", n.id))?;
    }
    Ok(tree)
}

/// Load the referenced scene, clone its subtree, apply prop overrides.
/// Returns node records ready to insert into the host tree plus the
/// remapped id of the referenced scene's root.
pub fn instantiate(
    scene_path: &Path,
    prefix: &str,
    overrides: &BTreeMap<String, toml::Value>,
) -> Result<(Vec<NodeDesc>, String)> {
    let doc = load_scene_cached(scene_path)?;
    let mut descs = Vec::with_capacity(doc.node.len());
    for n in &doc.node {
        let mut props = Props::default();
        for (k, v) in &n.props {
            props.insert(k.clone(), toml_to_prop(v));
        }
        descs.push(NodeDesc {
            id: NodeId::from(format!("{prefix}{}", n.id)),
            type_name: n.type_name.clone(),
            name: n.name.clone(),
            parent: n
                .parent
                .clone()
                .map(|p| NodeId::from(format!("{prefix}{p}"))),
            props,
            script: n.script.as_ref().map(|s| ScriptRef {
                path: s.path.clone(),
                class_name: s.class.clone(),
            }),
        });
    }
    for (key, value) in overrides {
        let (node_id, prop) = key.split_once('.').with_context(|| {
            format!(
                "override {key:?} must be \"node_id.prop\" in {}",
                scene_path.display()
            )
        })?;
        let target = format!("{prefix}{node_id}");
        let desc = descs
            .iter_mut()
            .find(|d| d.id.as_str() == target)
            .with_context(|| {
                format!(
                    "override target {target:?} not found in {}",
                    scene_path.display()
                )
            })?;
        desc.props.insert(prop.to_string(), toml_to_prop(value));
    }
    Ok((descs, format!("{prefix}{}", doc.root)))
}

fn resolve_scene_ref(
    scene_ref: &str,
    scene_dir: &Path,
    project_dir: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(rel) = scene_ref.strip_prefix("res://") {
        let root = project_dir.with_context(|| {
            format!("scene {scene_ref:?} uses res:// but no project root is known")
        })?;
        Ok(root.join(rel))
    } else {
        let p = Path::new(scene_ref);
        if p.is_absolute() {
            Ok(p.to_path_buf())
        } else {
            Ok(scene_dir.join(p))
        }
    }
}

/// Build the full runtime tree: host nodes plus every `[[instance]]`
/// subtree (loaded, id-remapped, overrides applied).
pub fn build_tree(
    doc: &SceneDoc,
    scene_dir: &Path,
    project_dir: Option<&Path>,
) -> Result<NodeTree> {
    let mut tree = to_node_tree(doc)?;
    let host_ids: std::collections::HashSet<&str> =
        doc.node.iter().map(|n| n.id.as_str()).collect();
    for inst in &doc.instance {
        let ref_path = resolve_scene_ref(&inst.scene, scene_dir, project_dir)?;
        let (mut descs, subtree_root) = instantiate(&ref_path, &inst.prefix, &inst.overrides)?;
        let attach_at = inst.parent.clone().unwrap_or_else(|| doc.root.clone());
        if !host_ids.contains(attach_at.as_str())
            && tree.get(&NodeId::from(attach_at.clone())).is_none()
        {
            return Err(pite_core::CoreError::MissingParent {
                child: format!("instance of {}", inst.scene),
                parent: attach_at,
            }
            .into());
        }
        for desc in descs.drain(..) {
            let mut desc = desc;
            if desc.id.as_str() == subtree_root {
                desc.parent = Some(NodeId::from(attach_at.clone()));
            }
            tree.insert(desc.clone())
                .map_err(|e| anyhow::anyhow!("instance of {}: {e}", inst.scene))?;
        }
    }
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = r#"
format_version = 1
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
position = [100.0, 200.0]
mystery = "kept"

[node.script]
path = "res://scripts/player.py"
class = "Player"
"#;

    #[test]
    fn round_trip_preserves_unknown_props() {
        let doc = parse_scene_str(HOST).unwrap();
        let saved = save_scene(&doc).unwrap();
        let again = parse_scene_str(&saved).unwrap();
        assert_eq!(doc, again);
        let player = again.node.iter().find(|n| n.id == "player").unwrap();
        assert_eq!(
            player.props.get("mystery"),
            Some(&toml::Value::String("kept".to_string()))
        );
    }

    #[test]
    fn version_bump_errors_loudly() {
        let err = parse_scene_str("format_version = 99\nroot = \"root\"\n").unwrap_err();
        assert!(err.to_string().contains("unsupported scene format_version"));
    }

    fn write_fixture(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn instantiate_clones_subtree_and_applies_overrides() {
        let dir = std::env::temp_dir().join(format!("pite-inst-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ref_path = write_fixture(
            &dir,
            "enemy.pitescene",
            r#"
format_version = 1
root = "enemy"

[[node]]
id = "enemy"
type = "Node2D"
name = "Enemy"

[[node]]
id = "sprite"
type = "Sprite2D"
name = "Sprite"
parent = "enemy"

[node.props]
position = [0.0, 0.0]
"#,
        );
        let mut overrides = BTreeMap::new();
        overrides.insert(
            "sprite.position".to_string(),
            toml::Value::Array(vec![toml::Value::Float(10.0), toml::Value::Float(20.0)]),
        );
        let (descs, root) = instantiate(&ref_path, "e1_", &overrides).unwrap();
        assert_eq!(root, "e1_enemy");
        assert_eq!(descs.len(), 2);
        let sprite = descs.iter().find(|d| d.id.as_str() == "e1_sprite").unwrap();
        assert_eq!(sprite.parent, Some(NodeId::from("e1_enemy".to_string())));
        assert_eq!(
            sprite.props.get("position"),
            Some(&PropValue::Vec2(10.0, 20.0))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn instantiate_unknown_override_target_fails() {
        let dir = std::env::temp_dir().join(format!("pite-inst-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ref_path = write_fixture(
            &dir,
            "tiny.pitescene",
            "format_version = 1\nroot = \"solo\"\n\n[[node]]\nid = \"solo\"\ntype = \"Node\"\nname = \"Solo\"\n",
        );
        let mut overrides = BTreeMap::new();
        overrides.insert("ghost.x".to_string(), toml::Value::Integer(1));
        let err = instantiate(&ref_path, "", &overrides).unwrap_err();
        assert!(err.to_string().contains("override target"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
