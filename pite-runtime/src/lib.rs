// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-runtime`: thin orchestrator wiring scene + render + script.
//! Opens a window, loads the scene file into a node tree, runs the loop.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use pite_core::{global_position, NodeId, NodeTree, PropValue};
use pite_physics::{PhysicsEvent, PhysicsWorld, ShapeConfig};
use pite_render::{Renderer2D, WgpuRenderer};
use pite_script::{
    input_mouse, input_pressed, input_released, Pyo3Backend, ScriptBackend, ScriptHost,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

fn key_name(code: KeyCode) -> Option<&'static str> {
    Some(match code {
        KeyCode::ArrowLeft => "ArrowLeft",
        KeyCode::ArrowRight => "ArrowRight",
        KeyCode::ArrowUp => "ArrowUp",
        KeyCode::ArrowDown => "ArrowDown",
        KeyCode::Space => "Space",
        KeyCode::Enter => "Enter",
        KeyCode::Escape => "Escape",
        KeyCode::Tab => "Tab",
        KeyCode::Backspace => "Backspace",
        KeyCode::ShiftLeft | KeyCode::ShiftRight => "Shift",
        KeyCode::ControlLeft | KeyCode::ControlRight => "Control",
        KeyCode::AltLeft | KeyCode::AltRight => "Alt",
        KeyCode::KeyA => "A",
        KeyCode::KeyB => "B",
        KeyCode::KeyC => "C",
        KeyCode::KeyD => "D",
        KeyCode::KeyE => "E",
        KeyCode::KeyF => "F",
        KeyCode::KeyG => "G",
        KeyCode::KeyH => "H",
        KeyCode::KeyI => "I",
        KeyCode::KeyJ => "J",
        KeyCode::KeyK => "K",
        KeyCode::KeyL => "L",
        KeyCode::KeyM => "M",
        KeyCode::KeyN => "N",
        KeyCode::KeyO => "O",
        KeyCode::KeyP => "P",
        KeyCode::KeyQ => "Q",
        KeyCode::KeyR => "R",
        KeyCode::KeyS => "S",
        KeyCode::KeyT => "T",
        KeyCode::KeyU => "U",
        KeyCode::KeyV => "V",
        KeyCode::KeyW => "W",
        KeyCode::KeyX => "X",
        KeyCode::KeyY => "Y",
        KeyCode::KeyZ => "Z",
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        _ => return None,
    })
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub scene: PathBuf,
    pub no_reload: bool,
}

struct ScriptSlot {
    id: NodeId,
    file: PathBuf,
    backend: Box<dyn ScriptBackend>,
    errored: bool,
}

