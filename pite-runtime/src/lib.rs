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
use pite_core::{global_position, Node, NodeId, NodeTree, PropValue};
use pite_physics::{PhysicsEvent, PhysicsWorld, ShapeConfig};
use pite_render::{Renderer2D, WgpuRenderer};
use pite_script::{Pyo3Backend, ScriptBackend, ScriptHost};
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

struct InstanceSlot {
    id: NodeId,
    type_name: String,
    file: Option<PathBuf>,
    backend: Box<dyn ScriptBackend>,
    errored: bool,
    has_ready: bool,
    has_process: bool,
    has_physics_process: bool,
}

fn slot_still_valid(
    slot: &InstanceSlot,
    type_name: &str,
    script: &Option<(PathBuf, String)>,
) -> bool {
    if slot.type_name != type_name {
        return false;
    }
    match (script, slot.file.as_ref()) {
        (Some((path, _class)), Some(existing)) => existing == path,
        (None, None) => true,
        _ => false,
    }
}

/// Read `key` from `node.props`, falling back to the type's schema default
/// (and coercing integer literals to numbers). `rebuild_physics` uses these
/// instead of hardcoded fallbacks so a schema default change propagates
/// without touching the runtime.
fn prop_num(node: &Node, key: &str) -> f32 {
    if let Some(PropValue::Num(n)) = node.props.get(key) {
        return *n as f32;
    }
    if let Some(PropValue::Int(n)) = node.props.get(key) {
        return *n as f32;
    }
    pite_core::all_props(&node.type_name)
        .iter()
        .find(|p| p.key == key)
        .and_then(|p| match p.default_value {
            pite_core::DefaultPropVal::Num(n) => Some(n as f32),
            pite_core::DefaultPropVal::Int(n) => Some(n as f32),
            _ => None,
        })
        .unwrap_or(0.0)
}

fn prop_int(node: &Node, key: &str) -> i64 {
    if let Some(PropValue::Int(n)) = node.props.get(key) {
        return *n;
    }
    if let Some(PropValue::Num(n)) = node.props.get(key) {
        return *n as i64;
    }
    pite_core::all_props(&node.type_name)
        .iter()
        .find(|p| p.key == key)
        .and_then(|p| match p.default_value {
            pite_core::DefaultPropVal::Int(n) => Some(n),
            _ => None,
        })
        .unwrap_or(0)
}

fn prop_bool(node: &Node, key: &str) -> bool {
    if let Some(PropValue::Bool(b)) = node.props.get(key) {
        return *b;
    }
    pite_core::all_props(&node.type_name)
        .iter()
        .find(|p| p.key == key)
        .map(|p| matches!(p.default_value, pite_core::DefaultPropVal::Bool(true)))
        .unwrap_or(false)
}

/// Which physics body a node type becomes. One place decides; both
/// `rebuild_physics` and `CollisionShape2D`'s parent-body walker ask this
/// instead of repeating the type match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhysicsRole {
    Static,
    Character,
    Rigid,
    Area,
}

fn physics_role(type_name: &str) -> Option<PhysicsRole> {
    match type_name {
        "StaticBody2D" => Some(PhysicsRole::Static),
        "CharacterBody2D" => Some(PhysicsRole::Character),
        "RigidBody2D" => Some(PhysicsRole::Rigid),
        "Area2D" => Some(PhysicsRole::Area),
        _ => None,
    }
}

/// Which draw call a node type produces. `draw_into` walks the tree once
/// through this, rather than three separate `filter(type_name == ...)`
/// passes. Adding a renderable type = one new arm here, no new loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderRole {
    Sprite,
    Label,
    Button,
    TileMap,
}

fn render_role(type_name: &str) -> Option<RenderRole> {
    match type_name {
        "Sprite2D" => Some(RenderRole::Sprite),
        "Label" => Some(RenderRole::Label),
        "Button" => Some(RenderRole::Button),
        "TileMap2D" => Some(RenderRole::TileMap),
        _ => None,
    }
}

/// One node's renderable payload, resolved under the tree lock and drawn
/// after it's released.
enum NodeDraw {
    Sprite {
        id: String,
        texture: String,
        atlas: Option<String>,
        frame: Option<String>,
        pos: (f64, f64),
        rotation: f32,
    },
    Text {
        text: String,
        size: f32,
        color: [u8; 4],
        pos: (f64, f64),
    },
    Button {
        text: String,
        size: f32,
        color: [u8; 4],
        pos: (f64, f64),
        extent: (f64, f64),
    },
    TileMap {
        id: String,
        pos: (f64, f64),
    },
}

