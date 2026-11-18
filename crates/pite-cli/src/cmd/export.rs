// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite export`: package a playable build for one platform.

use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use crate::output::{Outcome, SCHEMA_VERSION};
use crate::text::Spans;
use crate::ui::{BarTone, Ui};

#[derive(Debug, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum PythonData {
    Bundled { zip: String },
    System,
}

#[derive(Debug, Serialize)]
struct ExportData {
    schema_version: u32,
    out_dir: String,
    binary: String,
    files: usize,
    python: PythonData,
}

pub fn run(
    ui: &mut Ui,
    platform: &str,
    path: Option<&str>,
    out: Option<&str>,
    binary: Option<&str>,
    skip_python_check: bool,
    no_bundle_python: bool,
) -> Result<Outcome> {
    let start = crate::resolve_start_dir(path)?;
    let root = pite_project::find_project_root(&start).ok_or_else(|| {
        anyhow::anyhow!(
            "no {} found above {}; run inside a project or pass --path",
            pite_project::MANIFEST_FILE,
            start.display()
        )
    })?;
    let report = pite_export::export_project(
        &root,
        &pite_export::ExportOptions {
            platform: platform.to_string(),
            out_dir: out.map(PathBuf::from),
            binary: binary.map(PathBuf::from),
            skip_python_check,
            bundle_python: !no_bundle_python,
        },
        ui,
    )?;

    let out_dir = report.out_dir.display().to_string();
    // Human paths follow the build's own platform: a Windows export shows
    // backslashes, a Linux/macOS export forward slashes, even when built on
    // the other kind of host.
    let out_display = target_path(&out_dir, platform);
    let binary_path = report.binary.display().to_string();
    let binary_display = target_path(&binary_path, platform);
    let files = report.files.len() + 1;
    let python = match &report.python {
        pite_export::PythonChoice::Bundled { zip } => PythonData::Bundled {
            zip: zip.display().to_string(),
        },
        pite_export::PythonChoice::System => PythonData::System,
    };

    if !ui.quiet() {
        ui.blank();
        let python_label = match &python {
            PythonData::Bundled { .. } => "Python bundled",
            PythonData::System => "Python system",
        };
        ui.bar(
            &format!("Exported to {out_display}"),
            &format!("{files} files · {python_label}"),
            BarTone::Success,
        );
    }
    ui.detail("Binary", Spans::new().strong(&binary_display));
    ui.detail("Files", files.to_string());
    match &python {
        PythonData::Bundled { zip } => ui.detail(
            "Python",
            Spans::new().plain("bundled (").strong(zip).plain(")"),
        ),
        PythonData::System => ui.detail("Python", "system CPython 3.12 required"),
    }
    ui.blank();
    let launch = if platform == "windows" {
        format!("{out_display}\\run.bat")
    } else {
        format!("{out_display}/run.sh")
    };
    ui.info(Spans::new().info("Next:").plain(" run ").strong(launch));

    Outcome::completed(ExportData {
        schema_version: SCHEMA_VERSION,
        out_dir,
        binary: binary_path,
        files,
        python,
    })
}

/// An exported-build path rendered with the target platform's separator.
fn target_path(path: &str, platform: &str) -> String {
    if platform == "windows" {
        path.replace('/', "\\")
    } else {
        path.replace('\\', "/")
    }
}
