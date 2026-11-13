// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn resolve_scene_arg(scene: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    if let Some(rel) = scene.strip_prefix("res://") {
        let root = pite_project::find_project_root(&cwd)
            .with_context(|| "scene uses res:// but no pite.toml found above cwd")?;
        return Ok(root.join(rel));
    }
    let p = PathBuf::from(scene);
    Ok(if p.is_absolute() { p } else { cwd.join(p) })
}

pub fn resolve_default_scene() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let root = pite_project::find_project_root(&cwd)
        .with_context(|| "no scene given and no pite.toml found above cwd")?;
    let main = pite_project::load_manifest(&root)?.project.main_scene;
    Ok(match main.strip_prefix("res://") {
        Some(rel) => root.join(rel),
        None => root.join(main),
    })
}

pub(crate) fn resolve_script(
    script_ref: &str,
    scene_dir: &Path,
    project_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(rel) = script_ref.strip_prefix("res://") {
        project_dir.map(|root| root.join(rel))
    } else {
        let p = PathBuf::from(script_ref);
        if p.is_absolute() {
            Some(p)
        } else {
            Some(scene_dir.join(p))
        }
    }
}
