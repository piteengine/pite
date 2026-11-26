// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reachable-scene walk: one visit per scene, collecting structured findings.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::diagnostics::{format_path_rel, Finding, Severity};

use super::report::CacheEntry;

/// State threaded through the recursive scene walk.
pub struct Walker<'a> {
    root: &'a Path,
    custom_props: &'a HashMap<String, String>,
    asset_registry: &'a pite_assets::AssetRegistry,
    by_hash: &'a HashMap<&'a str, &'a str>,
    schemas: pite_core::SchemaRegistry,
    plugin_ids: HashSet<String>,
    pub findings: Vec<Finding>,
    pub cache: Vec<CacheEntry>,
    pub atlas_uses: Vec<pite_assets::AtlasUse>,
    pub anim_uses: Vec<pite_assets::AnimationUse>,
    visited: HashSet<String>,
    cache_seen: HashSet<String>,
    pub referenced: HashSet<String>,
}

impl<'a> Walker<'a> {
    pub fn new(
        root: &'a Path,
        custom_props: &'a HashMap<String, String>,
        asset_registry: &'a pite_assets::AssetRegistry,
        by_hash: &'a HashMap<&'a str, &'a str>,
    ) -> Self {
        Self {
            root,
            custom_props,
            asset_registry,
            by_hash,
            schemas: pite_core::SchemaRegistry::new(),
            plugin_ids: HashSet::new(),
            findings: Vec::new(),
            cache: Vec::new(),
            atlas_uses: Vec::new(),
            anim_uses: Vec::new(),
            visited: HashSet::new(),
            cache_seen: HashSet::new(),
            referenced: HashSet::new(),
        }
    }

    /// Configures the [`SchemaRegistry`] and enabled plugin IDs used during checks.
    pub fn with_plugins(
        mut self,
        schemas: pite_core::SchemaRegistry,
        plugin_ids: HashSet<String>,
    ) -> Self {
        self.schemas = schemas;
        self.plugin_ids = plugin_ids;
        self
    }

    fn error(&mut self, finding: Finding) {
        debug_assert_eq!(finding.severity, Severity::Error);
        self.findings.push(finding);
    }

    fn warning(&mut self, finding: Finding) {
        debug_assert_eq!(finding.severity, Severity::Warning);
        self.findings.push(finding);
    }

    /// Record a scene load once per file: an instanced scene is loaded by the
    /// instance check and again by its own walk, but the cache report counts
    /// each scene file once.
    fn record_cache(&mut self, status: impl ToString, file: String) {
        if self.cache_seen.insert(file.clone()) {
            self.cache.push(CacheEntry::new(status.to_string(), file));
        }
    }

    fn rel(&self, path: &Path) -> String {
        format_path_rel(self.root, &path.to_string_lossy())
    }

    /// Number of distinct scenes visited so far.
    pub fn visited_len(&self) -> usize {
        self.visited.len()
    }

    fn scene_path(&self, reference: &str) -> std::result::Result<PathBuf, pite_core::ResPathError> {
        pite_core::resolve_project_ref(Some(self.root), self.root, reference)
    }

    /// Walk `scene_ref` and every scene it instances.
    pub fn walk(&mut self, scene_ref: &str) {
        let path = match self.scene_path(scene_ref) {
            Ok(path) => path,
            Err(error) => {
                let rel = format_path_rel(self.root, scene_ref);
                self.error(Finding::scene_load_failed(&rel, &error.to_string()));
                return;
            }
        };
        if !self.visited.insert(path.to_string_lossy().to_string()) {
            return;
        }
        let doc = match pite_scene::load_cached(&path) {
            Ok((doc, status, warning)) => {
                let scene_rel = self.rel(&path);
                if let Some(warning) = warning {
                    self.warning(Finding::scene_warning(&scene_rel, &warning));
                }
                self.record_cache(status, scene_rel);
                doc
            }
            Err(error) => {
                let scene_rel = self.rel(&path);
                self.error(Finding::scene_load_failed(&scene_rel, &error.to_string()));
                return;
            }
        };
        let scene_rel = self.rel(&path);
        let scene_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.root.to_path_buf());
        let ids: HashSet<&str> = doc.node.iter().map(|n| n.id.as_str()).collect();
        if !ids.contains(doc.root.as_str()) {
            self.error(Finding::scene_root_missing(&scene_rel, &doc.root));
        }

