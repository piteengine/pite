// SPDX-License-Identifier: MIT OR Apache-2.0
//! Command-line interface and development entrypoint for Pite.
//!
//! Provides the `pite` executable for project scaffolding (`pite new`), visual editing
//! (`pite edit`), standalone gameplay (`pite run`), project verification (`pite check`),
//! cross-platform desktop exports (`pite export`), dependency vendoring (`pite vendor`),
//! and toolchain management (`pite toolchain`).

mod cmd_check;
mod cmd_edit;
mod cmd_export;
mod cmd_new;
mod cmd_run;
mod cmd_toolchain;
mod cmd_vendor;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "pite", version, about = "Pite game engine CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scaffold a new game project from a template (or blank).
    New {
        /// Name of the new project directory.
        name: String,
        /// Project template to scaffold (available: "minimal-2d", "gem-collector", or omit for blank).
        #[arg(long, value_parser = ["minimal-2d", "gem-collector"])]
        template: Option<String>,
    },
    /// Run a scene in the standalone engine runtime.
    Run {
        /// Scene file to run (res:// path or filesystem path); defaults to the main scene.
        #[arg(value_name = "SCENE")]
        scene: Option<String>,
        /// Disable hot reloading of scripts and assets.
        #[arg(long, default_value_t = false)]
        no_reload: bool,
    },
    /// Validate project manifests, scene files, assets, and Python scripts.
    Check {
        /// Path to the project root directory; defaults to current directory.
        #[arg(value_name = "PATH")]
        path: Option<String>,
        /// Re-scan assets, reconcile UID registry, and update references before checking.
        #[arg(long, default_value_t = false)]
        reimport_assets: bool,
        /// Treat warnings as errors and exit with a non-zero status.
        #[arg(long, default_value_t = false)]
        strict: bool,
        /// Output the check report formatted as JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Control colored output: auto, always, never.
        #[arg(long, value_name = "WHEN", default_value = "auto")]
        color: cmd_check::ColorChoice,
        /// Show verbose output including scene cache hit/miss details.
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// Launch the interactive graphical scene and script editor.
    Edit {
        /// Scene file to open initially; defaults to the project's main scene.
        #[arg(value_name = "SCENE")]
        scene: Option<String>,
    },
    /// Export a playable build with runtime and resolved game content.
    Export {
        /// Target platform.
        #[arg(value_parser = ["linux", "windows", "macos"])]
        platform: String,
        /// Path to the project root directory; defaults to current directory.
        #[arg(long)]
        path: Option<String>,
        /// Output directory for the exported build.
        #[arg(long)]
        out: Option<String>,
        /// Path to a custom player binary to ship in the export.
        #[arg(long)]
        binary: Option<String>,
        /// Skip the host Python version verification check.
        #[arg(long, default_value_t = false)]
        skip_python_check: bool,
        /// Keep the system-Python requirement instead of bundling a pinned CPython runtime.
        #[arg(long, default_value_t = false)]
        no_bundle_python: bool,
    },
    /// Manage engine toolchains (CPython runtimes and export players).
    Toolchain {
        #[command(subcommand)]
        action: cmd_toolchain::ToolchainAction,
    },
    /// Manage third-party vendored Python packages.
    Vendor {
        #[command(subcommand)]
        action: cmd_vendor::VendorAction,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    match cli.command {
        None => {
            let cwd = std::env::current_dir()?;
            if pite_project::find_project_root(&cwd).is_some() {
                cmd_edit::run(None)
            } else {
                Cli::command().print_help()?;
                println!();
                Ok(())
            }
        }
        Some(Command::New { name, template }) => cmd_new::run(&name, template.as_deref()),
        Some(Command::Run { scene, no_reload }) => cmd_run::run(scene.as_deref(), no_reload),
        Some(Command::Check {
            path,
            reimport_assets,
            strict,
            json,
            color,
            verbose,
        }) => cmd_check::run(
            path.as_deref(),
            reimport_assets,
            strict,
            json,
            color,
            verbose,
        ),
        Some(Command::Edit { scene }) => cmd_edit::run(scene.as_deref()),
        Some(Command::Export {
            platform,
            path,
            out,
            binary,
            skip_python_check,
            no_bundle_python,
        }) => cmd_export::run(
            &platform,
            path.as_deref(),
            out.as_deref(),
            binary.as_deref(),
            skip_python_check,
            no_bundle_python,
        ),
        Some(Command::Toolchain { action }) => cmd_toolchain::run(action),
        Some(Command::Vendor { action }) => cmd_vendor::run(action),
    }
}
