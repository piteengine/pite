// SPDX-License-Identifier: MIT OR Apache-2.0

mod physics;
mod rebuild;
mod render;
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
use pite_core::{global_position, NodeId, NodeTree, PropValue};
use pite_physics::PhysicsWorld;
use pite_script::{Pyo3Backend, ScriptBackend, ScriptHost};

use crate::scenes::resolve_script;

use rebuild::InstanceSlot;

/// Active game execution session managing scene nodes, script instances, physics simulation,
/// and live hot reload.
pub struct GameSession {
    host: Arc<ScriptHost>,
    instances: Vec<InstanceSlot>,
    scene: PathBuf,
    scene_dir: PathBuf,
    project_dir: Option<PathBuf>,
    no_reload: bool,
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
}

impl GameSession {
    pub fn open(scene: &Path, no_reload: bool) -> Result<Self> {
        let scene_dir = scene
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let project_dir = pite_project::find_project_root(scene);
        let physics = Arc::new(Mutex::new(PhysicsWorld::new()));
        let mut session = Self {
            host: Arc::new(ScriptHost::new(NodeTree::new())),
            instances: Vec::new(),
            scene: scene.to_path_buf(),
            scene_dir,
            project_dir,
            no_reload,
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
        };
        pite_script::set_current_host(session.host.clone());
        pite_script::set_current_physics(Some(session.physics.clone()));
        pite_audio::set_project_dir(session.project_dir.clone());
        pite_script::set_project_dir(session.project_dir.clone());
        if let Some(dir) = session.project_dir.as_deref() {
            if let Ok(manifest) = pite_project::load_manifest(dir) {
                session.host.set_custom_prop_schema(manifest.props.clone());
            }
        }
        session.rebuild()?;
        if !session.no_reload {
            session.start_watcher()?;
        }
        Ok(session)
    }

    pub fn host(&self) -> &Arc<ScriptHost> {
        &self.host
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
