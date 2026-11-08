// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-editor`: `Panel` trait plus panel registry.
//! New panels register without editing dock layout code.
//! The eframe shell provides viewport, tree, inspector, assets, console
//! and code pane — all on the same `NodeTree` the game runs.

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
