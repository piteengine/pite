// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::Path;

use anyhow::Result;

pub fn run(name: &str, template: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let dir = pite_project::create_project(Path::new(&cwd), name, template)?;
    match template {
        Some(t) => println!("created project {} from template {t:?}", dir.display()),
        None => println!("created blank project {}", dir.display()),
    }
    println!("\nGet started with:\n  cd {name}\n  pite edit");
    Ok(())
}