pub struct GameSession {
    host: Arc<ScriptHost>,
    slots: Vec<ScriptSlot>,
    scene: PathBuf,
    scene_dir: PathBuf,
    project_dir: Option<PathBuf>,
    no_reload: bool,
    _watcher: Option<RecommendedWatcher>,
    watch_rx: Option<Receiver<std::result::Result<notify::Event, notify::Error>>>,
    viewport: (u32, u32),
    armed: HashSet<String>,
    timers: std::collections::HashMap<String, f64>,
    physics: Arc<Mutex<PhysicsWorld>>,
    physics_accumulator: f64,
    pub pause_on_error: bool,
    pub last_error: Option<pite_script::ScriptError>,
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
            slots: Vec::new(),
            scene: scene.to_path_buf(),
            scene_dir,
            project_dir,
            no_reload,
            _watcher: None,
            watch_rx: None,
            viewport: (800, 600),
            armed: HashSet::new(),
            timers: std::collections::HashMap::new(),
            physics,
            physics_accumulator: 0.0,
            pause_on_error: false,
            last_error: None,
        };
        pite_script::set_current_host(session.host.clone());
        pite_script::set_current_physics(Some(session.physics.clone()));
        pite_audio::set_project_dir(session.project_dir.clone());
        pite_script::set_project_dir(session.project_dir.clone());
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
        self.slots
            .iter()
            .filter_map(|s| s.backend.last_error())
            .collect()
    }

    pub fn reload_script_path(&mut self, path: &Path) {
        for slot in self.slots.iter_mut().filter(|s| same_file(&s.file, path)) {
            match slot.backend.reload() {
                Ok(()) => {
                    slot.errored = false;
                    tracing::info!(node = %slot.id, "script reloaded");
                }
                Err(e) => {
                    tracing::error!(node = %slot.id, "reload failed, keeping last good state: {e:#}")
                }
            }
        }
    }

    pub fn tree_len(&self) -> usize {
        self.host.with_tree(|t| t.len())
    }

    pub fn script_count(&self) -> usize {
        self.slots.len()
    }

    pub fn set_viewport(&mut self, w: u32, h: u32) {
        if w > 0 && h > 0 {
            self.viewport = (w, h);
        }
    }

    /// Project window size: explicit `window_width/height` when set, else the
    /// engine default. Doubles as the design resolution the game scales from
    /// and as the window's restore size when maximized.
    pub fn design_size(&self) -> Option<(u32, u32)> {
        let dir = self.project_dir.as_deref()?;
        pite_project::load_manifest(dir)
            .ok()
            .map(|m| (m.project.window_width, m.project.window_height))
    }

    /// Camera zoom composed with the contain-fit scale, so content keeps the
    /// design aspect at any window size. `button_at` shares it, keeping
    /// clicks on what is drawn.
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

    pub fn draw_into(&mut self, r: &mut dyn Renderer2D) -> Result<()> {
        let (cam, _) = self.camera_view();
        r.set_camera(cam.0, cam.1, self.effective_zoom());
        let sprite_nodes: Vec<(String, String, Option<String>, Option<String>, (f64, f64))> =
            self.host.with_tree(|tree| {
                tree.iter()
                    .filter(|n| n.type_name == "Sprite2D")
                    .map(|n| {
                        let prop = |key: &str| match n.props.get(key) {
                            Some(PropValue::Str(s)) => Some(s.clone()),
                            _ => None,
                        };
                        (
                            n.id.to_string(),
                            prop("texture").unwrap_or_default(),
                            prop("atlas"),
                            prop("frame"),
                            global_position(tree, &n.id),
                        )
                    })
                    .collect()
            });
        for (id, texture, atlas, frame, pos) in sprite_nodes {
            let has_texture = !texture.is_empty();
            if has_texture && atlas.is_some() {
                anyhow::bail!("node {id}: `texture` and `atlas` are mutually exclusive");
            }
            match (atlas, frame) {
                (Some(atlas), Some(frame)) => {
                    let sidecar = self.resolve_path(&atlas);
                    let sheet = pite_render::atlas::sheet_for_sidecar(&sidecar);
                    r.draw_sprite_frame(&sheet.to_string_lossy(), Some(&frame), pos.0, pos.1)?;
                }
                (Some(_), None) => anyhow::bail!("node {id}: `atlas` requires a `frame`"),
                (None, Some(_)) => anyhow::bail!("node {id}: `frame` requires an `atlas`"),
                (None, None) => {
                    if has_texture {
                        let resolved = self.resolve_path(&texture);
                        r.draw_sprite(&resolved.to_string_lossy(), pos.0, pos.1)?;
                    }
                }
            }
        }
        let labels: Vec<(String, f32, [u8; 4], (f64, f64))> = self.host.with_tree(|tree| {
            tree.iter()
                .filter(|n| n.type_name == "Label")
                .filter_map(|n| {
                    let text = match n.props.get("text") {
                        Some(PropValue::Str(s)) if !s.is_empty() => s.clone(),
                        _ => return None,
                    };
                    let size = match n.props.get("font_size") {
                        Some(PropValue::Num(s)) => *s as f32,
                        Some(PropValue::Int(s)) => *s as f32,
                        _ => 16.0,
                    };
                    let color = match n.props.get("color") {
                        Some(PropValue::Str(s)) => pite_render::text::parse_color(s),
                        _ => [255, 255, 255, 255],
                    };
                    Some((text, size, color, global_position(tree, &n.id)))
                })
                .collect()
        });
        for (text, size, color, pos) in labels {
            r.draw_text(&text, pos.0, pos.1, size, color)?;
        }
        let buttons: Vec<(String, f32, [u8; 4], (f64, f64), (f64, f64))> =
            self.host.with_tree(|tree| {
                tree.iter()
                    .filter(|n| n.type_name == "Button")
                    .map(|n| {
                        let text = match n.props.get("text") {
                            Some(PropValue::Str(s)) => s.clone(),
                            _ => String::new(),
                        };
                        let size = match n.props.get("font_size") {
                            Some(PropValue::Num(s)) => *s as f32,
                            Some(PropValue::Int(s)) => *s as f32,
                            _ => 16.0,
                        };
                        let color = match n.props.get("color") {
                            Some(PropValue::Str(s)) => pite_render::text::parse_color(s),
                            _ => [51, 65, 85, 255],
                        };
                        let (w, h) = button_size(n);
                        (text, size, color, global_position(tree, &n.id), (w, h))
                    })
                    .collect()
            });
        for (text, size, color, pos, (w, h)) in buttons {
            r.draw_rect(pos.0, pos.1, w, h, color)?;
            if !text.is_empty() {
                r.draw_text(&text, pos.0, pos.1, size, [255, 255, 255, 255])?;
            }
        }
        Ok(())
    }

    pub fn button_at(&self, screen: (f64, f64)) -> Option<String> {
        let ((cx, cy), _) = self.camera_view();
        let zoom = self.effective_zoom();
        let (world_x, world_y) = pite_render::screen_to_world(
            (screen.0 as f32, screen.1 as f32),
            (cx, cy),
            zoom,
            self.viewport,
        );
        self.host.with_tree(|tree| {
            tree.iter()
                .filter(|n| n.type_name == "Button")
                .find(|n| {
                    let (w, h) = button_size(n);
                    let (px, py) = global_position(tree, &n.id);
                    (world_x - px).abs() <= w / 2.0 && (world_y - py).abs() <= h / 2.0
                })
                .map(|n| n.id.to_string())
        })
    }

    fn fire_pressed(&self, id: &str) {
        if !self.host.has_signal(id, "pressed") {
            return;
        }
        if let Err(e) = pite_script::fire_pressed(&self.host, id) {
            tracing::error!(node = %id, "pressed handlers failed: {e:#}");
        }
    }

    fn fire_timeout(&self, id: &str) {
        if !self.host.has_signal(id, "timeout") {
            return;
        }
        if let Err(e) = pite_script::fire_timeout(&self.host, id) {
            tracing::error!(node = %id, "timeout handlers failed: {e:#}");
        }
    }

    fn rebuild(&mut self) -> Result<()> {
        let doc = pite_scene::load_scene_cached(&self.scene)
            .with_context(|| format!("cannot load scene {}", self.scene.display()))?;
        let tree = pite_scene::build_tree(&doc, &self.scene_dir, self.project_dir.as_deref())?;
        self.host.with_tree_mut(|t| *t = tree);
        // `pressed` belongs to the Button type itself (Godot-style), not to
        // each script: the engine fires it, so it must be declared BEFORE
        // any `_ready` runs — scripts connect to it during `_ready`.
        // Re-declaring it in a script is a harmless overwrite.
        let buttons: Vec<String> = self.host.with_tree(|tree| {
            tree.iter()
                .filter(|n| n.type_name == "Button")
                .map(|n| n.id.to_string())
                .collect()
        });
        for id in &buttons {
            if let Err(e) = self
                .host
                .declare_signals(id, vec![("pressed".to_string(), Vec::new())])
            {
                tracing::error!(node = %id, "cannot declare builtin pressed: {e:#}");
            }
        }
        // `timeout` belongs to the Timer type itself, like `pressed` to
        // Button: the engine fires it, so it must be declared BEFORE any
        // `_ready` runs — scripts connect to it during `_ready`.
        let timers: Vec<String> = self.host.with_tree(|tree| {
            tree.iter()
                .filter(|n| n.type_name == "Timer")
                .map(|n| n.id.to_string())
                .collect()
        });
        for id in &timers {
            if let Err(e) = self
                .host
                .declare_signals(id, vec![("timeout".to_string(), Vec::new())])
            {
                tracing::error!(node = %id, "cannot declare builtin timeout: {e:#}");
            }
        }

        let areas: Vec<String> = self.host.with_tree(|tree| {
            tree.iter()
                .filter(|n| n.type_name == "Area2D")
                .map(|n| n.id.to_string())
                .collect()
        });
        for id in &areas {
            let signals = vec![
                ("body_entered".to_string(), vec!["str".to_string()]),
                ("body_exited".to_string(), vec!["str".to_string()]),
                ("area_entered".to_string(), vec!["str".to_string()]),
                ("area_exited".to_string(), vec!["str".to_string()]),
            ];
            if let Err(e) = self.host.declare_signals(id, signals) {
                tracing::error!(node = %id, "cannot declare builtin area signals: {e:#}");
            }
        }

        self.rebuild_physics();
        self.timers.clear();
        let mut olds: HashMap<String, ScriptSlot> = HashMap::new();
        for slot in self.slots.drain(..) {
            olds.insert(slot.id.to_string(), slot);
        }
        let wanted: Vec<(NodeId, PathBuf, String)> = self.host.with_tree(|tree| {
            tree.iter()
                .filter_map(|node| {
                    let script = node.script.as_ref()?;
                    let path =
                        resolve_script(&script.path, &self.scene_dir, self.project_dir.as_deref())?;
                    Some((node.id.clone(), path, script.class_name.clone()))
                })
                .collect()
        });
        for (id, path, class) in wanted {
            if let Some(mut slot) = olds.remove(&id.to_string()) {
                if slot.file != path {
                    slot.file = path;
                    if let Err(e) = slot.backend.load(&slot.file.to_string_lossy(), &class) {
                        tracing::error!(node = %id, "cannot reload script: {e:#}");
                        slot.errored = true;
                    }
                }
                let seeds = self.host.with_tree(|tree| {
                    (
                        seed_position(tree, &id),
                        seed_text(tree, &id),
                        seed_size(tree, &id),
                        seed_wait(tree, &id),
                    )
                });
                if let Some((x, y)) = seeds.0 {
                    slot.backend.set_position(x, y);
                }
                if let Some(text) = seeds.1 {
                    slot.backend.set_text(&text);
                }
                if let Some((w, h)) = seeds.2 {
                    slot.backend.set_size(w, h);
                }
                if let Some(wait) = seeds.3 {
                    slot.backend.set_wait_time(wait);
                }
                self.slots.push(slot);
                continue;
            }
            if !path.is_file() {
                tracing::warn!(node = %id, script = %path.display(), "missing script, keeping placeholder node");
                continue;
            }
            let mut backend = Box::new(Pyo3Backend::new(id.to_string()));
            backend.set_host(self.host.clone());
            match backend.load(&path.to_string_lossy(), &class) {
                Ok(()) => {
                    let seeds = self.host.with_tree(|tree| {
                        (
                            seed_position(tree, &id),
                            seed_text(tree, &id),
                            seed_size(tree, &id),
                            seed_wait(tree, &id),
                        )
                    });
                    if let Some((x, y)) = seeds.0 {
                        backend.set_position(x, y);
                    }
                    if let Some(text) = seeds.1 {
                        backend.set_text(&text);
                    }
                    if let Some((w, h)) = seeds.2 {
                        backend.set_size(w, h);
                    }
                    if let Some(wait) = seeds.3 {
                        backend.set_wait_time(wait);
                    }
                    let mut errored = false;
                    if let Err(e) = backend.call_ready() {
                        tracing::error!(node = %id, "{e:#}");
                        errored = true;
                    }
                    self.slots.push(ScriptSlot {
                        id,
                        file: path,
                        backend,
                        errored,
                    });
                }
                Err(e) => {
                    tracing::error!(node = %id, "cannot load script: {e:#}");
                }
            }
        }
        for (id, _) in olds {
            self.host.unregister(&id);
            self.host.disconnect_node(&id);
        }
        tracing::info!(
            nodes = self.tree_len(),
            scripts = self.slots.len(),
            "scene ready"
        );
        Ok(())
    }

    fn start_watcher(&mut self) -> Result<()> {
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| self.scene_dir.clone());
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        self._watcher = Some(watcher);
        self.watch_rx = Some(rx);
        tracing::info!(watch = %root.display(), "hot reload watching");
        Ok(())
    }

    pub fn poll_watch(&mut self) {
        if self.no_reload {
            return;
        }
        let mut script_touched: HashSet<PathBuf> = HashSet::new();
        let mut scene_touched = false;
        if let Some(rx) = &self.watch_rx {
            while let Ok(result) = rx.try_recv() {
                let Ok(event) = result else { continue };
                if !matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    continue;
                }
                for path in event.paths {
                    match path.extension().and_then(|e| e.to_str()) {
                        Some("py") => {
                            script_touched.insert(path);
                        }
                        Some("ptscn") => {
                            scene_touched = true;
                        }
                        _ => {}
                    }
                }
            }
        }
        for path in script_touched {
            self.reload_script_path(&path);
        }
        if scene_touched {
            match self.rebuild() {
                Ok(()) => tracing::info!("scene reloaded"),
                Err(e) => tracing::error!("scene reload failed, keeping old tree: {e:#}"),
            }
        }
    }

    pub fn rebuild_physics(&mut self) {
        let mut world = PhysicsWorld::new();
        self.host.with_tree(|tree| {
            // 1. Add bodies & areas
            for node in tree.iter() {
                let pos = global_position(tree, &node.id);
                let rot = match node.props.get("rotation") {
                    Some(PropValue::Num(r)) => *r as f32,
                    _ => 0.0,
                };
                match node.type_name.as_str() {
                    "StaticBody2D" => {
                        world.add_static_body(node.id.clone(), (pos.0 as f32, pos.1 as f32), rot);
                    }
                    "CharacterBody2D" => {
                        world.add_character_body(
                            node.id.clone(),
                            (pos.0 as f32, pos.1 as f32),
                            rot,
                        );
                    }
                    "RigidBody2D" => {
                        let mass = match node.props.get("mass") {
                            Some(PropValue::Num(m)) => *m as f32,
                            _ => 1.0,
                        };
                        let grav = match node.props.get("gravity_scale") {
                            Some(PropValue::Num(g)) => *g as f32,
                            _ => 1.0,
                        };
                        let lock_rot = match node.props.get("lock_rotation") {
                            Some(PropValue::Bool(b)) => *b,
                            _ => false,
                        };
                        world.add_rigid_body(
                            node.id.clone(),
                            (pos.0 as f32, pos.1 as f32),
                            rot,
                            mass,
                            grav,
                            lock_rot,
                        );
                    }
                    "Area2D" => {
                        world.add_area(node.id.clone(), (pos.0 as f32, pos.1 as f32), rot);
                    }
                    _ => {}
                }
            }

            // 2. Attach collision shapes
            for node in tree.iter().filter(|n| n.type_name == "CollisionShape2D") {
                let disabled = match node.props.get("disabled") {
                    Some(PropValue::Bool(b)) => *b,
                    _ => false,
                };
                if disabled {
                    continue;
                }

                // Find parent body
                let mut parent_id = node.parent.clone();
                let mut parent_body: Option<&pite_core::Node> = None;
                while let Some(pid) = parent_id {
                    if let Some(pnode) = tree.get(&pid) {
                        if matches!(
                            pnode.type_name.as_str(),
                            "StaticBody2D" | "CharacterBody2D" | "RigidBody2D" | "Area2D"
                        ) {
                            parent_body = Some(pnode);
                            break;
                        }
                        parent_id = pnode.parent.clone();
                    } else {
                        break;
                    }
                }

                let Some(body) = parent_body else {
                    continue;
                };

                let body_pos = global_position(tree, &body.id);
                let shape_pos = global_position(tree, &node.id);
                let offset = (
                    (shape_pos.0 - body_pos.0) as f32,
                    (shape_pos.1 - body_pos.1) as f32,
                );

                let shape_rot = match node.props.get("rotation") {
                    Some(PropValue::Num(r)) => *r as f32,
                    _ => 0.0,
                };

                let shape_type = match node.props.get("shape") {
                    Some(PropValue::Str(s)) => s.as_str(),
                    _ => "box",
                };

                let config = match shape_type {
                    "circle" => {
                        let radius = match node.props.get("radius") {
                            Some(PropValue::Num(r)) => *r as f32,
                            _ => 16.0,
                        };
                        ShapeConfig::Circle { radius }
                    }
                    "capsule" => {
                        let radius = match node.props.get("radius") {
                            Some(PropValue::Num(r)) => *r as f32,
                            _ => 16.0,
                        };
                        let height = match node.props.get("height") {
                            Some(PropValue::Num(h)) => *h as f32,
                            _ => 32.0,
                        };
                        ShapeConfig::Capsule { radius, height }
                    }
                    _ => {
                        let (w, h) = match node.props.get("size") {
                            Some(PropValue::Vec2(w, h)) => (*w as f32, *h as f32),
                            _ => (32.0, 32.0),
                        };
                        ShapeConfig::Box {
                            width: w,
                            height: h,
                        }
                    }
                };

                let layer = match body.props.get("collision_layer") {
                    Some(PropValue::Int(l)) => *l as u32,
                    _ => 1,
                };
                let mask = match body.props.get("collision_mask") {
                    Some(PropValue::Int(m)) => *m as u32,
                    _ => 1,
                };
                let friction = match body.props.get("friction") {
                    Some(PropValue::Num(f)) => *f as f32,
                    _ => 0.5,
                };
                let restitution = match body.props.get("restitution") {
                    Some(PropValue::Num(r)) => *r as f32,
                    _ => 0.0,
                };

                let is_sensor = body.type_name == "Area2D";

                world.attach_shape(
                    &body.id,
                    node.id.clone(),
                    config,
                    offset,
                    shape_rot,
                    is_sensor,
                    layer,
                    mask,
                    friction,
                    restitution,
                );
            }
        });

        *self.physics.lock().unwrap() = world;
        pite_script::set_current_physics(Some(self.physics.clone()));
    }

    pub fn update(&mut self, delta: f64) {
        self.poll_buttons();
        pite_audio::poll_audio();

        // 60 Hz fixed physics stepping
        const FIXED_STEP: f64 = 1.0 / 60.0;
        self.physics_accumulator += delta;
        if self.physics_accumulator > 0.25 {
            self.physics_accumulator = 0.25;
        }

        while self.physics_accumulator >= FIXED_STEP {
            for slot in &mut self.slots {
                if slot.errored {
                    continue;
                }
                if let Err(e) = slot.backend.call_physics_process(FIXED_STEP) {
                    tracing::error!(node = %slot.id, "physics process error: {e:#}");
                    slot.errored = true;
                    if self.pause_on_error && self.last_error.is_none() {
                        self.last_error = slot.backend.last_script_error();
                    }
                }
                if let Some((x, y)) = slot.backend.position() {
                    let mut lock = self.physics.lock().unwrap();
                    lock.set_body_position(&slot.id, (x, y));
                }
            }

            let events = {
                let mut lock = self.physics.lock().unwrap();
                lock.step();
                lock.drain_events()
            };

            // Sync dynamic physics positions (RigidBody2D) back into tree & slots
            {
                let lock = self.physics.lock().unwrap();
                for slot in &mut self.slots {
                    if lock.is_rigid_body(&slot.id) {
                        if let Some(pos) = lock.get_body_position(&slot.id) {
                            slot.backend.set_position(pos.0, pos.1);
                            self.host.with_tree_mut(|tree| {
                                if let Some(node) = tree.get_mut(&slot.id) {
                                    node.props.insert(
                                        "position".to_string(),
                                        PropValue::Vec2(pos.0, pos.1),
                                    );
                                }
                            });
                        }
                    }
                }
            }

            // Dispatch Area2D signals
            for ev in events {
                match ev {
                    PhysicsEvent::BodyEntered { area_id, body_id } => {
                        let _ = pite_script::fire_body_entered(
                            &self.host,
                            area_id.as_str(),
                            body_id.as_str(),
                        );
                    }
                    PhysicsEvent::BodyExited { area_id, body_id } => {
                        let _ = pite_script::fire_body_exited(
                            &self.host,
                            area_id.as_str(),
                            body_id.as_str(),
                        );
                    }
                    PhysicsEvent::AreaEntered {
                        area_id,
                        other_area_id,
                    } => {
                        let _ = pite_script::fire_area_entered(
                            &self.host,
                            area_id.as_str(),
                            other_area_id.as_str(),
                        );
                    }
                    PhysicsEvent::AreaExited {
                        area_id,
                        other_area_id,
                    } => {
                        let _ = pite_script::fire_area_exited(
                            &self.host,
                            area_id.as_str(),
                            other_area_id.as_str(),
                        );
                    }
                }
            }

            self.physics_accumulator -= FIXED_STEP;
        }

        for slot in &mut self.slots {
            if slot.errored {
                continue;
            }
            if let Err(e) = slot.backend.call_process(delta) {
                tracing::error!(node = %slot.id, "{e:#}");
                slot.errored = true;
                if self.pause_on_error && self.last_error.is_none() {
                    self.last_error = slot.backend.last_script_error();
                }
            }
            if let Some(wait) = slot.backend.wait_time() {
                self.host.with_tree_mut(|tree| {
                    if let Some(node) = tree.get_mut(&slot.id) {
                        node.props
                            .insert("wait_time".to_string(), PropValue::Num(wait));
                    }
                });
            }
        }
        self.tick_timers(delta);
    }

    /// Advance every Timer node; fire `timeout` on each elapsed `wait_time`
    /// and restart the clock (looping). Non-positive waits never fire.
    fn tick_timers(&mut self, delta: f64) {
        let timers: Vec<(String, f64)> = self.host.with_tree(|tree| {
            tree.iter()
                .filter(|n| n.type_name == "Timer")
                .filter_map(|n| {
                    let wait = match n.props.get("wait_time") {
                        Some(PropValue::Num(f)) => *f,
                        Some(PropValue::Int(i)) => *i as f64,
                        _ => return None,
                    };
                    (wait > 0.0).then(|| (n.id.to_string(), wait))
                })
                .collect()
        });
        for (id, wait) in &timers {
            let fire = {
                let elapsed = self.timers.entry(id.clone()).or_insert(0.0);
                *elapsed += delta;
                if *elapsed >= *wait {
                    *elapsed = 0.0;
                    true
                } else {
                    false
                }
            };
            if fire {
                self.fire_timeout(id);
            }
        }
        let live: std::collections::HashSet<String> =
            timers.into_iter().map(|(id, _)| id).collect();
        self.timers.retain(|id, _| live.contains(id));
    }

    fn poll_buttons(&mut self) {
        let mouse = input_mouse();
        let hovered = self.button_at(mouse);
        if input_pressed("MouseLeft") {
            if let Some(id) = hovered.clone() {
                self.armed.insert(id);
            }
        }
        if input_released("MouseLeft") {
            match hovered {
                Some(id) if self.armed.contains(&id) => self.fire_pressed(&id),
                _ => {}
            }
            self.armed.clear();
        }
    }

    /// One frame: pump watchers, run scripts, then retire edge input.
    ///
    /// `input_begin_frame()` MUST come last. OS/window events arrive
    /// between frames, so clearing edges before `update()` wipes presses
    /// and releases scripts have not consumed yet (`pite.pressed()` would
    /// read false forever and buttons would never arm). Edges set since
    /// the previous frame stay visible for exactly one `update()`.
    pub fn frame(&mut self, delta: f64) {
        self.poll_watch();
        self.update(delta);
        pite_script::input_begin_frame();
    }
}

