// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn resolve_scene_arg(scene: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    if scene.starts_with("res://") {
        let root = pite_project::find_project_root(&cwd)
            .with_context(|| "scene uses res:// but no pite.toml found above cwd")?;
        return pite_core::resolve_res(&root, scene).map_err(anyhow::Error::from);
    }
    let p = PathBuf::from(scene);
    Ok(if p.is_absolute() { p } else { cwd.join(p) })
}

pub fn resolve_default_scene() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let root = pite_project::find_project_root(&cwd)
        .with_context(|| "no scene given and no pite.toml found above cwd")?;
    let main = pite_project::load_manifest(&root)?.project.main_scene;
    pite_core::resolve_project_ref(Some(&root), &root, &main).map_err(anyhow::Error::from)
}

pub(crate) fn resolve_script(
    script_ref: &str,
    scene_dir: &Path,
    project_dir: Option<&Path>,
) -> Option<PathBuf> {
    pite_core::resolve_project_ref(project_dir, scene_dir, script_ref).ok()
}