        for node in &doc.node {
            self.check_node_type(&scene_rel, node, &ids);
            self.check_node_script(&scene_rel, node);
            self.check_sprite(&scene_rel, node);
            self.check_animated_sprite(&scene_rel, node);
            self.check_tilemap(&scene_rel, node);
            self.check_prop_references(&scene_rel, node, &scene_dir);
            self.check_custom_props(&scene_rel, node);
        }

        for instance in &doc.instance {
            self.check_instance(&scene_rel, instance, &doc, &ids, &scene_dir);
        }
    }

    fn check_node_type(&mut self, scene: &str, node: &pite_scene::SceneNode, ids: &HashSet<&str>) {
        if !self.schemas.contains(&node.type_name) {
            self.error(Finding::unregistered_type(scene, &node.id, &node.type_name));
        }
        if let Some(parent) = &node.parent {
            if !ids.contains(parent.as_str()) {
                self.error(Finding::unknown_parent(scene, &node.id, parent));
            }
        }
    }

    fn check_node_script(&mut self, scene: &str, node: &pite_scene::SceneNode) {
        let Some(script) = &node.script else {
            return;
        };
        let Ok(script_path) =
            pite_core::resolve_project_ref(Some(self.root), self.root, &script.path)
        else {
            self.warning(Finding::missing_script(scene, &node.id, &script.path));
            return;
        };
        if !script_path.is_file() {
            self.warning(Finding::missing_script(scene, &node.id, &script.path));
        }
        if pite_script::is_reserved(&script.class) {
            self.warning(Finding::reserved_class(scene, &node.id, &script.class));
        }
        let Ok(text) = std::fs::read_to_string(&script_path) else {
            return;
        };
        for module in script_imports(&text) {
            if module == "pite" || module == "__future__" || self.plugin_ids.contains(module) {
                continue;
            }
            let sibling = script_path.parent().is_some_and(|dir| {
                dir.join(format!("{module}.py")).is_file()
                    || dir.join(module).join("__init__.py").is_file()
            });
            let vendored = crate::cmd::vendor::vendor_provides(&self.root.join("vendor"), module);
            if sibling || vendored || pite_script::is_stdlib_module(module) {
                continue;
            }
            self.warning(Finding::third_party_import(scene, &node.id, module));
        }
        for asset in pite_assets::played_assets(&text) {
            self.referenced.insert(normalize_asset_ref(asset));
            let Ok(asset_path) = pite_core::resolve_project_ref(Some(self.root), self.root, asset)
            else {
                self.error(Finding::missing_audio(scene, &node.id, asset));
                continue;
            };
            if !asset_path.is_file() {
                self.error(Finding::missing_audio(scene, &node.id, asset));
            } else if let Err(error) = pite_audio::decode_audio(&asset_path) {
                self.error(Finding::undecodable_audio(
                    scene,
                    &node.id,
                    asset,
                    &format!("{error:#}"),
                ));
            }
        }
    }

    fn check_sprite(&mut self, scene: &str, node: &pite_scene::SceneNode) {
        if node.type_name != "Sprite2D" {
            return;
        }
        let prop = |key: &str| node.props.get(key).and_then(|value| value.as_str());
        match pite_assets::resolve_sprite_source(prop("texture"), prop("atlas"), prop("frame")) {
            Ok(pite_assets::SpriteSource::Atlas { atlas, frame }) => {
                self.atlas_uses.push(pite_assets::AtlasUse {
                    scene: scene.to_string(),
                    node: node.id.clone(),
                    atlas,
                    frame,
                });
            }
            Ok(_) => {}
            Err(error) => {
                self.error(Finding::sprite_source_invalid(scene, &node.id, &error));
            }
        }
    }

    fn check_animated_sprite(&mut self, scene: &str, node: &pite_scene::SceneNode) {
        if node.type_name != "AnimatedSprite2D" {
            return;
        }
        let prop = |key: &str| node.props.get(key).and_then(|value| value.as_str());
        let atlas = prop("atlas").unwrap_or_default();
        let animation = prop("animation").unwrap_or_default();
        let autoplay = prop("autoplay").unwrap_or_default();

        if atlas.is_empty() {
            if !animation.is_empty() {
                self.error(Finding::animated_requires_atlas(
                    scene,
                    &node.id,
                    "animation",
                ));
            }
            if !autoplay.is_empty() {
                self.error(Finding::animated_requires_atlas(
                    scene, &node.id, "autoplay",
                ));
            }
            return;
        }

        self.referenced.insert(normalize_asset_ref(atlas));
        if !animation.is_empty() {
            self.anim_uses.push(pite_assets::AnimationUse {
                scene: scene.to_string(),
                node: node.id.clone(),
                atlas: atlas.to_string(),
                animation: animation.to_string(),
            });
        }
        if !autoplay.is_empty() && autoplay != animation {
            self.anim_uses.push(pite_assets::AnimationUse {
                scene: scene.to_string(),
                node: node.id.clone(),
                atlas: atlas.to_string(),
                animation: autoplay.to_string(),
            });
        }
    }

    fn check_tilemap(&mut self, scene: &str, node: &pite_scene::SceneNode) {
        if node.type_name != "TileMap2D" {
            return;
        }
        let Some(map_ref) = node.props.get("tilemap").and_then(|value| value.as_str()) else {
            return;
        };
        let Ok(map_path) = pite_core::resolve_project_ref(Some(self.root), self.root, map_ref)
        else {
            return;
        };
        if !map_path.is_file() {
            return;
        }
        match pite_scene::load_tilemap(&map_path) {
            Ok(doc) => {
                self.referenced.insert(normalize_asset_ref(&doc.atlas));
                for layer in &doc.layers {
                    for tile in &layer.tiles {
                        self.atlas_uses.push(pite_assets::AtlasUse {
                            scene: scene.to_string(),
                            node: node.id.clone(),
                            atlas: doc.atlas.clone(),
                            frame: tile.2.clone(),
                        });
                    }
                }
            }
            Err(error) => self.error(Finding::invalid_tilemap(
                scene,
                &node.id,
                map_ref,
                &format!("{error:#}"),
            )),
        }
    }

    fn check_prop_references(
        &mut self,
        scene: &str,
        node: &pite_scene::SceneNode,
        scene_dir: &Path,
    ) {
        for value in node.props.values() {
            let Some(reference) = value.as_str() else {
                continue;
            };
            if !reference.starts_with("res://") {
                continue;
            }
            self.referenced.insert(reference.to_string());
            let asset_path = resolve_asset(self.root, scene_dir, reference);
            if asset_path.as_ref().is_some_and(|p| p.is_file()) {
                continue;
            }
            let moved_to = self
                .asset_registry
                .iter()
                .find(|entry| entry.path == reference)
                .and_then(|entry| {
                    if entry.hash.is_empty() {
                        None
                    } else {
                        self.by_hash.get(entry.hash.as_str()).copied()
                    }
                })
                .filter(|new_path| *new_path != reference);
            match moved_to {
                Some(new_path) => {
                    self.error(Finding::moved_asset(scene, &node.id, reference, new_path))
                }
                None => self.error(Finding::missing_asset(scene, &node.id, reference)),
            }
        }
    }

    fn check_custom_props(&mut self, scene: &str, node: &pite_scene::SceneNode) {
        // Schema props skip: they have typed accessors and their own checks.
        let schema_keys: HashSet<String> = self
            .schemas
            .all_props(&node.type_name)
            .into_iter()
            .map(|prop| prop.key.to_string())
            .collect();
        let mut keys: Vec<&String> = node.props.keys().collect();
        keys.sort();
        for key in keys {
            if schema_keys.contains(key.as_str()) {
                continue;
            }
            let raw = &node.props[key];
            match self.custom_props.get(key) {
                Some(declared) => {
                    let value = pite_scene::toml_to_prop(raw);
                    if let Err(error) = pite_core::validate_custom_prop_type(declared, &value) {
                        self.error(Finding::custom_prop_type(
                            scene,
                            &node.id,
                            key,
                            &error.to_string(),
                        ));
                    }
                }
                None => self.warning(Finding::unknown_prop(scene, &node.id, key)),
            }
        }
    }

    fn check_instance(
        &mut self,
        scene: &str,
        instance: &pite_scene::SceneInstance,
        doc: &pite_scene::SceneDoc,
        ids: &HashSet<&str>,
        scene_dir: &Path,
    ) {
        let instance_path =
            match pite_core::resolve_project_ref(Some(self.root), scene_dir, &instance.scene) {
                Ok(path) => path,
                Err(error) => {
                    self.error(Finding::instance_load(
                        scene,
                        &instance.scene,
                        &format!("{error:#}"),
                    ));
                    return;
                }
            };
        let ref_doc = match pite_scene::load_cached(&instance_path) {
            Ok((doc, status, warning)) => {
                let instance_rel = self.rel(&instance_path);
                if let Some(warning) = warning {
                    self.warning(Finding::scene_warning(&instance_rel, &warning));
                }
                self.record_cache(status, instance_rel);
                doc
            }
            Err(error) => {
                self.error(Finding::instance_load(
                    scene,
                    &instance.scene,
                    &format!("{error:#}"),
                ));
                return;
            }
        };

        let attach_at = instance.parent.clone().unwrap_or_else(|| doc.root.clone());
        if !ids.contains(attach_at.as_str()) {
            self.error(Finding::instance_parent(scene, &instance.scene, &attach_at));
        }
        let ref_ids: HashSet<&str> = ref_doc.node.iter().map(|node| node.id.as_str()).collect();
        for key in instance.overrides.keys() {
            match key.split_once('.') {
                Some((node_id, _)) if ref_ids.contains(node_id) => {}
                _ => self.error(Finding::instance_override(scene, &instance.scene, key)),
            }
        }
        // Override values apply as props at runtime, so asset refs hiding in
        // them count exactly like prop refs.
        for value in instance.overrides.values() {
            if let Some(reference) = value.as_str() {
                if reference.starts_with("res://") {
                    self.referenced.insert(reference.to_string());
                }
            }
        }

        let nested_ref = if instance.scene.starts_with("res://") {
            instance.scene.clone()
        } else {
            scene_dir
                .join(&instance.scene)
                .to_string_lossy()
                .to_string()
        };
        self.walk(&nested_ref);
    }
}

