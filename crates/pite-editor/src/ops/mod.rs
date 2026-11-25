// SPDX-License-Identifier: MIT OR Apache-2.0
//! Headless editor operations: pure tree/scene/error logic with no egui.
//!
//! `app.rs` calls these from UI callbacks and routes every `Err` to the
//! console; nothing here touches the UI, so it is all unit-testable.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use pite_core::{NodeDesc, NodeId, NodeTree, NodeTypeRegistry, PropValue, Props};
use pite_scene::{prop_to_toml, SceneDoc, SceneInstance, SceneNode, SceneScript, FORMAT_VERSION};

/// Instantiable node types, excluding abstract base classes.
pub fn registered_types() -> Vec<String> {
    pite_core::NODE_SCHEMAS
        .iter()
        .copied()
        .filter(|s| !s.abstract_class)
        .map(|s| s.type_name.to_string())
        .collect()
}

fn slugify(name: &str) -> String {
    let mut slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    slug = slug.trim_matches('_').to_string();
    if slug.is_empty() {
        slug = "node".to_string();
    }
    slug
}

pub fn add_node(
    tree: &mut NodeTree,
    parent: Option<NodeId>,
    type_name: &str,
    name: &str,
) -> Result<NodeId> {
    let registry = NodeTypeRegistry::new();
    let slug = slugify(name);
    let mut candidate = slug.clone();
    let mut counter = 2;
    while tree.contains(&NodeId::from(candidate.clone())) {
        candidate = format!("{slug}_{counter}");
        counter += 1;
    }
    let id = NodeId::from(candidate);
    let mut node = registry
        .create(id.clone(), type_name)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    node.name = name.to_string();
    // Fill schema defaults (inherited included) so a freshly created node
    // already satisfies the "every schema prop present" invariant — same
    // one `to_node_tree` enforces at load time.
    for prop in pite_core::all_props(type_name) {
        node.props
            .insert(prop.key, prop.default_value.to_prop_value());
    }
    let parent = match parent {
        Some(p) => {
            if !tree.contains(&p) {
                anyhow::bail!("parent `{p}` does not exist");
            }
            Some(p)
        }
        None => tree.root().cloned(),
    };
    let desc = NodeDesc {
        id: id.clone(),
        type_name: node.type_name.clone(),
        name: node.name.clone(),
        parent,
        props: node.props.clone(),
        script: None,
    };
    tree.insert(desc).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(id)
}

