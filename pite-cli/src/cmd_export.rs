// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::PathBuf;

use anyhow::Result;

pub fn run(
    platform: &str,
    path: Option<&str>,
    out: Option<&str>,
    binary: Option<&str>,
    skip_python_check: bool,
) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let start = match path {
        Some(p) => {
            let pb = PathBuf::from(p);
            if pb.is_absolute() {
                pb
            } else {
                cwd.join(pb)
            }
        }
        None => cwd,
    };
    let root = pite_project::find_project_root(&start).unwrap_or_else(|| {
        let dogfood = start.join("examples").join("minimal-2d");
        if dogfood.join(pite_project::MANIFEST_FILE).is_file() {
            dogfood
        } else {
            start.clone()
        }
    });
    let report = pite_export::export_project(
        &root,
        &pite_export::ExportOptions {
            platform: platform.to_string(),
            out_dir: out.map(PathBuf::from),
            binary: binary.map(PathBuf::from),
            skip_python_check,
        },
    )?;
    println!("exported to {}", report.out_dir.display());
    println!("binary: {}", report.binary.display());
    println!("files: {}", report.files.len() + 1);
    if platform == "windows" {
        println!("next: run {}\\run.bat", report.out_dir.display());
    } else {
        println!("next: run {}/run.sh", report.out_dir.display());
    }
    Ok(())
}
