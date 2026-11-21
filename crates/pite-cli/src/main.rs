// SPDX-License-Identifier: MIT OR Apache-2.0
//! Command-line interface and development entrypoint for Pite.
//!
//! Provides the `pite` executable for project scaffolding (`pite new`), visual editing
//! (`pite edit`), standalone gameplay (`pite run`), project verification (`pite check`),
//! cross-platform desktop exports (`pite export`), dependency vendoring (`pite vendor`),
//! and toolchain management (`pite toolchain`).
//!
//! Every command receives the shared [`ui::Ui`] built here, so color policy,
//! `--json`, `--quiet`, and `--verbose` behave identically everywhere.

mod cmd;
mod diagnostics;
mod output;
mod text;
mod ui;

use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

use crate::ui::{ColorChoice, Ui};

#[derive(Debug, Parser)]
#[command(name = "pite", version, about = "Pite game engine CLI")]
struct Cli {
    /// Control colored output: auto, always, never.
    #[arg(
        long,
        value_name = "WHEN",
        value_enum,
        default_value = "auto",
        global = true
    )]
    color: ColorChoice,
    /// Emit a machine-readable JSON envelope on stdout.
    #[arg(long, global = true, default_value_t = false)]
    json: bool,
    /// Print extra detail about the command's work.
    #[arg(short, long, global = true, default_value_t = false)]
    verbose: bool,
    /// Suppress progress, success, and table output.
    #[arg(short, long, global = true, default_value_t = false)]
    quiet: bool,
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
        /// Re-scan assets, reconcile asset manifest, and update references before checking.
        #[arg(long, default_value_t = false)]
        reimport_assets: bool,
        /// Treat warnings as errors and exit with a non-zero status.
        #[arg(long, default_value_t = false)]
        strict: bool,
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
        action: cmd::toolchain::ToolchainAction,
    },
    /// Manage third-party vendored Python packages.
    Vendor {
        #[command(subcommand)]
        action: cmd::vendor::VendorAction,
    },
}

fn main() {
    std::process::exit(run_cli());
}

fn run_cli() -> i32 {
    let wants_json = std::env::args().any(|arg| arg == "--json");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            use clap::error::ErrorKind;
            match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    let _ = error.print();
                    return 0;
                }
                _ if wants_json => {
                    println!(
                        "{}",
                        output::usage_error_envelope("pite", &error.to_string())
                    );
                    return 2;
                }
                _ => {
                    let _ = error.print();
                    return error.exit_code();
                }
            }
        }
    };

    let mut ui = Ui::stdio(cli.color, cli.json, cli.quiet, cli.verbose);
    init_tracing(&ui);

    // No subcommand outside a project: help on stdout, or an error envelope
    // when a machine-readable report was requested.
    if cli.command.is_none() && ui.json() {
        let cwd = std::env::current_dir().unwrap_or_default();
        if pite_project::find_project_root(&cwd).is_none() {
            println!(
                "{}",
                output::usage_error_envelope("pite", "no command given")
            );
            return 2;
        }
    }

    let command = command_name(&cli.command);
    match dispatch(&mut ui, cli.command) {
        Ok(outcome) => {
            if ui.json() {
                match output::envelope(command, &outcome) {
                    Ok(text) => {
                        ui.write_out(&text);
                        ui.write_out("\n");
                    }
                    Err(error) => {
                        output::print_error(&mut ui, &error);
                        return 1;
                    }
                }
            }
            i32::from(outcome.exit)
        }
        Err(error) => {
            if ui.json() {
                match output::error_envelope(command, &error) {
                    Ok(text) => {
                        ui.write_out(&text);
                        ui.write_out("\n");
                    }
                    Err(_) => output::print_error(&mut ui, &error),
                }
            } else {
                output::print_error(&mut ui, &error);
            }
            1
        }
    }
}

