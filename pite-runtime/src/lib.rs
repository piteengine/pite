//! `pite-runtime`: thin orchestrator wiring scene + render + script.
//! Opens a window, loads the scene file into a node tree, runs the loop.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use pite_core::{NodeId, NodeTree, PropValue};
use pite_script::{Pyo3Backend, ScriptBackend};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub scene: PathBuf,
    pub no_reload: bool,
}

struct ScriptSlot {
    id: NodeId,
    backend: Box<dyn ScriptBackend>,
    errored: bool,
}

struct App {
    window: Option<Window>,
    title: String,
    tree: NodeTree,
    scripts: Vec<ScriptSlot>,
    last_frame: Instant,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes().with_title(self.title.clone());
            match event_loop.create_window(attrs) {
                Ok(window) => self.window = Some(window),
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
            WindowEvent::RedrawRequested => {
                let delta = self.last_frame.elapsed().as_secs_f64().min(0.1);
                self.last_frame = Instant::now();
                for slot in &mut self.scripts {
                    if slot.errored {
                        continue;
                    }
                    if let Err(e) = slot.backend.call_process(delta) {
                        tracing::error!(node = %slot.id, "{e:#}");
                        slot.errored = true;
                    }
                    if let Some((x, y)) = slot.backend.position() {
                        if let Some(node) = self.tree.get_mut(&slot.id) {
                            node.props.insert("position".to_string(), PropValue::Vec2(x, y));
                        }
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

pub fn run_scene(scene: &Path, no_reload: bool) -> Result<()> {
    run_with_options(&RunOptions {
        scene: scene.to_path_buf(),
        no_reload,
    })
}

pub fn run_with_options(options: &RunOptions) -> Result<()> {
    let doc = pite_scene::load_scene(&options.scene)
        .with_context(|| format!("cannot load scene {}", options.scene.display()))?;
    let scene_dir = options
        .scene
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let project_dir = pite_project::find_project_root(&options.scene);
    let tree = pite_scene::build_tree(&doc, &scene_dir, project_dir.as_deref())?;
    let title = format!(
        "Pite — {} ({} nodes)",
        options.scene.display(),
        tree.len()
    );
    tracing::info!(
        scene = %options.scene.display(),
        nodes = tree.len(),
        no_reload = options.no_reload,
        "scene loaded"
    );

    let mut scripts = Vec::new();
    for node in tree.iter() {
        let Some(script) = &node.script else {
            continue;
        };
        let Some(path) = resolve_script(&script.path, &scene_dir, project_dir.as_deref()) else {
            continue;
        };
        if !path.is_file() {
            tracing::warn!(
                node = %node.id,
                script = %script.path,
                "missing script, keeping placeholder node"
            );
            continue;
        }
        let mut backend = Box::new(Pyo3Backend::new(node.id.to_string()));
        match backend.load(&path.to_string_lossy(), &script.class_name) {
            Ok(()) => {
                if let Some((x, y)) = seed_position(&tree, &node.id) {
                    backend.set_position(x, y);
                }
                let mut errored = false;
                if let Err(e) = backend.call_ready() {
                    tracing::error!(node = %node.id, "{e:#}");
                    errored = true;
                }
                scripts.push(ScriptSlot {
                    id: node.id.clone(),
                    backend,
                    errored,
                });
            }
            Err(e) => {
                tracing::error!(node = %node.id, "cannot load script: {e:#}");
            }
        }
    }
    tracing::info!(scripts = scripts.len(), "scripts attached");

    let event_loop = EventLoop::new().context("cannot create event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        window: None,
        title,
        tree,
        scripts,
        last_frame: Instant::now(),
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
