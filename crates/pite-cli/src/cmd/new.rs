// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite new`: scaffold a project from a template (or blank).

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::output::{Outcome, SCHEMA_VERSION};
use crate::text::Spans;
use crate::ui::Ui;

#[derive(Debug, Serialize)]
struct NewData {
    schema_version: u32,
    name: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    template: Option<String>,
}

pub fn run(ui: &mut Ui, name: &str, template: Option<&str>) -> Result<Outcome> {
    let cwd = std::env::current_dir()?;
    let dir = pite_project::create_project(Path::new(&cwd), name, template)?;
    let _ = pite_assets::reimport_project(&dir);
    if let Err(err) = pite_project::trust_workspace(&dir) {
        tracing::warn!(path = %dir.display(), "cannot mark new project trusted: {err:#}");
    }
    match template {
        Some(template) => ui.success(
            Spans::new()
                .plain("Created project ")
                .strong(dir.display().to_string())
                .plain(" from template ")
                .info(template)
                .plain("."),
        ),
        None => ui.success(
            Spans::new()
                .plain("Created blank project ")
                .strong(dir.display().to_string())
                .plain("."),
        ),
    }
    if !ui.quiet() {
        ui.blank();
        ui.info(Spans::new().info("Get started with:"));
    }
    ui.info(Spans::new().plain("  cd ").strong(name));
    ui.info(Spans::new().plain("  ").strong("pite edit"));
    ui.verbose_line(&format!(
        "  manifest: {}",
        dir.join(pite_project::MANIFEST_FILE).display()
    ));

    Outcome::completed(NewData {
        schema_version: SCHEMA_VERSION,
        name: name.to_string(),
        path: dir.display().to_string(),
        template: template.map(str::to_string),
    })
}