/// Remove `id` and its subtree. Refusing the root is a console-grade error.
pub fn remove_node(tree: &mut NodeTree, id: &NodeId) -> Result<()> {
    if tree.root() == Some(id) {
        anyhow::bail!("cannot remove the root node `{id}`");
    }
    tree.remove(id).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

/// Reparent `id` under `new_parent`. Cycle and root errors surface as-is.
pub fn move_node(tree: &mut NodeTree, id: &NodeId, new_parent: Option<NodeId>) -> Result<()> {
    tree.reparent(id, new_parent)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

/// Place `id` under `new_parent` at child `index` (clamped to the end).
/// Unlike `move_node` this controls sibling order: index 0 makes it first.
/// Refuses unknown nodes, root moves, and cycles (under itself or a
/// descendant); the node keeps its subtree.
pub fn place_node(
    tree: &mut NodeTree,
    id: &NodeId,
    new_parent: &NodeId,
    index: usize,
) -> Result<()> {
    if tree.root() == Some(id) {
        anyhow::bail!("cannot move the root node `{id}`");
    }
    let node = tree
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` does not exist"))?;
    if !tree.contains(new_parent) {
        anyhow::bail!("parent `{new_parent}` does not exist");
    }
    if new_parent == id {
        anyhow::bail!("cannot place `{id}` under itself");
    }
    let mut cursor = Some(new_parent.clone());
    while let Some(current) = cursor {
        if &current == id {
            anyhow::bail!("cannot place `{id}` under its descendant `{new_parent}`");
        }
        cursor = tree.get(&current).and_then(|n| n.parent.clone());
    }
    let old_parent = node.parent.clone();
    if let Some(old) = &old_parent {
        if let Some(n) = tree.get_mut(old) {
            n.children.retain(|c| c != id);
        }
    }
    let node_parent = tree
        .get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` does not exist"))?;
    node_parent.parent = Some(new_parent.clone());
    let host = tree
        .get_mut(new_parent)
        .ok_or_else(|| anyhow::anyhow!("parent `{new_parent}` does not exist"))?;
    let at = index.min(host.children.len());
    host.children.insert(at, id.clone());
    Ok(())
}

/// Shift `id` by `delta` slots among its siblings (negative moves up).
/// Out-of-range shifts clamp to the first/last slot; parentless nodes and
/// the root are errors.
pub fn move_sibling(tree: &mut NodeTree, id: &NodeId, delta: i32) -> Result<()> {
    let node = tree
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` does not exist"))?;
    let parent = node
        .parent
        .clone()
        .ok_or_else(|| anyhow::anyhow!("node `{id}` has no parent to reorder in"))?;
    let siblings = tree.children_of(&parent);
    let pos = siblings
        .iter()
        .position(|s| s == id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` is not listed under its parent"))?;
    let len = siblings.len() as i32;
    let at = (pos as i32 + delta).clamp(0, len - 1) as usize;
    place_node(tree, id, &parent, at)
}

/// Set one prop on `id`. Unknown nodes are errors.
pub fn set_prop(tree: &mut NodeTree, id: &NodeId, key: &str, value: PropValue) -> Result<()> {
    match tree.get_mut(id) {
        Some(node) => {
            node.props.insert(key.to_string(), value);
            Ok(())
        }
        None => anyhow::bail!("node `{id}` does not exist"),
    }
}

/// Remove one prop from `id`. Unknown nodes are errors.
pub fn remove_prop(tree: &mut NodeTree, id: &NodeId, key: &str) -> Result<()> {
    match tree.get_mut(id) {
        Some(node) => {
            node.props.0.remove(key);
            Ok(())
        }
        None => anyhow::bail!("node `{id}` does not exist"),
    }
}

/// Rename display `name` of node `id`.
pub fn rename_node(tree: &mut NodeTree, id: &NodeId, new_name: &str) -> Result<()> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        anyhow::bail!("node name cannot be empty");
    }
    let node = tree
        .get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` does not exist"))?;
    node.name = new_name.to_string();
    Ok(())
}

/// Recursively clone `id` and its subtree under `id`'s parent, placing it immediately after `id`.
pub fn duplicate_node(tree: &mut NodeTree, id: &NodeId) -> Result<NodeId> {
    if tree.root() == Some(id) {
        anyhow::bail!("cannot duplicate the root node `{id}`");
    }
    let node = tree
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("node `{id}` does not exist"))?;
    let parent = node
        .parent
        .clone()
        .ok_or_else(|| anyhow::anyhow!("node `{id}` has no parent"))?;

    // Collect all nodes in subtree: (old_id, old_parent_id, type_name, old_name, props, script)
    let mut subtree = Vec::new();
    let mut stack = vec![id.clone()];
    while let Some(curr_id) = stack.pop() {
        let n = tree
            .get(&curr_id)
            .ok_or_else(|| anyhow::anyhow!("node `{curr_id}` missing"))?;
        let children = tree.children_of(&curr_id);
        subtree.push((
            curr_id.clone(),
            n.parent.clone(),
            n.type_name.clone(),
            n.name.clone(),
            n.props.clone(),
            n.script.clone(),
        ));
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }

    // Map old_id -> (new_id, new_name)
    let mut id_map = std::collections::HashMap::new();
    let root_old_name = &subtree[0].3;
    let mut new_root_name = format!("{root_old_name}_copy");
    let slug = slugify(&new_root_name);
    let mut candidate = slug.clone();
    let mut counter = 2;
    while tree.contains(&NodeId::from(candidate.clone())) {
        candidate = format!("{slug}_{counter}");
        new_root_name = format!("{root_old_name}_copy{counter}");
        counter += 1;
    }
    let new_root_id = NodeId::from(candidate);
    id_map.insert(id.clone(), (new_root_id.clone(), new_root_name));

    for (old_id, _, _, old_name, _, _) in &subtree[1..] {
        let child_slug = slugify(old_name);
        let mut child_cand = child_slug.clone();
        let mut cnt = 2;
        while tree.contains(&NodeId::from(child_cand.clone()))
            || id_map.values().any(|(nid, _)| nid.as_str() == child_cand)
        {
            child_cand = format!("{child_slug}_{cnt}");
            cnt += 1;
        }
        id_map.insert(old_id.clone(), (NodeId::from(child_cand), old_name.clone()));
    }

    // Insert new nodes in tree
    for (old_id, old_parent, type_name, _, props, script) in subtree {
        let (new_id, new_name) = id_map
            .get(&old_id)
            .ok_or_else(|| anyhow::anyhow!("Missing duplicated id mapping for {old_id}"))?
            .clone();
        let new_parent = if old_id == *id {
            Some(parent.clone())
        } else {
            let p = old_parent
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Child node {old_id} missing parent"))?;
            let mapped = id_map
                .get(p)
                .ok_or_else(|| anyhow::anyhow!("Missing parent id mapping for {p}"))?;
            Some(mapped.0.clone())
        };
        let desc = NodeDesc {
            id: new_id,
            type_name,
            name: new_name,
            parent: new_parent,
            props,
            script,
        };
        tree.insert(desc).map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    // Place new_root_id immediately after id in parent's children
    let siblings = tree.children_of(&parent);
    if let Some(pos) = siblings.iter().position(|s| s == id) {
        let _ = place_node(tree, &new_root_id, &parent, pos + 1);
    }

    Ok(new_root_id)
}

/// Instantiate a scene (.ptscn) as a child under `parent`.
pub fn instantiate_scene_as_child(
    tree: &mut NodeTree,
    parent: Option<NodeId>,
    scene_path: &Path,
) -> Result<NodeId> {
    let stem = scene_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("inst");
    let prefix_slug = slugify(stem);
    let mut prefix = format!("{prefix_slug}_");
    let mut counter = 2;
    while tree.iter().any(|n| n.id.as_str().starts_with(&prefix)) {
        prefix = format!("{prefix_slug}{counter}_");
        counter += 1;
    }
    let (descs, root_id) = pite_scene::instantiate(scene_path, &prefix, &BTreeMap::new())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let root_node_id = NodeId::from(root_id);

    let effective_parent = match parent {
        Some(p) => {
            if !tree.contains(&p) {
                anyhow::bail!("parent `{p}` does not exist");
            }
            Some(p)
        }
        None => tree.root().cloned(),
    };

    for mut desc in descs {
        if desc.id == root_node_id {
            desc.parent = effective_parent.clone();
        }
        tree.insert(desc).map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    Ok(root_node_id)
}

/// Paths needed to resolve `[[instance]]` scene references while saving.
#[derive(Clone, Copy)]
pub struct InstanceRefs<'a> {
    pub scene_dir: &'a Path,
    pub project_dir: Option<&'a Path>,
}

/// Build a saveable [`SceneDoc`] from the live tree, in tree order.
///
/// `source` is the scene file as it exists on disk and stays authoritative:
/// its `[[instance]]` entries are re-attached instead of being flattened into
/// plain nodes (a live tree cannot tell instantiated nodes from authored
/// ones). Prop edits inside an instantiated subtree are written back as
/// `overrides`, so a save round-trips exactly; edits the format cannot
/// express (renaming, retyping, reparenting or deleting inside a subtree,
/// removing an inherited prop) fail loudly instead of being dropped.
pub fn build_doc(
    tree: &NodeTree,
    source: Option<&SceneDoc>,
    refs: Option<InstanceRefs<'_>>,
) -> Result<SceneDoc> {
    if let Some(source) = source {
        if !source.instance.is_empty() && refs.is_none() {
            anyhow::bail!(
                "cannot save: {} instance(s) need scene/project paths to resolve",
                source.instance.len()
            );
        }
    }
    let root = tree.root().map(ToString::to_string).unwrap_or_default();
    let mut owned: HashSet<String> = HashSet::new();
    let mut instance: Vec<SceneInstance> = Vec::new();
    if let (Some(source), Some(refs)) = (source, refs) {
        for inst in &source.instance {
            let plan = plan_instance(inst, tree, &root, refs)
                .with_context(|| format!("cannot save instance of {}", inst.scene))?;
            owned.extend(plan.owned);
            instance.push(plan.entry);
        }
    }
    let mut node = Vec::new();
    for n in tree.iter() {
        if owned.contains(n.id.as_str()) {
            continue;
        }
        if let Some(parent) = &n.parent {
            if owned.contains(parent.as_str()) {
                anyhow::bail!(
                    "node {:?} is parented inside an instanced subtree; the scene format \
                     cannot express that",
                    n.id
                );
            }
        }
        node.push(SceneNode {
            id: n.id.to_string(),
            type_name: n.type_name.clone(),
            name: n.name.clone(),
            parent: n.parent.clone().map(|p| p.to_string()),
            props: props_to_toml_filtered(&n.type_name, &n.props),
            script: n.script.as_ref().map(|s| SceneScript {
                path: s.path.clone(),
                class: s.class_name.clone(),
            }),
        });
    }
    if let Some(source) = source {
        reject_foreign_nodes(tree, source, &owned)?;
    }
    Ok(SceneDoc {
        format_version: FORMAT_VERSION,
        root,
        node,
        instance,
    })
}

fn props_to_toml(props: &Props) -> BTreeMap<String, toml::Value> {
    let mut out = BTreeMap::new();
    for (k, v) in &props.0 {
        out.insert(k.clone(), prop_to_toml(v));
    }
    out
}

/// Like `props_to_toml` but omits props whose value equals the schema
/// default for `type_name`, keeping scene files minimal. Unknown (non-schema)
/// props always save. A save → load → save round-trip is byte-identical
/// because load fills the same defaults before comparing.
fn props_to_toml_filtered(type_name: &str, props: &Props) -> BTreeMap<String, toml::Value> {
    let schema_defaults: std::collections::HashMap<&str, pite_core::PropValue> =
        pite_core::all_props(type_name)
            .iter()
            .map(|p| (p.key, p.default_value.to_prop_value()))
            .collect();
    let mut out = BTreeMap::new();
    for (k, v) in &props.0 {
        if let Some(default) = schema_defaults.get(k.as_str()) {
            if default == v {
                continue;
            }
        }
        out.insert(k.clone(), prop_to_toml(v));
    }
    out
}

/// A re-attached instance entry plus the live ids it owns.
struct InstancePlan {
    entry: SceneInstance,
    owned: HashSet<String>,
}

fn resolve_instance_path(scene_ref: &str, refs: InstanceRefs<'_>) -> Result<PathBuf> {
    pite_core::resolve_project_ref(refs.project_dir, refs.scene_dir, scene_ref)
        .map_err(anyhow::Error::from)
}

/// Re-derive one `[[instance]]` entry from the live tree: the subtree must
/// still match the referenced scene node-for-node, and every prop that differs
/// from that scene becomes an override.
fn plan_instance(
    inst: &SceneInstance,
    tree: &NodeTree,
    doc_root: &str,
    refs: InstanceRefs<'_>,
) -> Result<InstancePlan> {
    let path = resolve_instance_path(&inst.scene, refs)?;
    let ref_doc = pite_scene::load_scene(&path)?;
    // Fill schema defaults on the reference so prop comparison sees canonical
    // values. Without this, a live prop equal to a schema default would look
    // "changed" against a sparse reference and get spuriously recorded as an
    // override.
    let ref_tree = pite_scene::to_node_tree(&ref_doc)?;

    let mut owned = HashSet::new();
    let mut overrides: BTreeMap<String, toml::Value> = BTreeMap::new();
    for ref_node in &ref_doc.node {
        let live_id = format!("{}{}", inst.prefix, ref_node.id);
        let live = tree
            .get(&NodeId::from(live_id.clone()))
            .with_context(|| format!("live tree is missing `{live_id}`"))?;
        let ref_filled = ref_tree
            .get(&NodeId::from(ref_node.id.clone()))
            .with_context(|| format!("reference tree is missing `{}`", ref_node.id))?;
        owned.insert(live_id.clone());
        if live.type_name != ref_node.type_name || live.name != ref_node.name {
            anyhow::bail!("`{live_id}` was renamed or retyped; overrides carry props only");
        }
        let live_script = live
            .script
            .as_ref()
            .map(|s| (s.path.as_str(), s.class_name.as_str()));
        let ref_script = ref_node
            .script
            .as_ref()
            .map(|s| (s.path.as_str(), s.class.as_str()));
        if live_script != ref_script {
            anyhow::bail!("script on `{live_id}` cannot be overridden per instance");
        }
        if ref_node.id != ref_doc.root {
            let expected = ref_node
                .parent
                .as_ref()
                .map(|p| format!("{}{p}", inst.prefix));
            if live.parent.as_ref().map(ToString::to_string) != expected {
                anyhow::bail!("`{live_id}` was reparented inside the instance");
            }
        }
        // Full prop comparison: ref_filled is schema-filled, live is
        // schema-filled-then-overridden. Compare every key.
        for key in ref_filled.props.0.keys() {
            if !live.props.0.contains_key(key) {
                anyhow::bail!(
                    "prop `{key}` was removed from `{live_id}`; an instance cannot \
                     unset an inherited prop"
                );
            }
        }
        for (key, value) in props_to_toml(&live.props) {
            let ref_value = ref_filled.props.get(&key).map(prop_to_toml);
            if ref_value.as_ref() != Some(&value) {
                overrides.insert(format!("{}.{}", ref_node.id, key), value);
            }
        }
    }
    let subtree_root = format!("{}{}", inst.prefix, ref_doc.root);
    let attach = tree
        .get(&NodeId::from(subtree_root.clone()))
        .and_then(|n| n.parent.clone())
        .map(|p| p.to_string());
    let parent = match (&inst.parent, attach.as_deref()) {
        (Some(original), Some(current)) if original == current => Some(original.clone()),
        (None, Some(current)) if current == doc_root => None,
        (_, Some(current)) => Some(current.to_string()),
        (_, None) => None,
    };
    Ok(InstancePlan {
        entry: SceneInstance {
            scene: inst.scene.clone(),
            parent,
            prefix: inst.prefix.clone(),
            overrides,
        },
        owned,
    })
}

/// A node that belongs to no source entry and hangs under an instanced node
/// was added inside an instance — unrepresentable, so refuse the save.
fn reject_foreign_nodes(tree: &NodeTree, source: &SceneDoc, owned: &HashSet<String>) -> Result<()> {
    let authored: HashSet<&str> = source.node.iter().map(|n| n.id.as_str()).collect();
    for n in tree.iter() {
        let id = n.id.as_str();
        if authored.contains(id) || owned.contains(id) {
            continue;
        }
        let mut cursor = n.parent.clone();
        while let Some(parent) = cursor {
            if owned.contains(parent.as_str()) {
                anyhow::bail!(
                    "node `{id}` was added inside an instanced subtree; the scene \
                     format cannot express that"
                );
            }
            cursor = tree.get(&parent).and_then(|p| p.parent.clone());
        }
    }
    Ok(())
}

/// Parse Python-traceback `line N` (`File "...", line N`) marks: one
/// 1-based line (last match wins) plus the error's last non-empty line
/// (the exception line, not the `Traceback` header). Entries without a
/// line number (clean strings) are ignored.
pub fn gutter_marks(errors: &[String]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for err in errors {
        let last = err
            .lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty())
            .unwrap_or("");
        let mut line_no: Option<usize> = None;
        for (idx, _) in err.match_indices("line ") {
            let rest = &err[idx + "line ".len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                if let Ok(n) = digits.parse::<usize>() {
                    line_no = Some(n);
                }
            }
        }
        if let Some(n) = line_no {
            out.push((n, last.to_string()));
        }
    }
    out
}

/// Thin file write used by scene and script saves.
pub fn save_text(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests;
