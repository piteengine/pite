//! `pite-editor`: `Panel` trait plus panel registry.
//! New panels register without editing dock layout code (M1c fills this in).

use anyhow::Result;

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
