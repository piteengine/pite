// SPDX-License-Identifier: MIT OR Apache-2.0
use anyhow::Result;

pub fn run(scene: Option<&str>, no_reload: bool) -> Result<()> {
    let path = match scene {
        Some(s) if !s.is_empty() => pite_runtime::resolve_scene_arg(s)?,
        _ => pite_runtime::resolve_default_scene()?,
    };
    pite_runtime::run_scene(&path, no_reload)
}
