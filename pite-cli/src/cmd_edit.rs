// SPDX-License-Identifier: MIT OR Apache-2.0
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
        _ => pite_runtime::resolve_default_scene()?,
    };
    pite_editor::launch(&path)
}