fn button_size(node: &pite_core::Node) -> (f64, f64) {
    match node.props.get("size") {
        Some(PropValue::Vec2(w, h)) => (*w, *h),
        _ => (120.0, 40.0),
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<WgpuRenderer>,
    title: String,
    session: GameSession,
    last_frame: Instant,
    render_errors: u32,
}

fn window_icon() -> Option<winit::window::Icon> {
    const PNG: &[u8] = include_bytes!("../../assets/icon.png");
    let img = image::load_from_memory_with_format(PNG, image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    winit::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let mut attrs = Window::default_attributes().with_title(self.title.clone());
            // Maximized like the editor; the design size is the restore size.
            attrs = attrs.with_maximized(true);
            if let Some((w, h)) = self.session.design_size() {
                attrs = attrs.with_inner_size(winit::dpi::PhysicalSize::new(w, h));
            }
            if let Some(icon) = window_icon() {
                attrs = attrs.with_window_icon(Some(icon));
            }
            match event_loop.create_window(attrs) {
                Ok(window) => {
                    let window = Arc::new(window);
                    match WgpuRenderer::new(window.clone()) {
                        Ok(renderer) => {
                            self.window = Some(window);
                            self.renderer = Some(renderer);
                        }
                        Err(e) => {
                            tracing::error!("cannot init renderer: {e:#}");
                            event_loop.exit();
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("cannot create window: {e}");
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if let Some(name) = key_name(code) {
                        pite_script::input_set_key(name, event.state == ElementState::Pressed);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Physical pixels: the renderer's viewport (`inner_size`)
                // is physical too, and `button_at` maps the cursor through
                // it. Dividing by the scale factor here once made every
                // click miss on scaled displays while the button drew fine.
                pite_script::input_set_mouse(position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let name = match button {
                    MouseButton::Left => "MouseLeft",
                    MouseButton::Right => "MouseRight",
                    MouseButton::Middle => "MouseMiddle",
                    _ => return,
                };
                pite_script::input_set_key(name, state == ElementState::Pressed);
            }
            WindowEvent::RedrawRequested => {
                let delta = self.session_delta();
                if let Some(renderer) = &self.renderer {
                    let (w, h) = renderer.size();
                    self.session.set_viewport(w, h);
                }
                self.session.frame(delta);
                self.render_frame();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

impl App {
    fn render_frame(&mut self) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let result = (|| -> Result<()> {
            renderer.begin_frame()?;
            self.session.draw_into(renderer)?;
            renderer.end_frame()?;
            Ok(())
        })();
        if let Err(e) = result {
            self.render_errors += 1;
            if self.render_errors <= 3 {
                tracing::error!("render frame failed: {e:#}");
            }
        }
    }
}

impl App {
    fn session_delta(&mut self) -> f64 {
        let delta = self.last_frame.elapsed().as_secs_f64().min(0.1);
        self.last_frame = Instant::now();
        delta
    }
}

/// Resolve a scene argument to an absolute path. `res://` resolves against
/// the enclosing project root, everything else against the current dir.
/// Shared by `pite run` and `pite-player` so both accept the same paths.
pub fn resolve_scene_arg(scene: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    if let Some(rel) = scene.strip_prefix("res://") {
        let root = pite_project::find_project_root(&cwd)
            .with_context(|| "scene uses res:// but no pite.toml found above cwd")?;
        return Ok(root.join(rel));
    }
    let p = PathBuf::from(scene);
    Ok(if p.is_absolute() { p } else { cwd.join(p) })
}

/// Default scene when scene argument is omitted: the enclosing project's
/// `main_scene`. Same rule `pite edit` uses with no scene; outside a
/// project it fails loudly instead of guessing.
pub fn resolve_default_scene() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let root = pite_project::find_project_root(&cwd)
        .with_context(|| "no scene given and no pite.toml found above cwd")?;
    let main = pite_project::load_manifest(&root)?.project.main_scene;
    Ok(match main.strip_prefix("res://") {
        Some(rel) => root.join(rel),
        None => root.join(main),
    })
}

pub fn run_scene(scene: &Path, no_reload: bool) -> Result<()> {
    run_with_options(&RunOptions {
        scene: scene.to_path_buf(),
        no_reload,
    })
}

pub fn run_with_options(options: &RunOptions) -> Result<()> {
    let session = GameSession::open(&options.scene, options.no_reload)?;
    let title = format!(
        "Pite — {} ({} nodes)",
        options.scene.display(),
        session.tree_len()
    );
    tracing::info!(
        scene = %options.scene.display(),
        nodes = session.tree_len(),
        scripts = session.script_count(),
        no_reload = options.no_reload,
        "scene loaded"
    );

    let event_loop = EventLoop::new().context("cannot create event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        window: None,
        renderer: None,
        title,
        session,
        last_frame: Instant::now(),
        render_errors: 0,
    };
    event_loop.run_app(&mut app).context("event loop failed")?;
    Ok(())
}

fn seed_position(tree: &NodeTree, id: &NodeId) -> Option<(f64, f64)> {
    match tree.get(id)?.props.get("position") {
        Some(PropValue::Vec2(x, y)) => Some((*x, *y)),
        _ => None,
    }
}

fn seed_text(tree: &NodeTree, id: &NodeId) -> Option<String> {
    match tree.get(id)?.props.get("text") {
        Some(PropValue::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn seed_wait(tree: &NodeTree, id: &NodeId) -> Option<f64> {
    match tree.get(id)?.props.get("wait_time") {
        Some(PropValue::Num(f)) => Some(*f),
        Some(PropValue::Int(i)) => Some(*i as f64),
        _ => None,
    }
}

fn seed_size(tree: &NodeTree, id: &NodeId) -> Option<(f64, f64)> {
    match tree.get(id)?.props.get("size") {
        Some(PropValue::Vec2(w, h)) => Some((*w, *h)),
        _ => None,
    }
}

fn resolve_script(
    script_ref: &str,
    scene_dir: &Path,
    project_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(rel) = script_ref.strip_prefix("res://") {
        project_dir.map(|root| root.join(rel))
    } else {
        let p = PathBuf::from(script_ref);
        if p.is_absolute() {
            Some(p)
        } else {
            Some(scene_dir.join(p))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn make_project(tag: &str, class: &str, script_src: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("pite-sess-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            format!("format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"mover\"\ntype = \"Sprite2D\"\nname = \"Mover\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/mover.py\"\nclass = \"{class}\"\n"),
        )
        .unwrap();
        let script = dir.join("scripts").join("mover.py");
        std::fs::write(&script, script_src).unwrap();
        (dir, script)
    }

    const V1: &str = "import pite\n\nclass Mover(pite.Node2D):\n    def _process(self, delta):\n        x, y = self.position\n        self.position = (x + 100.0 * delta, y)\n";
    const V2: &str = "import pite\n\nclass Mover(pite.Node2D):\n    def _process(self, delta):\n        x, y = self.position\n        self.position = (x + 400.0 * delta, y)\n";

    fn drive_x(session: &mut GameSession) -> f64 {
        session.update(0.1);
        session
            .host
            .with_tree(|t| match t.get(&NodeId::from("mover".to_string())) {
                Some(n) => match n.props.get("position") {
                    Some(PropValue::Vec2(x, _)) => *x,
                    _ => f64::NAN,
                },
                None => f64::NAN,
            })
    }

    #[test]
    fn editing_script_changes_behavior_without_restart() {
        let _guard = SERIAL.lock().unwrap();
        let (dir, script) = make_project("reload", "Mover", V1);
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, false).unwrap();
        assert_eq!(session.script_count(), 1);
        let x1 = drive_x(&mut session);
        assert!((x1 - 10.0).abs() < 1e-6);

        std::fs::write(&script, V2).unwrap();
        let mut reloaded = false;
        for _ in 0..100 {
            session.poll_watch();
            let before = drive_x(&mut session);
            let after = drive_x(&mut session);
            if (after - before - 40.0).abs() < 1e-6 {
                reloaded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(reloaded, "v2 behavior never took effect");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scene_edit_adds_nodes_live() {
        let _guard = SERIAL.lock().unwrap();
        let (dir, _) = make_project("scene", "Mover", V1);
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, false).unwrap();
        assert_eq!(session.tree_len(), 2);
        std::fs::write(
            &scene,
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"extra\"\ntype = \"Timer\"\nname = \"Extra\"\nparent = \"root\"\n",
        )
        .unwrap();
        let mut grown = false;
        for _ in 0..100 {
            session.poll_watch();
            if session.tree_len() == 2 && session.script_count() == 0 {
                grown = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(grown, "rebuilt tree never applied");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_reload_disables_watching() {
        let _guard = SERIAL.lock().unwrap();
        let (dir, _) = make_project("noreload", "Mover", V1);
        let scene = dir.join("scenes").join("main.ptscn");
        let session = GameSession::open(&scene, true).unwrap();
        assert!(session.watch_rx.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    const HUD: &str = "import pite\n\nclass Hud(pite.Label):\n    def _process(self, delta):\n        self.text = \"HP: 2\"\n";
    fn hud_text(session: &GameSession) -> String {
        session
            .host
            .with_tree(|t| match t.get(&NodeId::from("hud".to_string())) {
                Some(n) => match n.props.get("text") {
                    Some(PropValue::Str(s)) => s.clone(),
                    _ => String::new(),
                },
                None => String::new(),
            })
    }

    #[test]
    fn script_text_reaches_tree_and_renderer() {
        let _guard = SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pite-sess-hud-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"hud\"\ntype = \"Label\"\nname = \"Hud\"\nparent = \"root\"\n\n[node.props]\ntext = \"HP: 3\"\nfont_size = 20.0\n\n[node.script]\npath = \"res://scripts/hud.py\"\nclass = \"Hud\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("scripts").join("hud.py"), HUD).unwrap();
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        assert_eq!(hud_text(&session), "HP: 3");
        session.update(0.016);
        assert_eq!(hud_text(&session), "HP: 2");
        std::fs::remove_dir_all(&dir).ok();
    }

    const PINGER: &str = "import pite\n\nclass Pinger(pite.Node2D):\n    ping = pite.signal(int)\n    def _process(self, delta):\n        if not getattr(self, \"_sent\", False):\n            self._sent = True\n            self.ping.emit(1)\n";
    const PONGER: &str = "import pite\n\nclass Ponger(pite.Node2D):\n    def _ready(self):\n        self.get_node(\"../Pinger\").ping.connect(self.on_ping)\n    def on_ping(self, n):\n        self.get_node(\"../Hud\").text = \"hit!\"\n";

    #[test]
    fn signal_reaches_label_through_proxy() {
        let _guard = SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pite-sess-siglbl-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"pinger\"\ntype = \"Node2D\"\nname = \"Pinger\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/pinger.py\"\nclass = \"Pinger\"\n\n[[node]]\nid = \"ponger\"\ntype = \"Node2D\"\nname = \"Ponger\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/ponger.py\"\nclass = \"Ponger\"\n\n[[node]]\nid = \"hud\"\ntype = \"Label\"\nname = \"Hud\"\nparent = \"root\"\n\n[node.props]\ntext = \"waiting\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("scripts").join("pinger.py"), PINGER).unwrap();
        std::fs::write(dir.join("scripts").join("ponger.py"), PONGER).unwrap();
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        assert_eq!(hud_text(&session), "waiting");
        session.update(0.016);
        assert_eq!(hud_text(&session), "hit!");
        std::fs::remove_dir_all(&dir).ok();
    }

    const TICKER: &str = "import pite\n\nclass Ticker(pite.Node):\n    def _ready(self):\n        self.get_node(\".\").timeout.connect(self.on_timeout)\n    def on_timeout(self):\n        hud = self.get_node(\"../Hud\")\n        hud.text = str(int(hud.text) + 1)\n";

    fn make_timer_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-sess-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"tick\"\ntype = \"Timer\"\nname = \"Tick\"\nparent = \"root\"\n\n[node.props]\nwait_time = 0.05\n\n[node.script]\npath = \"res://scripts/ticker.py\"\nclass = \"Ticker\"\n\n[[node]]\nid = \"hud\"\ntype = \"Label\"\nname = \"Hud\"\nparent = \"root\"\n\n[node.props]\ntext = \"0\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("scripts").join("ticker.py"), TICKER).unwrap();
        dir
    }

    #[test]
    fn timer_fires_and_loops() {
        let _guard = SERIAL.lock().unwrap();
        let dir = make_timer_project("timer-loop");
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        assert_eq!(hud_text(&session), "0");
        for _ in 0..3 {
            session.update(0.016);
        }
        assert_eq!(hud_text(&session), "0");
        session.update(0.016);
        assert_eq!(hud_text(&session), "1");
        for _ in 0..4 {
            session.update(0.016);
        }
        assert_eq!(hud_text(&session), "2");
        std::fs::remove_dir_all(&dir).ok();
    }

    const COUNTER: &str = "import pite\n\nclass Counter(pite.Button):\n    pressed = pite.signal()\n    def _ready(self):\n        self.text = \"0\"\n        self.get_node(\".\").pressed.connect(self.on_pressed)\n    def on_pressed(self):\n        self.text = str(int(self.text) + 1)\n";

    const COUNTER_BUILTIN: &str = "import pite\n\nclass Counter(pite.Button):\n    def _ready(self):\n        self.text = \"0\"\n        self.get_node(\".\").pressed.connect(self.on_pressed)\n    def on_pressed(self):\n        self.text = str(int(self.text) + 1)\n";

    #[test]
    fn vendored_import_resolves_in_session() {
        let _guard = SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pite-sess-vendor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::create_dir_all(dir.join("vendor").join("vpkg")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("vendor").join("vpkg").join("__init__.py"),
            "VALUE = 7\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"m\"\ntype = \"Label\"\nname = \"M\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/m.py\"\nclass = \"M\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("scripts").join("m.py"),
            "import pite\nimport vpkg\n\nclass M(pite.Label):\n    def _ready(self):\n        self.text = str(vpkg.VALUE)\n",
        )
        .unwrap();
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        session.update(0.016);
        assert!(session.errors().is_empty(), "got: {:?}", session.errors());
        let text = session
            .host
            .with_tree(|t| match t.get(&NodeId::from("m".to_string())) {
                Some(n) => match n.props.get("text") {
                    Some(PropValue::Str(s)) => s.clone(),
                    _ => String::new(),
                },
                None => String::from("<gone>"),
            });
        assert_eq!(text, "7");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn zipped_wheel_import_resolves_in_session() {
        let _guard = SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pite-sess-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::create_dir_all(dir.join("vendor")).unwrap();
        let crafted = std::process::Command::new("python3")
            .arg("-c")
            .arg(
                "import sys, zipfile; z = zipfile.ZipFile(sys.argv[1], 'w'); z.writestr('zpkg/__init__.py', 'VALUE = 9\\n'); z.close()",
            )
            .arg(dir.join("vendor").join("zpkg-1.0-py3-none-any.whl"))
            .output();
        if !crafted.as_ref().is_ok_and(|o| o.status.success()) {
            eprintln!("skipped: python3 with zipfile unavailable");
            return;
        }
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"m\"\ntype = \"Label\"\nname = \"M\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/m.py\"\nclass = \"M\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("scripts").join("m.py"),
            "import pite\nimport zpkg\n\nclass M(pite.Label):\n    def _ready(self):\n        self.text = str(zpkg.VALUE)\n",
        )
        .unwrap();
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        session.update(0.016);
        assert!(session.errors().is_empty(), "got: {:?}", session.errors());
        let text = session
            .host
            .with_tree(|t| match t.get(&NodeId::from("m".to_string())) {
                Some(n) => match n.props.get("text") {
                    Some(PropValue::Str(s)) => s.clone(),
                    _ => String::new(),
                },
                None => String::from("<gone>"),
            });
        assert_eq!(text, "9");
        std::fs::remove_dir_all(&dir).ok();
    }

    fn make_button_project(tag: &str) -> PathBuf {
        make_button_project_with(tag, COUNTER)
    }

    fn make_button_project_with(tag: &str, script_src: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-sess-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"btn\"\ntype = \"Button\"\nname = \"HitBtn\"\nparent = \"root\"\n\n[node.props]\nposition = [100.0, 100.0]\nsize = [120.0, 40.0]\ntext = \"0\"\n\n[node.script]\npath = \"res://scripts/counter.py\"\nclass = \"Counter\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("scripts").join("counter.py"), script_src).unwrap();
        dir
    }

    fn btn_text(session: &GameSession) -> String {
        session
            .host
            .with_tree(|t| match t.get(&NodeId::from("btn".to_string())) {
                Some(n) => match n.props.get("text") {
                    Some(PropValue::Str(s)) => s.clone(),
                    _ => String::new(),
                },
                None => String::from("<gone>"),
            })
    }

    fn click(session: &mut GameSession, down: (f64, f64), up: (f64, f64)) {
        use pite_script::{input_begin_frame, input_set_key, input_set_mouse};
        let (w, h) = session.design_size().unwrap_or((800, 600));
        session.set_viewport(w, h);
        input_set_mouse(down.0, down.1);
        input_set_key("MouseLeft", true);
        session.update(0.016);
        input_begin_frame();
        input_set_mouse(up.0, up.1);
        input_set_key("MouseLeft", false);
        session.update(0.016);
        input_begin_frame();
    }

    #[test]
    fn click_inside_fires_once_per_click() {
        let _guard = SERIAL.lock().unwrap();
        let dir = make_button_project("btn-once");
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        assert_eq!(btn_text(&session), "0");
        click(&mut session, (740.0, 460.0), (740.0, 460.0));
        assert_eq!(btn_text(&session), "1");
        click(&mut session, (740.0, 460.0), (740.0, 460.0));
        assert_eq!(btn_text(&session), "2");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn button_pressed_needs_no_script_declaration() {
        let _guard = SERIAL.lock().unwrap();
        let dir = make_button_project_with("btn-builtin", COUNTER_BUILTIN);
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        assert!(session.host.has_signal("btn", "pressed"));
        assert_eq!(btn_text(&session), "0");
        click(&mut session, (740.0, 460.0), (740.0, 460.0));
        assert_eq!(btn_text(&session), "1");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn click_outside_is_ignored() {
        let _guard = SERIAL.lock().unwrap();
        let dir = make_button_project("btn-out");
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        // Screen points for the 1280x720 design center (640, 360): the
        // fixture button sits at world (100, 100), i.e. screen (740, 460).
        click(&mut session, (100.0, 100.0), (100.0, 100.0));
        assert_eq!(btn_text(&session), "0");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn press_inside_release_outside_is_ignored() {
        let _guard = SERIAL.lock().unwrap();
        let dir = make_button_project("btn-drag");
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        click(&mut session, (740.0, 460.0), (100.0, 100.0));
        assert_eq!(btn_text(&session), "0");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dropped_button_disconnects_silently() {
        let _guard = SERIAL.lock().unwrap();
        let dir = make_button_project("btn-drop");
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        session.host.with_tree_mut(|t| {
            t.remove(&NodeId::from("btn".to_string())).unwrap();
        });
        session.host.disconnect_node("btn");
        assert!(!session.host.has_signal("btn", "pressed"));
        click(&mut session, (740.0, 460.0), (740.0, 460.0));
        assert_eq!(btn_text(&session), "<gone>");
        std::fs::remove_dir_all(&dir).ok();
    }

    const WALKER: &str = "import pite\n\nclass Walker(pite.Node2D):\n    def _process(self, delta):\n        if pite.held.right:\n            self.position.x += 200.0 * delta\n        if pite.pressed.space:\n            self.position.x += 10.0\n";

    fn walker_x(session: &GameSession) -> f64 {
        session
            .host
            .with_tree(|t| match t.get(&NodeId::from("mover".to_string())) {
                Some(n) => match n.props.get("position") {
                    Some(PropValue::Vec2(x, _)) => *x,
                    _ => f64::NAN,
                },
                None => f64::NAN,
            })
    }

    #[test]
    fn keypress_moves_node_through_real_input() {
        let _guard = SERIAL.lock().unwrap();
        use pite_script::{input_begin_frame, input_set_key};

        let (dir, _) = make_project("input", "Walker", WALKER);
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();

        input_begin_frame();
        input_set_key("ArrowRight", true);
        session.update(0.5);
        assert!((walker_x(&session) - 100.0).abs() < 1e-6);

        input_begin_frame();
        input_set_key("Space", true);
        session.update(0.5);
        let after_step = walker_x(&session);
        assert!((after_step - 210.0).abs() < 1e-6);

        input_begin_frame();
        session.update(0.5);
        assert!((walker_x(&session) - 310.0).abs() < 1e-6);
        input_set_key("ArrowRight", false);
        input_set_key("Space", false);
        std::fs::remove_dir_all(&dir).ok();
    }

    const STEPPER: &str = "import pite\n\nclass Walker(pite.Node2D):\n    def _process(self, delta):\n        if pite.pressed.space:\n            self.position.x += 10.0\n";

    #[test]
    fn frame_delivers_production_order_edges_exactly_once() {
        let _guard = SERIAL.lock().unwrap();
        use pite_script::input_set_key;

        let (dir, _) = make_project("edgeorder", "Walker", STEPPER);
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();

        // Production order: the OS event lands between frames, so the
        // edge is set BEFORE frame() with no begin_frame in between.
        // (frame() itself retires edges at its end.)
        input_set_key("Space", true);
        session.frame(0.5);
        assert!((walker_x(&session) - 10.0).abs() < 1e-6);
        session.frame(0.5);
        assert!(
            (walker_x(&session) - 10.0).abs() < 1e-6,
            "edge must be consumed exactly once"
        );
        input_set_key("Space", false);
        pite_script::input_begin_frame();
        std::fs::remove_dir_all(&dir).ok();
    }

    fn node_x(session: &GameSession, id: &str) -> f64 {
        session
            .host
            .with_tree(|t| match t.get(&NodeId::from(id.to_string())) {
                Some(n) => match n.props.get("position") {
                    Some(PropValue::Vec2(x, _)) => *x,
                    _ => f64::NAN,
                },
                None => f64::NAN,
            })
    }

    /// Records what the draw path actually queued.
    #[derive(Default)]
    struct RecordingRenderer {
        calls: Vec<(String, Option<String>, (f64, f64))>,
    }

    impl Renderer2D for RecordingRenderer {
        fn begin_frame(&mut self) -> Result<()> {
            Ok(())
        }

        fn draw_sprite(&mut self, texture: &str, x: f64, y: f64) -> Result<()> {
            self.calls.push((texture.to_string(), None, (x, y)));
            Ok(())
        }

        fn draw_sprite_frame(
            &mut self,
            texture: &str,
            frame: Option<&str>,
            x: f64,
            y: f64,
        ) -> Result<()> {
            self.calls
                .push((texture.to_string(), frame.map(str::to_string), (x, y)));
            Ok(())
        }

        fn draw_text(
            &mut self,
            _text: &str,
            _x: f64,
            _y: f64,
            _size: f32,
            _color: [u8; 4],
        ) -> Result<()> {
            Ok(())
        }

        fn draw_rect(&mut self, _x: f64, _y: f64, _w: f64, _h: f64, _color: [u8; 4]) -> Result<()> {
            Ok(())
        }

        fn end_frame(&mut self) -> Result<()> {
            Ok(())
        }
    }

    fn dogfood_scene() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("examples")
            .join("minimal-2d")
            .join("scenes")
            .join("main.ptscn")
    }

    #[test]
    fn dogfood_sprites_draw_from_one_sheet() {
        let _guard = SERIAL.lock().unwrap();
        let mut session = GameSession::open(&dogfood_scene(), true).unwrap();
        let mut recorder = RecordingRenderer::default();
        session.draw_into(&mut recorder).unwrap();

        let sheet = session
            .resolve_path("res://assets/sheet.atlas.json")
            .to_string_lossy()
            .into_owned();
        let sheet = PathBuf::from(&sheet)
            .with_file_name("sheet.png")
            .to_string_lossy()
            .into_owned();
        let frames: Vec<Option<&str>> = recorder
            .calls
            .iter()
            .filter(|(tex, _, _)| tex == &sheet)
            .map(|(_, frame, _)| frame.as_deref())
            .collect();
        assert!(
            frames.contains(&Some("player")),
            "player must draw from the sheet, got {:?}",
            recorder.calls
        );
        assert!(
            frames.contains(&Some("enemy")),
            "instanced enemy must draw from the sheet, got {:?}",
            recorder.calls
        );
        let sheets: std::collections::HashSet<&String> =
            recorder.calls.iter().map(|(tex, _, _)| tex).collect();
        assert_eq!(
            sheets.len(),
            1,
            "all frames share one texture, so they batch into one draw"
        );
    }

    #[test]
    fn default_scene_resolves_main_scene_and_fails_outside_projects() {
        let _guard = SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pite-defscene-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::write(
            dir.join("pite.toml"),
            "[project]\nname = \"t\"\nmain_scene = \"res://scenes/main.ptscn\"\n",
        )
        .unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let got = resolve_default_scene().unwrap();
        assert_eq!(got, dir.join("scenes").join("main.ptscn"));
        let bare = std::env::temp_dir().join(format!("pite-noproj-{}", std::process::id()));
        std::fs::create_dir_all(&bare).unwrap();
        std::env::set_current_dir(&bare).unwrap();
        let err = resolve_default_scene().expect_err("outside a project must fail loudly");
        assert!(err.to_string().contains("pite.toml"), "got {err:#}");
        std::env::set_current_dir(&prev).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&bare).ok();
    }

    #[test]
    fn texture_and_atlas_together_fail_loudly() {
        let _guard = SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pite-atlas-draw-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"both\"\ntype = \"Sprite2D\"\nname = \"Both\"\nparent = \"root\"\n\n[node.props]\ntexture = \"res://a.png\"\natlas = \"res://sheet.atlas.json\"\nframe = \"player\"\n",
        )
        .unwrap();
        let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
        let mut recorder = RecordingRenderer::default();
        let err = session
            .draw_into(&mut recorder)
            .expect_err("both texture and atlas must fail loudly");
        assert!(
            err.to_string().contains("mutually exclusive"),
            "unexpected: {err:#}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Child-process half of the bundled-Python probe. With
    /// `PITE_BUNDLE_PROBE=1` this initializes Python purely from whatever the
    /// environment provides (a bundled libpython + stdlib zip, no system
    /// Python) and prints `BUNDLE_PROBE_OK` only if a script really ran.
    #[test]
    fn bundle_probe_child() {
        if std::env::var("PITE_BUNDLE_PROBE").is_err() {
            eprintln!("skipped: set PITE_BUNDLE_PROBE=1 in the child process");
            return;
        }
        let dir = std::env::temp_dir().join("pite-bundle-probe-project");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"probe\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"probe\"\ntype = \"Label\"\nname = \"Probe\"\nparent = \"root\"\n\n[node.props]\ntext = \"before\"\n\n[node.script]\npath = \"res://scripts/probe.py\"\nclass = \"Probe\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("scripts").join("probe.py"),
            "import pite\n\n\nclass Probe(pite.Label):\n    def _process(self, delta):\n        self.text = \"bundled\"\n",
        )
        .unwrap();
        let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
        session.update(0.016);
        let text = session
            .host
            .with_tree(|t| match t.get(&NodeId::from("probe".to_string())) {
                Some(n) => match n.props.get("text") {
                    Some(PropValue::Str(s)) => s.clone(),
                    _ => String::new(),
                },
                None => String::new(),
            });
        assert_eq!(text, "bundled", "script did not run under this interpreter");
        println!("BUNDLE_PROBE_OK");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dogfood_player_hit_reaches_enemy() {
        let _guard = SERIAL.lock().unwrap();
        use pite_script::{input_begin_frame, input_set_key};

        let scene = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("examples")
            .join("minimal-2d")
            .join("scenes")
            .join("main.ptscn");
        let mut session = GameSession::open(&scene, true).unwrap();
        assert_eq!(session.script_count(), 3);
        input_begin_frame();
        input_set_key("Space", true);
        session.update(0.016);
        input_set_key("Space", false);
        let after = node_x(&session, "e1_enemy");
        assert!(
            (after + 5.0).abs() < 1e-6,
            "enemy should recoil to x=-5, got {after}"
        );
    }

    #[test]
    fn physics_character_and_raycast_e2e() {
        let _guard = SERIAL.lock().unwrap();

        let dir = std::env::temp_dir().join("pite-physics-e2e");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("pite.toml"),
            "[project]\nname = \"physics-test\"\n",
        )
        .unwrap();

        let scene = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Root"

[[node]]
id = "floor"
type = "StaticBody2D"
name = "Floor"
parent = "root"

[node.props]
position = [0.0, 100.0]

[[node]]
id = "floor_shape"
type = "CollisionShape2D"
name = "Shape"
parent = "floor"

[node.props]
shape = "box"
size = [200.0, 20.0]

[[node]]
id = "player"
type = "CharacterBody2D"
name = "Player"
parent = "root"

[node.props]
position = [0.0, 0.0]

[node.script]
path = "res://scripts/player.py"
class = "Player"

[[node]]
id = "player_shape"
type = "CollisionShape2D"
name = "Shape"
parent = "player"

[node.props]
shape = "box"
size = [20.0, 20.0]
"#;
        std::fs::write(dir.join("scenes").join("main.ptscn"), scene).unwrap();

        let script = r#"import pite

class Player(pite.CharacterBody2D):
    def _physics_process(self, dt):
        self.velocity = (0.0, 300.0)
        self.move_and_slide()
        hit = pite.raycast((0.0, 0.0), (0.0, 150.0))
        if hit is not None and hit.get("body") == "floor":
            pass
"#;
        std::fs::write(dir.join("scripts").join("player.py"), script).unwrap();

        let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
        for _ in 0..30 {
            session.update(0.016);
        }

        let player_y =
            session
                .host
                .with_tree(|t| match t.get(&NodeId::from("player".to_string())) {
                    Some(n) => match n.props.get("position") {
                        Some(PropValue::Vec2(_, y)) => *y,
                        _ => f64::NAN,
                    },
                    None => f64::NAN,
                });

        assert!(
            (player_y - 80.0).abs() < 1.0,
            "Player should rest on floor at y=80, got {player_y}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn physics_area2d_signal_e2e() {
        let _guard = SERIAL.lock().unwrap();

        let dir = std::env::temp_dir().join("pite-physics-area-e2e");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"area-test\"\n").unwrap();

        let scene = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Root"

[[node]]
id = "zone"
type = "Area2D"
name = "Zone"
parent = "root"

[node.props]
position = [0.0, 60.0]

[node.script]
path = "res://scripts/zone.py"
class = "Zone"

[[node]]
id = "zone_shape"
type = "CollisionShape2D"
name = "Shape"
parent = "zone"

[node.props]
shape = "box"
size = [60.0, 60.0]

[[node]]
id = "player"
type = "CharacterBody2D"
name = "Player"
parent = "root"

[node.props]
position = [0.0, 0.0]

[node.script]
path = "res://scripts/player.py"
class = "Player"

[[node]]
id = "player_shape"
type = "CollisionShape2D"
name = "Shape"
parent = "player"

[node.props]
shape = "box"
size = [20.0, 20.0]
"#;
        std::fs::write(dir.join("scenes").join("main.ptscn"), scene).unwrap();

        let player_script = r#"import pite

class Player(pite.CharacterBody2D):
    def _physics_process(self, dt):
        self.velocity = (0.0, 200.0)
        self.move_and_slide()
"#;
        std::fs::write(dir.join("scripts").join("player.py"), player_script).unwrap();

        let zone_script = r#"import pite

class Zone(pite.Area2D):
    def _ready(self):
        self.body_entered.connect(self.on_body)

    def on_body(self, body_id):
        self.position = (999.0, 999.0)
"#;
        std::fs::write(dir.join("scripts").join("zone.py"), zone_script).unwrap();

        let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
        for _ in 0..30 {
            session.update(0.016);
            let zp = session
                .host
                .with_tree(|t| match t.get(&NodeId::from("zone".to_string())) {
                    Some(n) => match n.props.get("position") {
                        Some(PropValue::Vec2(x, y)) => (*x, *y),
                        _ => (0.0, 0.0),
                    },
                    None => (0.0, 0.0),
                });
            if zp == (999.0, 999.0) {
                break;
            }
        }

        let zone_pos = session
            .host
            .with_tree(|t| match t.get(&NodeId::from("zone".to_string())) {
                Some(n) => match n.props.get("position") {
                    Some(PropValue::Vec2(x, y)) => (*x, *y),
                    _ => (0.0, 0.0),
                },
                None => (0.0, 0.0),
            });

        assert_eq!(
            zone_pos,
            (999.0, 999.0),
            "Area2D body_entered signal should move zone to (999, 999)"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
