// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::PathBuf;

use anyhow::Result;

pub fn run(path: Option<&str>) -> Result<()> {
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
    let root = pite_project::find_project_root(&start).or_else(|| {
        let dogfood = start.join("examples").join("minimal-2d");
        if dogfood.join(pite_project::MANIFEST_FILE).is_file() {
            Some(dogfood)
        } else {
            None
        }
    });
    let root = match root {
        Some(root) => root,
        None => anyhow::bail!(
            "no {} found above {}",
            pite_project::MANIFEST_FILE,
            start.display()
        ),
    };
    let old = pite_assets::load_registry(&root).unwrap_or_default();
    let files = pite_assets::scan_files(&root).unwrap_or_default();
    let (_next, scan) = pite_assets::reconcile(&old, &files);
    let (_reg, report) = pite_assets::reimport_project(&root)?;
    println!("scanned {} asset(s)", scan.scanned);
    for added in &scan.added {
        println!("added: {added}");
    }
    for (old_path, new_path) in &scan.renamed {
        println!("renamed: {old_path} -> {new_path}");
    }
    for removed in &scan.removed {
        println!("removed: {removed}");
    }
    for changed in &scan.changed {
        println!("changed: {changed}");
    }
    println!("reimported {} asset(s)", report.reimported.len());
    for r in &report.reimported {
        println!("reimported: {r}");
    }
    for r in &report.refs_updated {
        println!("refs updated: {r}");
    }
    if !report.failed.is_empty() {
        for f in &report.failed {
            println!("failed: {f}");
        }
        anyhow::bail!("reimport failed");
    }
    Ok(())
}
