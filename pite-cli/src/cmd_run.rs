// SPDX-License-Identifier: MIT OR Apache-2.0
use anyhow::Result;

pub fn run(scene: &str, no_reload: bool) -> Result<()> {
    let path = pite_runtime::resolve_scene_arg(scene)?;
    pite_runtime::run_scene(&path, no_reload)
}
