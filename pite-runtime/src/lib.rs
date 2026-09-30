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
        };
        session.rebuild()?;
        pite_script::set_current_host(session.host.clone());
        if !no_reload {
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
        }
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

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes().with_title(self.title.clone());
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
        let (dir, _) = make_project("noreload", "Mover", V1);
        let scene = dir.join("scenes").join("main.pitescene");
        let session = GameSession::open(&scene, true).unwrap();
        assert!(session.watch_rx.is_none());
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
}
