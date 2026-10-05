// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-editor`: `Panel` trait plus panel registry.
//! New panels register without editing dock layout code.
//! The eframe shell provides viewport, tree, inspector, assets, console
//! and code pane — all on the same `NodeTree` the game runs.

mod app;
mod icons;
pub mod lsp;
pub mod ops;
pub mod theme;

use anyhow::Result;

pub use app::{launch, EditorApp};

pub trait Panel {
    fn title(&self) -> &str;
    fn draw(&self) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct PanelRegistry {
    panels: Vec<String>,
}

impl PanelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, title: impl Into<String>) {
        self.panels.push(title.into());
    }

    pub fn titles(&self) -> &[String] {
        &self.panels
    }
}

#[derive(Debug, Default)]
pub struct Editor {
    pub panels: PanelRegistry,
}

impl Editor {
    pub fn new() -> Self {
        Self::default()
    }
}