fn resolve_asset(root: &Path, scene_dir: &Path, reference: &str) -> Option<PathBuf> {
    pite_core::resolve_project_ref(Some(root), scene_dir, reference).ok()
}

pub fn normalize_asset_ref(asset: &str) -> String {
    if asset.starts_with("res://") {
        return asset.to_string();
    }
    let rel = asset.replace('\\', "/");
    let rel = rel.strip_prefix("./").unwrap_or(&rel);
    format!("res://{rel}")
}

/// Top-level module names imported by a script (`import a, b`, `from a
/// import …`), comment-stripped like `played_assets`. Relative imports
/// (`.x`) resolve to siblings, never third-party.
pub fn script_imports(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if let Some(list) = line.strip_prefix("import ") {
            for item in list.split(',') {
                let name = item
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(';');
                let top = name.split('.').next().unwrap_or("");
                if !top.is_empty() && !out.contains(&top) {
                    out.push(top);
                }
            }
        } else if let Some(after_from) = line.strip_prefix("from ") {
            let module = after_from.split_whitespace().next().unwrap_or("");
            if module.starts_with('.') {
                continue;
            }
            let top = module.split('.').next().unwrap_or("");
            if !top.is_empty() && !out.contains(&top) {
                out.push(top);
            }
        }
    }
    out
}
