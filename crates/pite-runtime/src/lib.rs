// SPDX-License-Identifier: MIT OR Apache-2.0
//! Runtime host and main game loop orchestrator for the Pite game engine.
//!
//! Manages window creation (`winit`), fixed 60 Hz physics stepping, variable
//! rendering updates, live scene and script hot reloading, and input dispatch.

mod app;
mod input;
mod scenes;
mod session;

pub use app::{run_scene, run_with_options, RunOptions, DEFAULT_ICON_PNG};
pub use scenes::{resolve_default_scene, resolve_scene_arg};
pub use session::GameSession;
