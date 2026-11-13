// SPDX-License-Identifier: MIT OR Apache-2.0
use anyhow::Result;

pub fn run(scene: Option<&str>, no_reload: bool) -> Result<()> {
    let path = crate::resolve_scene(scene)?;
    pite_runtime::run_scene(&path, no_reload)
}
