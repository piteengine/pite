// SPDX-License-Identifier: MIT OR Apache-2.0
//! Lean export player: runs one scene with no hot reload and no editor.
//! Exported launchers invoke this, never `pite` itself.

use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "pite-player", version, about = "Pite export player")]
struct Cli {
    #[arg(long)]
    scene: String,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    let path = pite_runtime::resolve_scene_arg(&cli.scene)?;
    pite_runtime::run_scene(&path, true)
}
