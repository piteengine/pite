// SPDX-License-Identifier: MIT OR Apache-2.0
use anyhow::Result;

pub fn run(scene: Option<&str>) -> Result<()> {
    let path = crate::resolve_scene(scene)?;
    pite_editor::launch(&path)
}
