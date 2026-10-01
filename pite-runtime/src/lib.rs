//! `pite-runtime`: thin orchestrator wiring scene + render + script.
//! Opens a window, loads the scene file into a node tree, runs the loop.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use pite_core::{global_position, NodeId, NodeTree, PropValue};
use pite_render::{Renderer2D, WgpuRenderer};
use pite_script::{input_mouse, input_pressed, input_released, Pyo3Backend, ScriptBackend, ScriptHost};
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
}

impl GameSession {
    pub fn open(scene: &Path, no_reload: bool) -> Result<Self> {
        let scene_dir = scene
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let project_dir = pite_project::find_project_root(scene);
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
        };
        pite_script::set_current_host(session.host.clone());
        pite_audio::set_project_dir(session.project_dir.clone());
        session.rebuild()?;
        if !session.no_reload {
            session.start_watcher()?;
        }
        Ok(session)
    }

    pub fn host(&self) -> &Arc<ScriptHost> {
        &self.host
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
                Err(e) => tracing::error!(node = %slot.id, "reload failed, keeping last good state: {e:#}"),
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

    pub fn button_at(&self, screen: (f64, f64)) -> Option<String> {
        let ((cx, cy), zoom) = self.camera_view();
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

    fn rebuild(&mut self) -> Result<()> {
        let doc = pite_scene::load_scene(&self.scene)
            .with_context(|| format!("cannot load scene {}", self.scene.display()))?;
        let tree = pite_scene::build_tree(&doc, &self.scene_dir, self.project_dir.as_deref())?;
        self.host.with_tree_mut(|t| *t = tree);
        let mut olds: HashMap<String, ScriptSlot> = HashMap::new();
        for slot in self.slots.drain(..) {
            olds.insert(slot.id.to_string(), slot);
        }
        let wanted: Vec<(NodeId, PathBuf, String)> = self.host.with_tree(|tree| {
            tree.iter()
                .filter_map(|node| {
                    let script = node.script.as_ref()?;
                    let path = resolve_script(
                        &script.path,
                        &self.scene_dir,
                        self.project_dir.as_deref(),
                    )?;
                    Some((node.id.clone(), path, script.class_name.clone()))
                })
                .collect()
        });
        for (id, path, class) in wanted {
            if let Some(mut slot) = olds.remove(&id.to_string()) {
                if slot.file != path {
                    slot.file = path;
                    if let Err(e) = slot
                        .backend
                        .load(&slot.file.to_string_lossy(), &class)
                    {
                        tracing::error!(node = %id, "cannot reload script: {e:#}");
                        slot.errored = true;
                    }
                }
                self.host.with_tree(|tree| {
                    if let Some((x, y)) = seed_position(tree, &id) {
                        slot.backend.set_position(x, y);
                    }
                    if let Some(text) = seed_text(tree, &id) {
                        slot.backend.set_text(&text);
                    }
                });
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
                    self.host.with_tree(|tree| {
                        if let Some((x, y)) = seed_position(tree, &id) {
                            backend.set_position(x, y);
                        }
                        if let Some(text) = seed_text(tree, &id) {
                            backend.set_text(&text);
                        }
                    });
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
        tracing::info!(nodes = self.tree_len(), scripts = self.slots.len(), "scene ready");
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
                        Some("pitescene") => {
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

    pub fn update(&mut self, delta: f64) {
        self.poll_buttons();
        pite_audio::poll_audio();
        for slot in &mut self.slots {
            if slot.errored {
                continue;
            }
            if let Err(e) = slot.backend.call_process(delta) {
                tracing::error!(node = %slot.id, "{e:#}");
                slot.errored = true;
            }
            if let Some((x, y)) = slot.backend.position() {
                self.host.with_tree_mut(|tree| {
                    if let Some(node) = tree.get_mut(&slot.id) {
                        node.props.insert("position".to_string(), PropValue::Vec2(x, y));
                    }
                });
            }
            if let Some(text) = slot.backend.text() {
                self.host.with_tree_mut(|tree| {
                    if let Some(node) = tree.get_mut(&slot.id) {
                        node.props.insert("text".to_string(), PropValue::Str(text));
                    }
                });
            }
        }
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

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
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
                let scale = self.window.as_ref().map(|w| w.scale_factor()).unwrap_or(1.0);
                pite_script::input_set_mouse(position.x / scale, position.y / scale);
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
                self.session.poll_watch();
                pite_script::input_begin_frame();
                if let Some(renderer) = &self.renderer {
                    let (w, h) = renderer.size();
                    self.session.set_viewport(w, h);
                }
                self.session.update(delta);
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
            let (cam, zoom) = self.session.host().with_tree(|tree| {
                tree.iter()
                    .find(|n| n.type_name == "Camera2D")
                    .map(|cam| {
                        let pos = global_position(tree, &cam.id);
                        let zoom = match cam.props.get("zoom") {
                            Some(PropValue::Num(z)) => *z,
                            _ => 1.0,
                        };
                        (pos, zoom)
                    })
                    .unwrap_or(((0.0, 0.0), 1.0))
            });
            renderer.set_camera(cam.0, cam.1, zoom);
            let sprites: Vec<(String, (f64, f64))> = self.session.host().with_tree(|tree| {
                tree.iter()
                    .filter(|n| n.type_name == "Sprite2D")
                    .filter_map(|n| {
                        let tex = match n.props.get("texture") {
                            Some(PropValue::Str(s)) => s.clone(),
                            _ => return None,
                        };
                        Some((tex, global_position(tree, &n.id)))
                    })
                    .collect()
            });
            for (tex, pos) in sprites {
                let resolved = self.session.resolve_path(&tex);
                renderer.draw_sprite(&resolved.to_string_lossy(), pos.0, pos.1)?;
            }
            let labels: Vec<(String, f32, [u8; 4], (f64, f64))> =
                self.session.host().with_tree(|tree| {
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
                                Some(PropValue::Str(s)) => {
                                    pite_render::text::parse_color(s)
                                }
                                _ => [255, 255, 255, 255],
                            };
                            Some((text, size, color, global_position(tree, &n.id)))
                        })
                        .collect()
                });
            for (text, size, color, pos) in labels {
                renderer.draw_text(&text, pos.0, pos.1, size, color)?;
            }
            let buttons: Vec<(String, f32, [u8; 4], (f64, f64), (f64, f64))> =
                self.session.host().with_tree(|tree| {
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
                                Some(PropValue::Str(s)) => {
                                    pite_render::text::parse_color(s)
                                }
                                _ => [51, 65, 85, 255],
                            };
                            let (w, h) = button_size(n);
                            (text, size, color, global_position(tree, &n.id), (w, h))
                        })
                        .collect()
                });
            for (text, size, color, pos, (w, h)) in buttons {
                renderer.draw_rect(pos.0, pos.1, w, h, color)?;
                if !text.is_empty() {
                    renderer.draw_text(&text, pos.0, pos.1, size, [255, 255, 255, 255])?;
                }
            }
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
    event_loop
        .run_app(&mut app)
        .context("event loop failed")?;
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
            dir.join("scenes").join("main.pitescene"),
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
        let scene = dir.join("scenes").join("main.pitescene");
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
        let scene = dir.join("scenes").join("main.pitescene");
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
        let scene = dir.join("scenes").join("main.pitescene");
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
            dir.join("scenes").join("main.pitescene"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"hud\"\ntype = \"Label\"\nname = \"Hud\"\nparent = \"root\"\n\n[node.props]\ntext = \"HP: 3\"\nfont_size = 20.0\n\n[node.script]\npath = \"res://scripts/hud.py\"\nclass = \"Hud\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("scripts").join("hud.py"), HUD).unwrap();
        let scene = dir.join("scenes").join("main.pitescene");
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
            dir.join("scenes").join("main.pitescene"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"pinger\"\ntype = \"Node2D\"\nname = \"Pinger\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/pinger.py\"\nclass = \"Pinger\"\n\n[[node]]\nid = \"ponger\"\ntype = \"Node2D\"\nname = \"Ponger\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/ponger.py\"\nclass = \"Ponger\"\n\n[[node]]\nid = \"hud\"\ntype = \"Label\"\nname = \"Hud\"\nparent = \"root\"\n\n[node.props]\ntext = \"waiting\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("scripts").join("pinger.py"), PINGER).unwrap();
        std::fs::write(dir.join("scripts").join("ponger.py"), PONGER).unwrap();
        let scene = dir.join("scenes").join("main.pitescene");
        let mut session = GameSession::open(&scene, true).unwrap();
        assert_eq!(hud_text(&session), "waiting");
        session.update(0.016);
        assert_eq!(hud_text(&session), "hit!");
        std::fs::remove_dir_all(&dir).ok();
    }

    const COUNTER: &str = "import pite\n\nclass Counter(pite.Button):\n    pressed = pite.signal()\n    def _ready(self):\n        self.text = \"0\"\n        self.get_node(\".\").pressed.connect(self.on_pressed)\n    def on_pressed(self):\n        self.text = str(int(self.text) + 1)\n";

    fn make_button_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-sess-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::write(
            dir.join("scenes").join("main.pitescene"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"btn\"\ntype = \"Button\"\nname = \"HitBtn\"\nparent = \"root\"\n\n[node.props]\nposition = [100.0, 100.0]\nsize = [120.0, 40.0]\ntext = \"0\"\n\n[node.script]\npath = \"res://scripts/counter.py\"\nclass = \"Counter\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("scripts").join("counter.py"), COUNTER).unwrap();
        dir
    }

    fn btn_text(session: &GameSession) -> String {
        session.host.with_tree(|t| match t.get(&NodeId::from("btn".to_string())) {
            Some(n) => match n.props.get("text") {
                Some(PropValue::Str(s)) => s.clone(),
                _ => String::new(),
            },
            None => String::from("<gone>"),
        })
    }

    fn click(session: &mut GameSession, down: (f64, f64), up: (f64, f64)) {
        use pite_script::{input_begin_frame, input_set_key, input_set_mouse};
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
        let scene = dir.join("scenes").join("main.pitescene");
        let mut session = GameSession::open(&scene, true).unwrap();
        assert_eq!(btn_text(&session), "0");
        click(&mut session, (500.0, 400.0), (500.0, 400.0));
        assert_eq!(btn_text(&session), "1");
        click(&mut session, (500.0, 400.0), (500.0, 400.0));
        assert_eq!(btn_text(&session), "2");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn click_outside_is_ignored() {
        let _guard = SERIAL.lock().unwrap();
        let dir = make_button_project("btn-out");
        let scene = dir.join("scenes").join("main.pitescene");
        let mut session = GameSession::open(&scene, true).unwrap();
        click(&mut session, (700.0, 500.0), (700.0, 500.0));
        assert_eq!(btn_text(&session), "0");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn press_inside_release_outside_is_ignored() {
        let _guard = SERIAL.lock().unwrap();
        let dir = make_button_project("btn-drag");
        let scene = dir.join("scenes").join("main.pitescene");
        let mut session = GameSession::open(&scene, true).unwrap();
        click(&mut session, (500.0, 400.0), (700.0, 500.0));
        assert_eq!(btn_text(&session), "0");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dropped_button_disconnects_silently() {
        let _guard = SERIAL.lock().unwrap();
        let dir = make_button_project("btn-drop");
        let scene = dir.join("scenes").join("main.pitescene");
        let mut session = GameSession::open(&scene, true).unwrap();
        session.host.with_tree_mut(|t| {
            t.remove(&NodeId::from("btn".to_string())).unwrap();
        });
        session.host.disconnect_node("btn");
        assert!(!session.host.has_signal("btn", "pressed"));
        click(&mut session, (500.0, 400.0), (500.0, 400.0));
        assert_eq!(btn_text(&session), "<gone>");
        std::fs::remove_dir_all(&dir).ok();
    }

    const WALKER: &str = "import pite\n\nclass Walker(pite.Node2D):\n    def _process(self, delta):\n        x, y = self.position\n        if pite.held(\"T-WalkRight\"):\n            x += 200.0 * delta\n        if pite.pressed(\"T-Step\"):\n            x += 10.0\n        self.position = (x, y)\n";

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
        let scene = dir.join("scenes").join("main.pitescene");
        let mut session = GameSession::open(&scene, true).unwrap();

        input_begin_frame();
        input_set_key("T-WalkRight", true);
        session.update(0.5);
        assert!((walker_x(&session) - 100.0).abs() < 1e-6);

        input_begin_frame();
        input_set_key("T-Step", true);
        session.update(0.5);
        let after_step = walker_x(&session);
        assert!((after_step - 210.0).abs() < 1e-6);

        input_begin_frame();
        session.update(0.5);
        assert!((walker_x(&session) - 310.0).abs() < 1e-6);
        input_set_key("T-WalkRight", false);
        input_set_key("T-Step", false);
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

    #[test]
    fn dogfood_player_hit_reaches_enemy() {
        let _guard = SERIAL.lock().unwrap();
        use pite_script::{input_begin_frame, input_set_key};

        let scene = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("examples")
            .join("minimal-2d")
            .join("scenes")
            .join("main.pitescene");
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
}
