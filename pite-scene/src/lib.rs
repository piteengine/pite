//! `pite-scene`: TOML schema, instantiation, validation, migration.
//!
//! Depends on `pite-core` only. TOML is canonical; binary is a later
//! export cache, not a source format.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use pite_core::{NodeDesc, NodeId, NodeTree, Props, PropValue, ScriptRef};
use serde::{Deserialize, Serialize};

/// Current scene format version. Bump = migrate or error loudly, never silent.
pub const FORMAT_VERSION: u32 = 1;

/// Canonical `.pitescene` document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneDoc {
    pub format_version: u32,
    pub root: String,
    #[serde(default)]
    pub node: Vec<SceneNode>,
}

/// One flat node entry. `props` preserves unknown keys (forward compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneNode {
    pub id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    pub parent: Option<String>,
    #[serde(default)]
    pub props: HashMap<String, toml::Value>,
    pub script: Option<SceneScript>,
}

/// Attached script reference (project-relative path + class).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneScript {
    pub path: String,
    pub class: String,
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
    let ids: std::collections::HashSet<&str> =
        doc.node.iter().map(|n| n.id.as_str()).collect();
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

/// Convert a scene document into a runtime [`NodeTree`].
/// Missing scripts are placeholders here, never a load failure.
pub fn to_node_tree(doc: &SceneDoc) -> Result<NodeTree> {
    let mut tree = NodeTree::new();
    for n in &doc.node {
        let mut props = Props::default();
        for (k, v) in &n.props {
            let pv = match v {
                toml::Value::String(s) => PropValue::Str(s.clone()),
                toml::Value::Integer(i) => PropValue::Int(*i),
                toml::Value::Float(f) => PropValue::Num(*f),
                toml::Value::Boolean(b) => PropValue::Bool(*b),
                _ => PropValue::Str(v.to_string()),
            };
            props.insert(k.clone(), pv);
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

/// Instantiation seam: load referenced file, clone subtree, apply prop
/// overrides. Full implementation is M1a; M0 only reserves the shape.
pub fn instantiate(_scene: &str, _overrides: &HashMap<String, toml::Value>) -> Result<NodeTree> {
    Ok(NodeTree::new())
}
