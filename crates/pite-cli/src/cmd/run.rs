// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite run`: launch a scene in the standalone runtime.

use anyhow::Result;
use serde::Serialize;

use crate::output::{Outcome, SCHEMA_VERSION};
use crate::text::Spans;
use crate::ui::Ui;

#[derive(Debug, Serialize)]
struct RunData {
    schema_version: u32,
    scene: String,
    status: &'static str,
}

pub fn run(ui: &mut Ui, scene: Option<&str>, no_reload: bool) -> Result<Outcome> {
    let path = crate::resolve_scene(scene)?;
    let label = path.to_string_lossy().to_string();
    let mut note = Spans::new().info("Running").plain(" ").strong(&label);
    if !no_reload {
        note = note.plain(" ").faded("(hot reload enabled)");
    }
    ui.verbose_note(note);
    pite_runtime::run_scene(&path, no_reload)?;
    // Emitted on exit only: the window lifetime is the command's lifetime.
    Outcome::completed(RunData {
        schema_version: SCHEMA_VERSION,
        scene: label,
        status: "exited",
    })
}
