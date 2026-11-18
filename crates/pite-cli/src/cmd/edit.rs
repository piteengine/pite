// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite edit`: launch the graphical editor.

use anyhow::Result;
use serde::Serialize;

use crate::output::{Outcome, SCHEMA_VERSION};
use crate::text::Spans;
use crate::ui::Ui;

#[derive(Debug, Serialize)]
struct EditData {
    schema_version: u32,
    scene: String,
    status: &'static str,
}

pub fn run(ui: &mut Ui, scene: Option<&str>) -> Result<Outcome> {
    let path = crate::resolve_scene(scene)?;
    let label = path.to_string_lossy().to_string();
    ui.verbose_note(
        Spans::new()
            .info("Opening editor")
            .plain(" on ")
            .strong(&label),
    );
    pite_editor::launch(&path)?;
    // Emitted on exit only: the editor session is the command's lifetime.
    Outcome::completed(EditData {
        schema_version: SCHEMA_VERSION,
        scene: label,
        status: "exited",
    })
}
