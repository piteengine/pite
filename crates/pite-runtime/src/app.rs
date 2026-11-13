// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use pite_render::{Renderer2D, WgpuRenderer};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::input::key_name;
use crate::session::GameSession;

/// Configuration options for launching a game session runtime.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Path to the root scene file to load and execute.
    pub scene: PathBuf,
    /// Disables background filesystem watching and live hot reload.
    pub no_reload: bool,
}

pub const DEFAULT_ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

fn window_icon() -> Option<winit::window::Icon> {
    let img =
        image::load_from_memory_with_format(DEFAULT_ICON_PNG, image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    winit::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
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
