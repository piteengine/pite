use std::path::PathBuf;

use anyhow::{Context, Result};

pub fn run(scene: &str, no_reload: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let path = if let Some(rel) = scene.strip_prefix("res://") {
        let root = pite_project::find_project_root(&cwd)
            .with_context(|| "scene uses res:// but no pite.toml found above cwd")?;
        root.join(rel)
    } else {
        let p = PathBuf::from(scene);
        if p.is_absolute() {
            p
        } else {
            cwd.join(p)
        }
    };
    pite_runtime::run_scene(&path, no_reload)
}
