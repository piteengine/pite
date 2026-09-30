//! `pite-runtime`: thin orchestrator wiring scene + render + script.
//! Opens a window, loads the scene file into a node tree, runs the loop.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub scene: PathBuf,
    pub no_reload: bool,
}

struct App {
    window: Option<Window>,
    title: String,
    scene_nodes: usize,
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

    let event_loop = EventLoop::new().context("cannot create event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        window: None,
        title,
        scene_nodes: tree.len(),
    };
    let _ = app.scene_nodes;
    event_loop
        .run_app(&mut app)
        .context("event loop failed")?;
    Ok(())
}