fn extract_draw(node: &Node, tree: &NodeTree) -> Option<NodeDraw> {
    let role = render_role(&node.type_name)?;
    let pos = global_position(tree, &node.id);
    match role {
        RenderRole::Sprite => {
            let prop = |key: &str| match node.props.get(key) {
                Some(PropValue::Str(s)) if !s.is_empty() => Some(s.clone()),
                _ => None,
            };
            Some(NodeDraw::Sprite {
                id: node.id.to_string(),
                texture: prop("texture").unwrap_or_default(),
                atlas: prop("atlas"),
                frame: prop("frame"),
                pos,
                rotation: prop_num(node, "rotation"),
            })
        }
        RenderRole::Label => {
            let text = match node.props.get("text") {
                Some(PropValue::Str(s)) if !s.is_empty() => s.clone(),
                _ => return None,
            };
            let size = match node.props.get("font_size") {
                Some(PropValue::Num(s)) => *s as f32,
                Some(PropValue::Int(s)) => *s as f32,
                _ => 16.0,
            };
            let color = match node.props.get("color") {
                Some(PropValue::Color(r, g, b, a)) => [*r, *g, *b, *a],
                _ => [255, 255, 255, 255],
            };
            Some(NodeDraw::Text {
                text,
                size,
                color,
                pos,
            })
        }
        RenderRole::Button => {
            let text = match node.props.get("text") {
                Some(PropValue::Str(s)) => s.clone(),
                _ => String::new(),
            };
            let size = match node.props.get("font_size") {
                Some(PropValue::Num(s)) => *s as f32,
                Some(PropValue::Int(s)) => *s as f32,
                _ => 16.0,
            };
            let color = match node.props.get("color") {
                Some(PropValue::Color(r, g, b, a)) => [*r, *g, *b, *a],
                _ => [51, 65, 85, 255],
            };
            let extent = button_size(node);
            Some(NodeDraw::Button {
                text,
                size,
                color,
                pos,
                extent,
            })
        }
        RenderRole::TileMap => Some(NodeDraw::TileMap {
            id: node.id.to_string(),
            pos,
        }),
    }
}

