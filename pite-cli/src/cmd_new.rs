// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::Path;

use anyhow::Result;

pub fn run(name: &str, template: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let dir = pite_project::create_project(Path::new(&cwd), name, template)?;
    println!(
        "created project {} from template {template:?}",
        dir.display()
    );
    Ok(())
}
