// SPDX-License-Identifier: MIT OR Apache-2.0

mod physics;
mod rebuild;
mod render;
pub mod tilemap_cache;
mod update;
mod watcher;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use pite_core::{global_position, NodeId, NodeTree, PropValue, SchemaRegistry};
use pite_physics::PhysicsWorld;
use pite_script::{Pyo3Backend, ScriptBackend, ScriptHost};

use crate::scenes::resolve_script;

use rebuild::InstanceSlot;
pub use tilemap_cache::BakedTileMap;

/// Configuration options for opening a [`GameSession`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionOptions {
    /// Disable filesystem watching and live hot reload.
    pub no_reload: bool,
    /// Safe Mode: build and render the scene tree without loading or executing Python scripts.
    pub safe_mode: bool,
    /// Record plugin load/runtime errors in `GameSession::plugin_errors` instead of failing `open`.
    pub allow_plugin_errors: bool,
}

/// Active game execution session managing scene nodes, script instances, physics simulation,
/// and live hot reload.
pub struct GameSession {
    host: Arc<ScriptHost>,
    schemas: Arc<SchemaRegistry>,
    plugin_host: Arc<pite_script::PluginHost>,
    plugin_node_classes: HashMap<String, String>,
    pub plugin_errors: Vec<String>,
    allow_plugin_errors: bool,
    instances: Vec<InstanceSlot>,
    scene: PathBuf,
    scene_dir: PathBuf,
    project_dir: Option<PathBuf>,
    no_reload: bool,
    safe_mode: bool,
    _watcher: Option<RecommendedWatcher>,
    watch_rx: Option<Receiver<std::result::Result<notify::Event, notify::Error>>>,
    viewport: (u32, u32),
    physics: Arc<Mutex<PhysicsWorld>>,
    physics_accumulator: f64,
    pub pause_on_error: bool,
    pub last_error: Option<pite_script::ScriptError>,
    pub dirty_assets: HashSet<PathBuf>,
    animation_times: HashMap<String, f64>,
    atlas_cache: HashMap<PathBuf, pite_render::atlas::Atlas>,
    tilemap_cache: HashMap<String, BakedTileMap>,
    has_animated_sprites: bool,
}

impl GameSession {
    pub fn open(scene: &Path, no_reload: bool) -> Result<Self> {
        Self::open_with_options(
            scene,
            SessionOptions {
                no_reload,
                safe_mode: false,
                allow_plugin_errors: false,
            },
        )
    }

    pub fn open_with_options(scene: &Path, options: SessionOptions) -> Result<Self> {
        let scene_dir = scene
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let project_dir = pite_project::find_project_root(scene);
        let schemas = Arc::new(SchemaRegistry::new());
        let host = Arc::new(ScriptHost::new(NodeTree::new()));
        host.set_schema_registry(schemas.clone());
        let physics = Arc::new(Mutex::new(PhysicsWorld::new()));
        let plugin_host = Arc::new(pite_script::PluginHost::new());
        let mut session = Self {
            host,
            schemas,
            plugin_host,
            plugin_node_classes: HashMap::new(),
            plugin_errors: Vec::new(),
            allow_plugin_errors: options.allow_plugin_errors,
            instances: Vec::new(),
            scene: scene.to_path_buf(),
            scene_dir,
            project_dir,
            no_reload: options.no_reload,
            safe_mode: options.safe_mode,
            _watcher: None,
            watch_rx: None,
            viewport: (800, 600),
            physics,
            physics_accumulator: 0.0,
            pause_on_error: false,
            last_error: None,
            dirty_assets: HashSet::new(),
            animation_times: HashMap::new(),
            atlas_cache: HashMap::new(),
            tilemap_cache: HashMap::new(),
            has_animated_sprites: false,
        };
        pite_script::set_current_host(session.host.clone());
        pite_script::set_current_physics(Some(session.physics.clone()));
        pite_audio::set_project_dir(session.project_dir.clone());
        pite_script::set_project_dir(session.project_dir.clone());
        session.reload_input_map();
        session.load_project_plugins()?;
        session.rebuild()?;
        if !session.no_reload {
            session.start_watcher()?;
        }
        Ok(session)
    }

