mod cmd_check;
mod cmd_edit;
mod cmd_new;
mod cmd_run;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "pite", version, about = "Pite game engine CLI (M0 scaffold)")]
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
        scene: String,
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
    Edit {
        #[arg(long)]
        scene: Option<String>,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    match cli.command {
        Command::New { name, template } => cmd_new::run(&name, &template),
        Command::Run { scene, no_reload } => cmd_run::run(&scene, no_reload),
        Command::Check { path, strict, json } => cmd_check::run(path.as_deref(), strict, json),
        Command::Edit { scene } => cmd_edit::run(scene.as_deref()),
    }
}
