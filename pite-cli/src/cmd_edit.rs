use std::path::PathBuf;

use anyhow::{Context, Result};

pub fn run(scene: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let path = match scene {
        Some(s) if !s.is_empty() => {
            if let Some(rel) = s.strip_prefix("res://") {
                let root = pite_project::find_project_root(&cwd)
                    .with_context(|| "scene uses res:// but no pite.toml found above cwd")?;
                root.join(rel)
            } else {
                let p = PathBuf::from(s);
                if p.is_absolute() {
                    p
                } else {
                    cwd.join(p)
                }
            }
        }
        _ => {
            let root = pite_project::find_project_root(&cwd).unwrap_or(cwd.clone());
            let manifest = pite_project::load_manifest(&root)?;
            let main = manifest.project.main_scene;
            if let Some(rel) = main.strip_prefix("res://") {
                root.join(rel)
            } else {
                root.join(main)
            }
        }
    };
    pite_editor::launch(&path)
}