    /// Load `res://input.pite.toml` (if present) and install compiled action chords into `pite-script`.
    pub fn reload_input_map(&self) {
        let Some(dir) = self.project_dir.as_deref() else {
            pite_script::input_set_actions(HashMap::new());
            return;
        };
        let map = match pite_project::load_input_map(dir) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("failed to load {}: {e:#}", pite_project::INPUT_MAP_FILE);
                pite_script::input_set_actions(HashMap::new());
                return;
            }
        };
        let mut compiled: HashMap<String, Vec<Vec<String>>> = HashMap::new();
        for (action, bindings) in &map.actions {
            let mut chords = Vec::new();
            for b in bindings {
                match pite_project::parse_chord(b) {
                    Ok(chord) => chords.push(chord),
                    Err(err) => tracing::warn!("input action `{action}`: {err}"),
                }
            }
            compiled.insert(action.clone(), chords);
        }
        pite_script::input_set_actions(compiled);
    }

    /// Loads enabled plugin schemas, custom props, default node classes, and (when not in
    /// Safe Mode) runtime plugin entrypoints & autoload singletons.
    fn load_project_plugins(&mut self) -> Result<()> {
        self.plugin_errors.clear();
        self.plugin_node_classes.clear();
        let Some(dir) = self.project_dir.as_deref() else {
            return Ok(());
        };
        let Ok(manifest) = pite_project::load_manifest(dir) else {
            return Ok(());
        };
        let loaded = pite_project::load_enabled_plugins(dir, &manifest);
        let mut custom_props = manifest.props.clone();
        for (k, v) in loaded.custom_props {
            custom_props.entry(k).or_insert(v);
        }
        self.host.set_custom_prop_schema(custom_props);

        self.schemas = Arc::new(loaded.schemas);
        self.host.set_schema_registry(self.schemas.clone());

        for err in &loaded.errors {
            let msg = err.to_string();
            if !self.allow_plugin_errors {
                anyhow::bail!("{msg}");
            }
            tracing::warn!("{msg}");
            self.plugin_errors.push(msg);
        }

        for plugin in &loaded.plugins {
            for node in &plugin.manifest.nodes {
                if let Some(script_spec) = &node.script {
                    self.plugin_node_classes
                        .insert(node.type_name.clone(), script_spec.clone());
                }
            }
        }

        if !self.safe_mode && !loaded.plugins.is_empty() {
            let specs: Vec<pite_script::RuntimePluginSpec> = loaded
                .plugins
                .iter()
                .map(|p| pite_script::RuntimePluginSpec {
                    id: p.id.clone(),
                    runtime: p.manifest.runtime.clone(),
                    autoloads: p
                        .manifest
                        .autoloads
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                })
                .collect();
            let host_errors = self.plugin_host.load_plugins(&self.host, &specs);
            self.host.set_plugin_host(Some(self.plugin_host.clone()));
            for (plugin_id, msg) in host_errors {
                let formatted = format!("plugin {plugin_id}: {msg}");
                if !self.allow_plugin_errors {
                    anyhow::bail!("{formatted}");
                }
                tracing::error!("{formatted}");
                self.plugin_errors.push(formatted);
            }
        } else {
            self.host.set_plugin_host(None);
        }
        Ok(())
    }

    /// Returns `true` when the session is in Safe Mode (Python script execution disabled).
    pub fn is_safe_mode(&self) -> bool {
        self.safe_mode
    }

    /// Enables or disables Safe Mode and rebuilds script instances accordingly.
    ///
    /// # Errors
    /// Returns an error if reloading the scene or instantiating scripts fails.
    pub fn set_safe_mode(&mut self, safe_mode: bool) -> Result<()> {
        if self.safe_mode != safe_mode {
            self.safe_mode = safe_mode;
            self.load_project_plugins()?;
            self.rebuild()?;
        }
        Ok(())
    }

    pub fn host(&self) -> &Arc<ScriptHost> {
        &self.host
    }

    /// Returns the active [`SchemaRegistry`] for this session.
    pub fn schemas(&self) -> &Arc<SchemaRegistry> {
        &self.schemas
    }

    /// Installs an updated [`SchemaRegistry`] and propagates it to the script host.
    pub fn set_schemas(&mut self, schemas: Arc<SchemaRegistry>) {
        self.host.set_schema_registry(schemas.clone());
        self.schemas = schemas;
    }

    pub fn set_pause_on_error(&mut self, pause: bool) {
        self.pause_on_error = pause;
    }

    pub fn take_last_error(&mut self) -> Option<pite_script::ScriptError> {
        self.last_error.take()
    }

    pub fn resolve_path(&self, res_path: &str) -> PathBuf {
        resolve_script(res_path, &self.scene_dir, self.project_dir.as_deref())
            .unwrap_or_else(|| PathBuf::from(res_path))
    }

    pub fn errors(&self) -> Vec<String> {
        self.instances
            .iter()
            .filter_map(|s| s.backend.last_error())
            .collect()
    }

    pub fn tree_len(&self) -> usize {
        self.host.with_tree(|t| t.len())
    }

    pub fn scripted_count(&self) -> usize {
        self.instances.iter().filter(|s| s.file.is_some()).count()
    }

    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    pub fn set_viewport(&mut self, w: u32, h: u32) {
        if w > 0 && h > 0 {
            self.viewport = (w, h);
        }
    }

    pub fn design_size(&self) -> Option<(u32, u32)> {
        let dir = self.project_dir.as_deref()?;
        pite_project::load_manifest(dir)
            .ok()
            .map(|m| (m.project.window_width, m.project.window_height))
    }

    pub fn effective_zoom(&self) -> f64 {
        let ((_, _), zoom) = self.camera_view();
        let design = self.design_size().unwrap_or(self.viewport);
        zoom * pite_render::fit_scale(design, self.viewport)
    }

    pub fn camera_view(&self) -> ((f64, f64), f64) {
        self.host.with_tree(|tree| {
            tree.iter()
                .find(|n| n.type_name == "Camera2D")
                .map(|cam| {
                    let zoom = match cam.props.get("zoom") {
                        Some(PropValue::Num(z)) => *z,
                        _ => 1.0,
                    };
                    (global_position(tree, &cam.id), zoom)
                })
                .unwrap_or(((0.0, 0.0), 1.0))
        })
    }

    pub fn frame(&mut self, delta: f64) {
        self.poll_watch();
        self.update(delta);
        pite_script::input_begin_frame();
    }
}