fn dispatch(ui: &mut Ui, command: Option<Command>) -> Result<output::Outcome> {
    match command {
        None => {
            let cwd = std::env::current_dir()?;
            if pite_project::find_project_root(&cwd).is_some() {
                cmd::edit::run(ui, None)
            } else {
                let _ = Cli::command().print_help();
                ui.write_out("\n");
                Ok(output::Outcome::silent())
            }
        }
        Some(Command::New { name, template }) => cmd::new::run(ui, &name, template.as_deref()),
        Some(Command::Run { scene, no_reload }) => cmd::run::run(ui, scene.as_deref(), no_reload),
        Some(Command::Check {
            path,
            reimport_assets,
            strict,
        }) => cmd::check::run(ui, path.as_deref(), reimport_assets, strict),
        Some(Command::Edit { scene }) => cmd::edit::run(ui, scene.as_deref()),
        Some(Command::Export {
            platform,
            path,
            out,
            binary,
            skip_python_check,
            no_bundle_python,
        }) => cmd::export::run(
            ui,
            &platform,
            path.as_deref(),
            out.as_deref(),
            binary.as_deref(),
            skip_python_check,
            no_bundle_python,
        ),
        Some(Command::Toolchain { action }) => cmd::toolchain::run(ui, action),
        Some(Command::Vendor { action }) => cmd::vendor::run(ui, action),
    }
}

fn command_name(command: &Option<Command>) -> &'static str {
    match command {
        None => "pite",
        Some(Command::New { .. }) => "new",
        Some(Command::Run { .. }) => "run",
        Some(Command::Check { .. }) => "check",
        Some(Command::Edit { .. }) => "edit",
        Some(Command::Export { .. }) => "export",
        Some(Command::Toolchain { .. }) => "toolchain",
        Some(Command::Vendor { .. }) => "vendor",
    }
}

/// Logs always go to stderr, honor `RUST_LOG` when set, and use the same color
/// decision as human output. `--verbose` only adds command detail; it never
/// raises the log level, so engine debug logs stay out of normal verbose runs.
fn init_tracing(ui: &Ui) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::from_default_env();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(ui.color_enabled())
        .try_init();
}

pub(crate) fn resolve_scene(scene: Option<&str>) -> Result<PathBuf> {
    match scene {
        Some(s) if !s.is_empty() => pite_runtime::resolve_scene_arg(s),
        _ => pite_runtime::resolve_default_scene(),
    }
}

pub(crate) fn resolve_start_dir(path: Option<&str>) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    Ok(match path {
        Some(p) => {
            let pb = PathBuf::from(p);
            if pb.is_absolute() {
                pb
            } else {
                cwd.join(pb)
            }
        }
        None => cwd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("global flags parse")
    }

    #[test]
    fn global_flags_are_accepted_after_a_subcommand() {
        let cli = parse(&["pite", "check", "--json", "--color", "always", "-q", "-v"]);
        assert!(cli.json);
        assert!(cli.quiet);
        assert!(cli.verbose);
        assert_eq!(cli.color, ColorChoice::Always);
    }

    #[test]
    fn global_flags_default_to_human_auto_output() {
        let cli = parse(&["pite", "check"]);
        assert!(!cli.json);
        assert!(!cli.quiet);
        assert!(!cli.verbose);
        assert_eq!(cli.color, ColorChoice::Auto);
    }

    #[test]
    fn check_keeps_its_own_flags() {
        let cli = parse(&["pite", "check", "proj", "--reimport-assets", "--strict"]);
        match cli.command {
            Some(Command::Check {
                path,
                reimport_assets,
                strict,
            }) => {
                assert_eq!(path.as_deref(), Some("proj"));
                assert!(reimport_assets);
                assert!(strict);
            }
            other => panic!("expected check, got {other:?}"),
        }
    }

    #[test]
    fn toolchain_clean_takes_platform_lists() {
        let cli = parse(&[
            "pite",
            "toolchain",
            "clean",
            "--python",
            "macos",
            "--player",
            "windows,linux",
        ]);
        match cli.command {
            Some(Command::Toolchain {
                action:
                    cmd::toolchain::ToolchainAction::Clean {
                        python,
                        player,
                        all,
                    },
            }) => {
                assert_eq!(python, vec!["macos"]);
                assert_eq!(player, vec!["windows", "linux"]);
                assert!(!all);
            }
            other => panic!("expected toolchain clean, got {other:?}"),
        }
    }

    #[test]
    fn toolchain_clean_rejects_unknown_platforms() {
        assert!(Cli::try_parse_from(["pite", "toolchain", "clean", "--player", "beos"]).is_err());
    }

    #[test]
    fn every_command_has_a_stable_name() {
        assert_eq!(
            command_name(&Some(Command::Vendor {
                action: cmd::vendor::VendorAction::List
            })),
            "vendor"
        );
        assert_eq!(
            command_name(&Some(Command::Toolchain {
                action: cmd::toolchain::ToolchainAction::Status
            })),
            "toolchain"
        );
        assert_eq!(command_name(&None), "pite");
    }
}
