// SPDX-License-Identifier: MIT OR Apache-2.0
mod cmd_check;
mod cmd_edit;
mod cmd_export;
mod cmd_new;
mod cmd_reimport;
mod cmd_run;
mod cmd_vendor;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "pite", version, about = "Pite game engine CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    New {
        name: String,
        #[arg(long, default_value = "minimal-2d")]
        template: String,
    },
    Run {
        #[arg(long)]
        scene: Option<String>,
        #[arg(long, default_value_t = false)]
        no_reload: bool,
    },
    Check {
        #[arg(long)]
        path: Option<String>,
        #[arg(long, default_value_t = false)]
        strict: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Reimport {
        #[arg(long)]
        path: Option<String>,
    },
    Edit {
        #[arg(long)]
        scene: Option<String>,
    },
    Export {
        platform: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        out: Option<String>,
        #[arg(long)]
        binary: Option<String>,
        #[arg(long, default_value_t = false)]
        skip_python_check: bool,
        /// Keep the system-Python requirement instead of bundling a pinned one.
        #[arg(long, default_value_t = false)]
        no_bundle_python: bool,
    },
    Vendor {
        #[command(subcommand)]
        action: VendorAction,
    },
}

#[derive(Debug, Subcommand)]
enum VendorAction {
    /// Download wheels (plus transitive deps) into vendor/ and pin them.
    Add {
        /// `name` or `name==version`.
        spec: String,
        /// Keep the wheel file even for pure-Python packages.
        #[arg(long, default_value_t = false)]
        zipped: bool,
        /// Unpack into vendor/ even without native modules.
        #[arg(long, default_value_t = false)]
        unpacked: bool,
    },
    Remove {
        name: String,
    },
    List,
    Verify,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    match cli.command {
        Command::New { name, template } => cmd_new::run(&name, &template),
        Command::Run { scene, no_reload } => cmd_run::run(scene.as_deref(), no_reload),
        Command::Check { path, strict, json } => cmd_check::run(path.as_deref(), strict, json),
        Command::Reimport { path } => cmd_reimport::run(path.as_deref()),
        Command::Edit { scene } => cmd_edit::run(scene.as_deref()),
        Command::Export {
            platform,
            path,
            out,
            binary,
            skip_python_check,
            no_bundle_python,
        } => cmd_export::run(
            &platform,
            path.as_deref(),
            out.as_deref(),
            binary.as_deref(),
            skip_python_check,
            no_bundle_python,
        ),
        Command::Vendor { action } => match action {
            VendorAction::Add {
                spec,
                zipped,
                unpacked,
            } => cmd_vendor::add(&spec, zipped, unpacked),
            VendorAction::Remove { name } => cmd_vendor::remove(&name),
            VendorAction::List => cmd_vendor::list(),
            VendorAction::Verify => cmd_vendor::verify(),
        },
    }
}