/// Engine-owned signals every node of that type declares before `_ready`
/// runs. Subclasses that re-declare `pressed`/`timeout` still work
/// (`declare_signals` is idempotent). Adding a new engine signal = one
/// row here, no code change in `rebuild`.
const ENGINE_SIGNALS: &[(&str, &[(&str, &[&str])])] = &[
    ("Button", &[("pressed", &[])]),
    ("Timer", &[("timeout", &[])]),
    (
        "Area2D",
        &[
            ("body_entered", &["str"]),
            ("body_exited", &["str"]),
            ("area_entered", &["str"]),
            ("area_exited", &["str"]),
        ],
    ),
];

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

    pub fn reload_script_path(&mut self, path: &Path) -> bool {
        let mut reloaded = false;
        for slot in self
            .instances
            .iter_mut()
            .filter(|s| s.file.as_ref().is_some_and(|f| same_file(f, path)))
        {
            match slot.backend.reload() {
                Ok(()) => {
                    slot.errored = false;
                    slot.has_ready = slot.backend.has_ready();
                    slot.has_process = slot.backend.has_process();
                    slot.has_physics_process = slot.backend.has_physics_process();
                    self.host.invalidate_proxy_cache(slot.id.as_str());
                    reloaded = true;
                    tracing::info!(node = %slot.id, "script reloaded");
                }
                Err(e) => {
                    tracing::error!(node = %slot.id, "reload failed, keeping last good state: {e:#}")
                }
            }
        }
        reloaded
    }

    pub fn is_scene_affected(&self, path: &Path) -> bool {
        if same_file(path, &self.scene) {
            return true;
        }
        if let Ok(doc) = pite_scene::load_scene_cached(&self.scene) {
            for inst in &doc.instance {
                if let Ok(inst_path) = pite_scene::resolve_scene_ref(
                    &inst.scene,
                    &self.scene_dir,
                    self.project_dir.as_deref(),
                ) {
                    if same_file(path, &inst_path) {
                        return true;
                    }
                }
            }
        }
        false
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

    pub fn draw_into(&mut self, r: &mut dyn Renderer2D) -> Result<()> {
        if !self.dirty_assets.is_empty() {
            for p in self.dirty_assets.drain() {
                r.reload_asset(&p.to_string_lossy());
            }
        }
        let (cam, _) = self.camera_view();
        r.set_camera(cam.0, cam.1, self.effective_zoom());

        // One walk: every renderable node becomes a `NodeDraw`, drawn in tree
        // order (matching Godot's z-order — reorder the tree to reorder draws).
        // This replaces the previous three per-type filter loops; new
        // renderable types add an arm in `render_role`, not another pass.
        let draws: Vec<NodeDraw> = self
            .host
            .with_tree(|tree| tree.iter().filter_map(|n| extract_draw(n, tree)).collect());

        for draw in draws {
            match draw {
                NodeDraw::Sprite {
                    id,
                    texture,
                    atlas,
                    frame,
                    pos,
                    rotation,
                } => {
                    let has_texture = !texture.is_empty();
                    let atlas = atlas.filter(|s| !s.is_empty());
                    let frame = frame.filter(|s| !s.is_empty());
                    if has_texture && atlas.is_some() {
                        anyhow::bail!("node {id}: `texture` and `atlas` are mutually exclusive");
                    }
                    match (atlas, frame) {
                        (Some(atlas), Some(frame)) => {
                            let sidecar = self.resolve_path(&atlas);
                            let sheet = pite_render::atlas::sheet_for_sidecar(&sidecar);
                            r.draw_sprite_frame(
                                &sheet.to_string_lossy(),
                                Some(&frame),
                                pos.0,
                                pos.1,
                                rotation as f64,
                            )?;
                        }
                        (Some(_), None) => anyhow::bail!("node {id}: `atlas` requires a `frame`"),
                        (None, Some(_)) => anyhow::bail!("node {id}: `frame` requires an `atlas`"),
                        (None, None) => {
                            if has_texture {
                                let resolved = self.resolve_path(&texture);
                                r.draw_sprite(
                                    &resolved.to_string_lossy(),
                                    pos.0,
                                    pos.1,
                                    rotation as f64,
                                )?;
                            }
                        }
                    }
                }
                NodeDraw::Text {
                    text,
                    size,
                    color,
                    pos,
                } => {
                    r.draw_text(&text, pos.0, pos.1, size, color)?;
                }
                NodeDraw::Button {
                    text,
                    size,
                    color,
                    pos,
                    extent: (w, h),
                } => {
                    r.draw_rect(pos.0, pos.1, w, h, color)?;
                    if !text.is_empty() {
                        r.draw_text(&text, pos.0, pos.1, size, [255, 255, 255, 255])?;
                    }
                }
                NodeDraw::TileMap { id, pos } => {
                    if let Some(doc) = self.host.get_tilemap_doc(&id) {
                        let sidecar = self.resolve_path(&doc.atlas);
                        let sheet = pite_render::atlas::sheet_for_sidecar(&sidecar);
                        let sheet_str = sheet.to_string_lossy();
                        let tile_w = doc.tile_size[0] as f64;
                        let tile_h = doc.tile_size[1] as f64;
                        let zoom = self.effective_zoom();
                        let half_w = (self.viewport.0 as f64 / 2.0) / zoom;
                        let half_h = (self.viewport.1 as f64 / 2.0) / zoom;
                        let min_x = cam.0 - half_w;
                        let max_x = cam.0 + half_w;
                        let min_y = cam.1 - half_h;
                        let max_y = cam.1 + half_h;

                        let min_col = ((min_x - pos.0) / tile_w).floor() as i32 - 1;
                        let max_col = ((max_x - pos.0) / tile_w).ceil() as i32 + 1;
                        let min_row = ((min_y - pos.1) / tile_h).floor() as i32 - 1;
                        let max_row = ((max_y - pos.1) / tile_h).ceil() as i32 + 1;

                        for layer in &doc.layers {
                            if !layer.visible {
                                continue;
                            }
                            for tile in &layer.tiles {
                                if tile.0 >= min_col
                                    && tile.0 <= max_col
                                    && tile.1 >= min_row
                                    && tile.1 <= max_row
                                {
                                    let cx = pos.0 + (tile.0 as f64 + 0.5) * tile_w;
                                    let cy = pos.1 + (tile.1 as f64 + 0.5) * tile_h;
                                    r.draw_sprite_frame(&sheet_str, Some(&tile.2), cx, cy, 0.0)?;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn rebuild(&mut self) -> Result<()> {
        let doc = pite_scene::load_scene_cached(&self.scene)
            .with_context(|| format!("cannot load scene {}", self.scene.display()))?;
        let tree = pite_scene::build_tree(&doc, &self.scene_dir, self.project_dir.as_deref())?;
        self.host.with_tree_mut(|t| *t = tree);

        // Engine-owned signals come from one table; every matching node declares
        // its type's signals before `_ready` runs. Subclasses that re-declare
        // inherit through Python MRO as before.
        let nodes: Vec<(String, String)> = self.host.with_tree(|tree| {
            tree.iter()
                .map(|n| (n.type_name.clone(), n.id.to_string()))
                .collect()
        });
        for (type_name, id) in &nodes {
            let Some((_, signals)) = ENGINE_SIGNALS
                .iter()
                .find(|(t, _)| *t == type_name.as_str())
            else {
                continue;
            };
            let defs: Vec<(String, Vec<String>)> = signals
                .iter()
                .map(|(name, payload)| {
                    (
                        name.to_string(),
                        payload.iter().map(|s| s.to_string()).collect(),
                    )
                })
                .collect();
            if let Err(e) = self.host.declare_signals(id, defs) {
                tracing::error!(
                    node = %id,
                    ty = %type_name,
                    "cannot declare builtin signals: {e:#}"
                );
            }
        }

        let tilemaps: Vec<(String, String)> = self.host.with_tree(|tree| {
            tree.iter()
                .filter(|n| n.type_name == "TileMap2D")
                .filter_map(|n| {
                    let map_path = match n.props.get("tilemap") {
                        Some(PropValue::Str(s)) if !s.is_empty() => s.clone(),
                        _ => return None,
                    };
                    Some((n.id.to_string(), map_path))
                })
                .collect()
        });
        for (id, map_path) in tilemaps {
            let resolved = self.resolve_path(&map_path);
            match pite_scene::load_tilemap(&resolved) {
                Ok(doc) => {
                    self.host.set_tilemap_doc(&id, doc);
                }
                Err(e) => {
                    tracing::error!(node = %id, "cannot load tilemap {}: {e:#}", resolved.display());
                }
            }
        }

        self.rebuild_physics();

        let all_nodes: Vec<(NodeId, String, Option<(PathBuf, String)>)> =
            self.host.with_tree(|tree| {
                tree.iter()
                    .map(|node| {
                        let script = node.script.as_ref().and_then(|s| {
                            resolve_script(&s.path, &self.scene_dir, self.project_dir.as_deref())
                                .map(|p| (p, s.class_name.clone()))
                        });
                        (node.id.clone(), node.type_name.clone(), script)
                    })
                    .collect()
            });

        let mut olds: HashMap<String, InstanceSlot> = HashMap::new();
        for slot in self.instances.drain(..) {
            olds.insert(slot.id.to_string(), slot);
        }

        for (id, type_name, script_opt) in all_nodes {
            let id_str = id.to_string();
            if let Some(slot) = olds.remove(&id_str) {
                if slot_still_valid(&slot, &type_name, &script_opt) {
                    self.instances.push(slot);
                    continue;
                }
                self.host.unregister(&id_str);
                self.host.disconnect_node(&id_str);
            }

            match script_opt {
                Some((path, class)) => {
                    if !path.is_file() {
                        tracing::warn!(node = %id, script = %path.display(), "missing script, keeping placeholder node");
                        continue;
                    }
                    let mut backend = Box::new(Pyo3Backend::new(id_str.clone()));
                    backend.set_host(self.host.clone());
                    match backend.load(&path.to_string_lossy(), &class) {
                        Ok(()) => {
                            let has_ready = backend.has_ready();
                            let has_process = backend.has_process();
                            let has_physics_process = backend.has_physics_process();
                            let mut errored = false;
                            if has_ready {
                                if let Err(e) = backend.call_ready() {
                                    tracing::error!(node = %id, "{e:#}");
                                    errored = true;
                                }
                            }
                            self.instances.push(InstanceSlot {
                                id,
                                type_name,
                                file: Some(path),
                                backend,
                                errored,
                                has_ready,
                                has_process,
                                has_physics_process,
                            });
                        }
                        Err(e) => {
                            tracing::error!(node = %id, "cannot load script: {e:#}");
                        }
                    }
                }
                None => {
                    let mut backend = Box::new(Pyo3Backend::new(id_str.clone()));
                    backend.set_host(self.host.clone());
                    match backend.load_base(&type_name) {
                        Ok(()) => {
                            let has_ready = backend.has_ready();
                            let has_process = backend.has_process();
                            let has_physics_process = backend.has_physics_process();
                            let mut errored = false;
                            if has_ready {
                                if let Err(e) = backend.call_ready() {
                                    tracing::error!(node = %id, "{e:#}");
                                    errored = true;
                                }
                            }
                            self.instances.push(InstanceSlot {
                                id,
                                type_name,
                                file: None,
                                backend,
                                errored,
                                has_ready,
                                has_process,
                                has_physics_process,
                            });
                        }
                        Err(e) => {
                            tracing::error!(node = %id, "cannot instantiate base pyclass: {e:#}");
                        }
                    }
                }
            }
        }

        for (id_str, _) in olds {
            self.host.unregister(&id_str);
            self.host.disconnect_node(&id_str);
        }

        tracing::info!(
            nodes = self.tree_len(),
            instances = self.instances.len(),
            scripted = self.scripted_count(),
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

    pub fn poll_watch(&mut self) -> bool {
        if self.no_reload {
            return false;
        }
        let mut script_touched: HashSet<PathBuf> = HashSet::new();
        let mut scene_touched = false;
        let mut assets_touched: HashSet<PathBuf> = HashSet::new();
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
                            if self.is_scene_affected(&path) {
                                scene_touched = true;
                            }
                        }
                        Some("ptmap") => {
                            scene_touched = true;
                        }
                        Some("png") | Some("json") => {
                            assets_touched.insert(path);
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut changed = false;
        for path in script_touched {
            if self.reload_script_path(&path) {
                changed = true;
            }
        }
        if !assets_touched.is_empty() {
            self.dirty_assets.extend(assets_touched);
            changed = true;
        }
        if scene_touched {
            match self.rebuild() {
                Ok(()) => {
                    changed = true;
                    tracing::info!("scene reloaded");
                }
                Err(e) => tracing::error!("scene reload failed, keeping old tree: {e:#}"),
            }
        }
        changed
    }

    pub fn rebuild_physics(&mut self) {
        let mut world = PhysicsWorld::new();
        self.host.with_tree(|tree| {
            for node in tree.iter() {
                let pos = global_position(tree, &node.id);
                let rot = prop_num(node, "rotation");
                if let Some(role) = physics_role(&node.type_name) {
                    match role {
                        PhysicsRole::Static => {
                            world.add_static_body(
                                node.id.clone(),
                                (pos.0 as f32, pos.1 as f32),
                                rot,
                            );
                        }
                        PhysicsRole::Character => {
                            world.add_character_body(
                                node.id.clone(),
                                (pos.0 as f32, pos.1 as f32),
                                rot,
                            );
                        }
                        PhysicsRole::Rigid => {
                            world.add_rigid_body(
                                node.id.clone(),
                                (pos.0 as f32, pos.1 as f32),
                                rot,
                                prop_num(node, "mass"),
                                prop_num(node, "gravity_scale"),
                                prop_bool(node, "lock_rotation"),
                            );
                        }
                        PhysicsRole::Area => {
                            world.add_area(node.id.clone(), (pos.0 as f32, pos.1 as f32), rot);
                        }
                    }
                } else if node.type_name == "TileMap2D" {
                    if let Some(doc) = self.host.get_tilemap_doc(node.id.as_str()) {
                        let tile_size = (doc.tile_size[0] as f32, doc.tile_size[1] as f32);
                        for (idx, layer) in doc.layers.iter().enumerate() {
                            if layer.collision_enabled {
                                let solid_cells = doc.solid_cells(idx);
                                world.add_tilemap_colliders(
                                    &node.id,
                                    idx,
                                    (pos.0 as f32, pos.1 as f32),
                                    tile_size,
                                    &solid_cells,
                                    layer.collision_layer,
                                    layer.collision_mask,
                                );
                            }
                        }
                    }
                }
            }

            for node in tree.iter().filter(|n| n.type_name == "CollisionShape2D") {
                let disabled = match node.props.get("disabled") {
                    Some(PropValue::Bool(b)) => *b,
                    _ => false,
                };
                if disabled {
                    continue;
                }

                let mut parent_id = node.parent.clone();
                let mut parent_body: Option<&pite_core::Node> = None;
                while let Some(pid) = parent_id {
                    if let Some(pnode) = tree.get(&pid) {
                        if physics_role(&pnode.type_name).is_some() {
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

                let layer = prop_int(body, "collision_layer") as u32;
                let mask = prop_int(body, "collision_mask") as u32;
                // `friction`/`restitution` live on `PhysicsBody2D`; Area2D
                // doesn't inherit them, so it falls through to 0.0 (harmless
                // — sensors don't apply either).
                let friction = prop_num(body, "friction");
                let restitution = prop_num(body, "restitution");

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
        pite_audio::poll_audio();

        // Install the current camera pose so `pite.screen_to_world` (used by
        // `PyButton::_process` to hit-test clicks against world-space button
        // positions) sees this frame's camera before the `_process` loop.
        let ((cx, cy), _) = self.camera_view();
        pite_script::set_camera_view((cx, cy), self.effective_zoom(), self.viewport);

        const FIXED_STEP: f64 = 1.0 / 60.0;
        self.physics_accumulator += delta;
        if self.physics_accumulator > 0.25 {
            self.physics_accumulator = 0.25;
        }

        while self.physics_accumulator >= FIXED_STEP {
            for slot in &mut self.instances {
                if slot.errored || !slot.has_physics_process {
                    continue;
                }
                if let Err(e) = slot.backend.call_physics_process(FIXED_STEP) {
                    tracing::error!(node = %slot.id, "physics process error: {e:#}");
                    slot.errored = true;
                    if self.pause_on_error && self.last_error.is_none() {
                        self.last_error = slot.backend.last_script_error();
                    }
                }
                let pos = self.host.with_tree(|t| {
                    match t.get(&slot.id).and_then(|n| n.props.get("position")) {
                        Some(PropValue::Vec2(x, y)) => Some((*x, *y)),
                        _ => None,
                    }
                });
                if let Some((x, y)) = pos {
                    let mut lock = self.physics.lock().unwrap();
                    lock.set_body_position(&slot.id, (x, y));
                }
            }

            let events = {
                let mut lock = self.physics.lock().unwrap();
                lock.step();
                lock.drain_events()
            };

            {
                let lock = self.physics.lock().unwrap();
                let mut moved = Vec::new();
                for slot in &self.instances {
                    if lock.is_rigid_body(&slot.id) {
                        if let Some(pos) = lock.get_body_position(&slot.id) {
                            moved.push((slot.id.clone(), pos));
                        }
                    }
                }
                drop(lock);
                for (id, (x, y)) in moved {
                    self.host.with_tree_mut(|tree| {
                        if let Some(node) = tree.get_mut(&id) {
                            node.props
                                .insert("position".to_string(), PropValue::Vec2(x, y));
                        }
                    });
                }
            }

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

        // `_process` loop: iterates only slots that actually define
        // `_process` (via the probe cached in `InstanceSlot`). Base `Timer`
        // and `Button` now carry engine-owned `_process` here — no
        // `tick_timers`/`poll_buttons` dispatch.
        for slot in &mut self.instances {
            if slot.errored || !slot.has_process {
                continue;
            }
            if let Err(e) = slot.backend.call_process(delta) {
                tracing::error!(node = %slot.id, "{e:#}");
                slot.errored = true;
                if self.pause_on_error && self.last_error.is_none() {
                    self.last_error = slot.backend.last_script_error();
                }
            }
        }
    }

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

pub const DEFAULT_ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn window_icon() -> Option<winit::window::Icon> {
    let img =
        image::load_from_memory_with_format(DEFAULT_ICON_PNG, image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    winit::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let mut attrs = Window::default_attributes().with_title(self.title.clone());
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
        scripted = session.scripted_count(),
        instances = session.instance_count(),
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

    fn tree_vec2(session: &GameSession, node_id: &str, key: &str) -> Option<(f64, f64)> {
        session.host.with_tree(|t| {
            t.get(&NodeId::from(node_id.to_string()))
                .and_then(|n| n.props.get(key))
                .and_then(|v| match v {
                    PropValue::Vec2(x, y) => Some((*x, *y)),
                    _ => None,
                })
        })
    }

    #[test]
    fn editing_script_changes_behavior_without_restart() {
        let _guard = SERIAL.lock().unwrap();
        let (dir, script) = make_project("reload", "Mover", V1);
        let scene = dir.join("scenes").join("main.ptscn");
        let mut session = GameSession::open(&scene, false).unwrap();
        assert_eq!(session.scripted_count(), 1);
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
            if session.tree_len() == 2 && session.scripted_count() == 0 {
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

    #[test]
    fn unrelated_scene_edit_does_not_rebuild() {
        let _guard = SERIAL.lock().unwrap();
        let (dir, _) = make_project("unrelated", "Mover", V1);
        let scene = dir.join("scenes").join("main.ptscn");
        let other = dir.join("scenes").join("other.ptscn");
        std::fs::write(
            &other,
            "format_version = 1\nroot = \"other\"\n\n[[node]]\nid = \"other\"\ntype = \"Node2D\"\nname = \"Other\"\n",
        )
        .unwrap();
        let mut session = GameSession::open(&scene, false).unwrap();
        assert_eq!(session.tree_len(), 2);

        std::fs::write(
            &other,
            "format_version = 1\nroot = \"other2\"\n\n[[node]]\nid = \"other2\"\ntype = \"Node2D\"\nname = \"Other2\"\n",
        )
        .unwrap();
        for _ in 0..10 {
            session.poll_watch();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(session.tree_len(), 2);
        assert_eq!(session.scripted_count(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn script_reload_preserves_custom_attributes_and_velocity() {
        let _guard = SERIAL.lock().unwrap();
        let src1 = "import pite\n\nclass Char(pite.CharacterBody2D):\n    def _ready(self):\n        self.score = 42\n        self.velocity = (123.0, 456.0)\n";
        let src2 = "import pite\n\nclass Char(pite.CharacterBody2D):\n    def _process(self, delta):\n        pass\n";
        let (dir, script) = make_project("char-preserve", "Char", src1);
        let scene = dir.join("scenes").join("main.ptscn");
        std::fs::write(
            &scene,
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"mover\"\ntype = \"CharacterBody2D\"\nname = \"Char\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/mover.py\"\nclass = \"Char\"\n",
        )
        .unwrap();
        let mut session = GameSession::open(&scene, false).unwrap();
        session.update(0.016);

        use pite_script::pyo3::{self, types::PyAnyMethods};
        pyo3::Python::attach(|py| {
            let inst = session.host.lookup_instance(py, "mover").unwrap();
            let score: i32 = inst.bind(py).getattr("score").unwrap().extract().unwrap();
            assert_eq!(score, 42);
        });
        assert_eq!(
            tree_vec2(&session, "mover", "velocity"),
            Some((123.0, 456.0))
        );

        std::fs::write(&script, src2).unwrap();
        let mut reloaded = false;
        for _ in 0..100 {
            if session.poll_watch() {
                reloaded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(reloaded, "script reload never detected");

        pyo3::Python::attach(|py| {
            let inst = session.host.lookup_instance(py, "mover").unwrap();
            let score: i32 = inst.bind(py).getattr("score").unwrap().extract().unwrap();
            assert_eq!(score, 42);
        });
        assert_eq!(
            tree_vec2(&session, "mover", "velocity"),
            Some((123.0, 456.0))
        );
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

    const TICKER: &str = "import pite\n\nclass Ticker(pite.Timer):\n    def _ready(self):\n        self.get_node(\".\").timeout.connect(self.on_timeout)\n    def on_timeout(self):\n        hud = self.get_node(\"../Hud\")\n        hud.text = str(int(hud.text) + 1)\n";

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

    #[derive(Default)]
    struct RecordingRenderer {
        calls: Vec<(String, Option<String>, (f64, f64))>,
    }

    impl Renderer2D for RecordingRenderer {
        fn begin_frame(&mut self) -> Result<()> {
            Ok(())
        }

        fn draw_sprite(&mut self, texture: &str, x: f64, y: f64, _rotation: f64) -> Result<()> {
            self.calls.push((texture.to_string(), None, (x, y)));
            Ok(())
        }

        fn draw_sprite_frame(
            &mut self,
            texture: &str,
            frame: Option<&str>,
            x: f64,
            y: f64,
            _rotation: f64,
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

    #[test]
    fn plain_texture_sprite_draws_without_mutual_exclusion_error() {
        let _guard = SERIAL.lock().unwrap();
        let dir =
            std::env::temp_dir().join(format!("pite-plain-texture-draw-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"sprite\"\ntype = \"Sprite2D\"\nname = \"Sprite\"\nparent = \"root\"\n\n[node.props]\ntexture = \"res://assets/player.png\"\n",
        )
        .unwrap();
        let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
        let mut recorder = RecordingRenderer::default();
        session
            .draw_into(&mut recorder)
            .expect("plain texture sprite must draw cleanly without atlas collision");
        std::fs::remove_dir_all(&dir).ok();
    }

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
        assert_eq!(session.scripted_count(), 3);
        // Every node gets a slot, scripted or not. The dogfood scene has 5
        // nodes in main.ptscn + 2 from the instanced enemy.ptscn = 7.
        assert_eq!(session.instance_count(), 7);
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

    /// Sanity-check that every tree node reaches `update()` via an
    /// instance slot. A scriptless Sprite2D costs zero Python calls
    /// (`has_process = false`), but its slot still exists so future
    /// engine-owned behaviors can attach to it.
    #[test]
    fn scriptless_nodes_still_get_instances() {
        let _guard = SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pite-sess-scriptless-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"spr\"\ntype = \"Sprite2D\"\nname = \"Spr\"\nparent = \"root\"\n\n[[node]]\nid = \"lbl\"\ntype = \"Label\"\nname = \"Lbl\"\nparent = \"root\"\n",
        )
        .unwrap();
        let session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
        assert_eq!(session.scripted_count(), 0);
        assert_eq!(session.instance_count(), 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end check that a bare (unscripted) `Timer` advances its
    /// `current_elapsed` from `PyTimer::_process`, then resets on fire.
    /// This exercises the whole new pipeline: base pyclass → `_process` →
    /// prop update → wrap around. It would fail if either the slot
    /// instantiation or the `_process` loop regressed.
    #[test]
    fn scriptless_timer_advances_elapsed() {
        let _guard = SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pite-sess-bare-timer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"tick\"\ntype = \"Timer\"\nname = \"Tick\"\nparent = \"root\"\n\n[node.props]\nwait_time = 0.1\n",
        )
        .unwrap();
        let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
        assert_eq!(session.instance_count(), 2);
        assert_eq!(session.scripted_count(), 0);

        session.update(0.05);
        let e1 = session.host.with_tree(|t| {
            t.get(&NodeId::from("tick".to_string()))
                .and_then(|n| n.props.get("current_elapsed"))
                .and_then(|v| match v {
                    PropValue::Num(f) => Some(*f),
                    _ => None,
                })
                .unwrap_or(f64::NAN)
        });
        assert!((e1 - 0.05).abs() < 1e-6, "expected 0.05, got {e1}");

        session.update(0.06); // crosses 0.1 → resets
        let e2 = session.host.with_tree(|t| {
            t.get(&NodeId::from("tick".to_string()))
                .and_then(|n| n.props.get("current_elapsed"))
                .and_then(|v| match v {
                    PropValue::Num(f) => Some(*f),
                    _ => None,
                })
                .unwrap_or(f64::NAN)
        });
        assert!(e2.abs() < 1e-6, "expected 0 (reset), got {e2}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tilemap_loading_and_physics_e2e() {
        let _guard = SERIAL.lock().unwrap();

        let dir = std::env::temp_dir().join(format!("pite-tilemap-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(
            dir.join("pite.toml"),
            "[project]\nname = \"tilemap-test\"\n",
        )
        .unwrap();

        let atlas = r#"{
  "texture": "res://assets/tiles.png",
  "size": [64, 64],
  "frames": {
    "grass": { "x": 0, "y": 0, "w": 32, "h": 32 },
    "wall": { "x": 32, "y": 0, "w": 32, "h": 32 }
  }
}"#;
        std::fs::write(dir.join("assets").join("tiles.atlas.json"), atlas).unwrap();

        let ptmap = r#"atlas = "res://assets/tiles.atlas.json"
tile_size = [32, 32]

[[layer]]
name = "ground"
visible = true
collision_enabled = true
collision_layer = 1
collision_mask = 1
tiles = [
  [-1, 0, "wall"],
  [0, 0, "wall"],
  [1, 0, "wall"],
]
"#;
        std::fs::write(dir.join("maps").join("level.ptmap"), ptmap).unwrap();

        let scene = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Root"

[[node]]
id = "tilemap"
type = "TileMap2D"
name = "Map"
parent = "root"

[node.props]
tilemap = "res://maps/level.ptmap"
position = [0.0, 100.0]

[node.script]
path = "res://scripts/map.py"
class = "MapScript"

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

        let map_script = r#"import pite

class MapScript(pite.TileMap2D):
    def _ready(self):
        val = self.get_cell(0, 0)
        assert val == "wall", f"expected wall, got {val}"
        
        layers = self.layers
        assert layers == ["ground"], f"got layers {layers}"
        
        self.set_cell(2, 0, "grass")
        assert self.get_cell(2, 0) == "grass"
        
        col, row = self.world_to_map((0.0, 100.0))
        assert (col, row) == (0, 0), f"got ({col}, {row})"
        world = self.map_to_world(0, 0)
        assert world.x == 16.0 and world.y == 116.0, f"got {world.x}, {world.y}"
"#;
        std::fs::write(dir.join("scripts").join("map.py"), map_script).unwrap();

        let mut session = GameSession::open(&dir.join("scenes").join("main.ptscn"), true).unwrap();
        for _ in 0..40 {
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
            (player_y - 90.0).abs() < 2.0,
            "Player should rest on tilemap floor at y=90, got {player_y}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
