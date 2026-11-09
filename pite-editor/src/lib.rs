// SPDX-License-Identifier: MIT OR Apache-2.0
//! Visual desktop editor for the Pite game engine.
//!
//! Built on `egui` and `eframe`, providing an interactive 2D scene viewport,
//! hierarchical scene tree, property inspector, asset manager, and integrated
//! code editor with Pyright language server support.

mod app;
mod highlight;
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

#[cfg(test)]
mod docs_tests {
    use pite_core::NODE_SCHEMAS;

    /// Every concrete schema type must have a generated `.md` on disk. If
    /// this fails, `pite-editor/build.rs` didn't run (`cargo build -p
    /// pite-editor` regenerates), or someone deleted a docs file by hand.
    #[test]
    fn generated_node_docs_cover_all_types() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let dir = root.join("docs").join("reference").join("nodes");
        assert!(
            dir.is_dir(),
            "docs dir missing: {} (run `cargo build -p pite-editor` once to generate)",
            dir.display()
        );
        for schema in NODE_SCHEMAS {
            if schema.abstract_class {
                continue;
            }
            let path = dir.join(format!("{}.md", schema.type_name));
            assert!(
                path.is_file(),
                "missing doc for {}: {}",
                schema.type_name,
                path.display()
            );
        }
        assert!(
            dir.join("index.md").is_file(),
            "docs index missing: {}",
            dir.join("index.md").display()
        );
    }
}
