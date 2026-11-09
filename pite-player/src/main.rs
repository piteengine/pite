// SPDX-License-Identifier: MIT OR Apache-2.0
//! Lean standalone player binary for the Pite game engine.
//!
//! Executes exported scenes directly without editor tooling or development
//! dependencies, minimizing binary footprint and startup overhead.

use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "pite-player", version, about = "Pite export player")]
struct Cli {
    /// Scene file to run (res:// path or filesystem path).
    #[arg(value_name = "SCENE")]
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
