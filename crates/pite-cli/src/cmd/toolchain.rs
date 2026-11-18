// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite toolchain`: inspect, download, verify, and clean export toolchains.

use anyhow::Result;
use clap::Subcommand;
use serde::Serialize;

use crate::output::{Outcome, SCHEMA_VERSION};
use crate::text::{capitalize, Spans};
use crate::ui::{BarTone, Cell, Table, Ui};

#[derive(Debug, Subcommand)]
pub enum ToolchainAction {
    /// Show the cache status of Python runtimes and player binaries.
    Status,
    /// Download toolchain artifacts (players and Python runtimes) for offline export.
    Download {
        /// Target platform (linux, windows, macos). Defaults to all platforms.
        #[arg(value_parser = ["linux", "windows", "macos"])]
        platform: Option<String>,
        /// Download only player binaries.
        #[arg(long, default_value_t = false)]
        player: bool,
        /// Download only Python runtimes.
        #[arg(long, default_value_t = false)]
        python: bool,
    },
    /// Verify checksums and integrity of cached toolchains.
    Verify,
    /// Remove cached toolchain artifacts (players and/or Python runtimes).
    Clean {
        /// Platforms whose Python runtimes to remove (comma separated).
        #[arg(long, value_delimiter = ',', value_parser = ["linux", "windows", "macos"])]
        python: Vec<String>,
        /// Platforms whose player binaries to remove (comma separated).
        #[arg(long, value_delimiter = ',', value_parser = ["linux", "windows", "macos"])]
        player: Vec<String>,
        /// Remove the entire toolchain cache.
        #[arg(long, default_value_t = false)]
        all: bool,
    },
}

#[derive(Debug, Serialize)]
struct Artifact {
    kind: &'static str,
    platform: String,
    status: &'static str,
    detail: String,
}

#[derive(Debug, Serialize)]
struct StatusData {
    schema_version: u32,
    dir: String,
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Serialize)]
struct DownloadData {
    schema_version: u32,
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Serialize)]
struct VerifyData {
    schema_version: u32,
    ok: bool,
    verified: usize,
    failed: usize,
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Serialize)]
struct CleanData {
    schema_version: u32,
    dir: String,
    removed: Vec<Artifact>,
}

pub fn run(ui: &mut Ui, action: ToolchainAction) -> Result<Outcome> {
    match action {
        ToolchainAction::Status => status(ui),
        ToolchainAction::Download {
            platform,
            player,
            python,
        } => download(ui, platform.as_deref(), player, python),
        ToolchainAction::Verify => verify(ui),
        ToolchainAction::Clean {
            python,
            player,
            all,
        } => clean(ui, &python, &player, all),
    }
}

fn status(ui: &mut Ui) -> Result<Outcome> {
    let dir = pite_export::python_bundle::toolchain_dir();
    let mut artifacts = Vec::new();
    let mut table = Table::new(&["KIND", "PLATFORM", "STATUS", "PATH"]);
    let mut cached = 0usize;

    for (index, &platform) in pite_export::SUPPORTED_PLATFORMS.iter().enumerate() {
        let Some(spec) = pite_export::python_bundle::pinned(platform) else {
            continue;
        };
        let path = pite_export::python_bundle::archive_path(&spec);
        let (status, detail, cell) = if path.is_file() {
            cached += 1;
            (
                "cached",
                path.display().to_string(),
                Cell::success("Cached"),
            )
        } else {
            ("missing", spec.file.to_string(), Cell::warning("Missing"))
        };
        table = table.row(vec![
            Cell::plain(if index == 0 { "Python" } else { "" }),
            Cell::plain(pite_export::platform_label(platform)),
            cell,
            Cell::strong(detail.clone()),
        ]);
        artifacts.push(Artifact {
            kind: "python",
            platform: platform.to_string(),
            status,
            detail,
        });
    }

    for (index, &platform) in pite_export::SUPPORTED_PLATFORMS.iter().enumerate() {
        let (status, detail, cell) = match pite_export::player::find_cached_player(platform) {
            Some(path) => {
                cached += 1;
                (
                    "cached",
                    path.display().to_string(),
                    Cell::success("Cached"),
                )
            }
            None => (
                "missing",
                pite_export::player::player_bin_name(platform).to_string(),
                Cell::warning("Missing"),
            ),
        };
        table = table.row(vec![
            Cell::plain(if index == 0 { "Player" } else { "" }),
            Cell::plain(pite_export::platform_label(platform)),
            cell,
            Cell::strong(detail.clone()),
        ]);
        artifacts.push(Artifact {
            kind: "player",
            platform: platform.to_string(),
            status,
            detail,
        });
    }

    let missing = artifacts.len() - cached;
    ui.info(
        Spans::new()
            .plain("Toolchain directory: ")
            .strong(dir.display().to_string()),
    );
    ui.section("Toolchain cache");
    ui.table(&table);
    if !ui.quiet() {
        ui.blank();
        let right = if missing == 0 {
            format!("{cached} cached")
        } else {
            format!("{cached} cached · {missing} missing")
        };
        ui.bar(
            "Toolchain cache",
            &right,
            if missing == 0 {
                BarTone::Success
            } else {
                BarTone::Faded
            },
        );
    }

    Outcome::completed(StatusData {
        schema_version: SCHEMA_VERSION,
        dir: dir.display().to_string(),
        artifacts,
    })
}

fn download(
    ui: &mut Ui,
    platform: Option<&str>,
    player_only: bool,
    python_only: bool,
) -> Result<Outcome> {
    let platforms: Vec<&str> = match platform {
        Some(platform) => vec![platform],
        None => pite_export::SUPPORTED_PLATFORMS.to_vec(),
    };
    let download_python = !player_only;
    let download_player = !python_only;
    let mut artifacts = Vec::new();

    if download_python {
        ui.section("Python runtimes");
        for &platform in &platforms {
            let Some(spec) = pite_export::python_bundle::pinned(platform) else {
                continue;
            };
            let path = pite_export::python_bundle::ensure_archive(&spec, ui)?;
            ui.verbose_note(Spans::new().plain("  ").strong(path.display().to_string()));
            artifacts.push(Artifact {
                kind: "python",
                platform: platform.to_string(),
                status: "ready",
                detail: path.display().to_string(),
            });
        }
    }

    if download_player {
        ui.section("Player binaries");
        for &platform in &platforms {
            let path = pite_export::player::ensure_player_binary(platform, ui)?;
            ui.verbose_note(Spans::new().plain("  ").strong(path.display().to_string()));
            artifacts.push(Artifact {
                kind: "player",
                platform: platform.to_string(),
                status: "ready",
                detail: path.display().to_string(),
            });
        }
    }

    let mut table = Table::new(&["KIND", "PLATFORM", "STATUS", "PATH"]);
    let mut last_kind = "";
    for artifact in &artifacts {
        let kind = if artifact.kind == last_kind {
            ""
        } else {
            artifact.kind
        };
        last_kind = artifact.kind;
        table = table.row(vec![
            Cell::plain(capitalize(kind)),
            Cell::plain(pite_export::platform_label(&artifact.platform)),
            Cell::success("Cached"),
            Cell::strong(artifact.detail.as_str()),
        ]);
    }
    ui.section("Toolchain cache");
    ui.table(&table);
    if !ui.quiet() {
        ui.blank();
        ui.bar(
            "Toolchain download",
            &format!(
                "{} artifact{} ready",
                artifacts.len(),
                if artifacts.len() == 1 { "" } else { "s" }
            ),
            BarTone::Success,
        );
    }

    Outcome::completed(DownloadData {
        schema_version: SCHEMA_VERSION,
        artifacts,
    })
}

fn verify(ui: &mut Ui) -> Result<Outcome> {
    let mut artifacts = Vec::new();
    let mut verified = 0usize;
    let mut failed = 0usize;
    let mut table = Table::new(&["KIND", "PLATFORM", "STATUS", "DETAIL"]);

    for (index, &platform) in pite_export::SUPPORTED_PLATFORMS.iter().enumerate() {
        let Some(spec) = pite_export::python_bundle::pinned(platform) else {
            continue;
        };
        let path = pite_export::python_bundle::archive_path(&spec);
        let (status, detail, cell) = if path.is_file() {
            match pite_export::python_bundle::verify(&spec, &path, ui) {
                Ok(()) => {
                    verified += 1;
                    ("ok", path.display().to_string(), Cell::success("Verified"))
                }
                Err(error) => {
                    failed += 1;
                    (
                        "mismatch",
                        capitalize(&format!("{error:#}")),
                        Cell::error("Mismatch"),
                    )
                }
            }
        } else {
            (
                "not cached",
                spec.file.to_string(),
                Cell::warning("Missing"),
            )
        };
        let detail_cell = if status == "mismatch" || status == "not cached" {
            Cell::plain(detail.clone())
        } else {
            Cell::strong(detail.clone())
        };
        table = table.row(vec![
            Cell::plain(if index == 0 { "Python" } else { "" }),
            Cell::plain(pite_export::platform_label(platform)),
            cell,
            detail_cell,
        ]);
        artifacts.push(Artifact {
            kind: "python",
            platform: platform.to_string(),
            status,
            detail,
        });
    }

    for (index, &platform) in pite_export::SUPPORTED_PLATFORMS.iter().enumerate() {
        let (status, detail, cell) = match pite_export::player::find_cached_player(platform) {
            Some(path) => {
                let valid = std::fs::metadata(&path)
                    .map(|meta| meta.len() > 0)
                    .unwrap_or(false);
                if valid {
                    verified += 1;
                    ("ok", path.display().to_string(), Cell::success("Verified"))
                } else {
                    failed += 1;
                    (
                        "invalid",
                        path.display().to_string(),
                        Cell::error("Mismatch"),
                    )
                }
            }
            None => (
                "not cached",
                pite_export::player::player_bin_name(platform).to_string(),
                Cell::warning("Missing"),
            ),
        };
        let detail_cell = if status == "invalid" || status == "not cached" {
            Cell::plain(detail.clone())
        } else {
            Cell::strong(detail.clone())
        };
        table = table.row(vec![
            Cell::plain(if index == 0 { "Player" } else { "" }),
            Cell::plain(pite_export::platform_label(platform)),
            cell,
            detail_cell,
        ]);
        artifacts.push(Artifact {
            kind: "player",
            platform: platform.to_string(),
            status,
            detail,
        });
    }

    ui.section("Verification");
    ui.table(&table);
    let ok = failed == 0;
    if !ui.quiet() || !ok {
        ui.blank();
        if ok && verified == 0 {
            ui.info(
                Spans::new()
                    .plain("No toolchain artifacts are cached yet. Run ")
                    .strong("pite toolchain download")
                    .plain(" to fetch them."),
            );
            ui.bar("Toolchain verify", "0 artifacts cached", BarTone::Faded);
        } else if ok {
            ui.bar(
                "Toolchains verified",
                &format!(
                    "{verified} artifact{}",
                    if verified == 1 { "" } else { "s" }
                ),
                BarTone::Success,
            );
        } else {
            ui.bar(
                "Toolchain verify failed",
                &format!("{failed} failed · {verified} verified",),
                BarTone::Error,
            );
        }
    }

    let data = VerifyData {
        schema_version: SCHEMA_VERSION,
        ok,
        verified,
        failed,
        artifacts,
    };
    if ok {
        Outcome::completed(data)
    } else {
        Outcome::failed(data)
    }
}

fn clean(ui: &mut Ui, python: &[String], player: &[String], all: bool) -> Result<Outcome> {
    if !all && python.is_empty() && player.is_empty() {
        anyhow::bail!(
            "nothing to remove; pass --python <platforms>, --player <platforms>, or --all"
        );
    }

    let dir = pite_export::python_bundle::toolchain_dir();
    let mut removed: Vec<Artifact> = Vec::new();

    if all {
        for &platform in pite_export::SUPPORTED_PLATFORMS {
            if let Some(spec) = pite_export::python_bundle::pinned(platform) {
                let path = pite_export::python_bundle::archive_path(&spec);
                if path.is_file() {
                    removed.push(removed_artifact("python", platform, &path));
                }
            }
            if let Some(path) = pite_export::player::find_cached_player(platform) {
                removed.push(removed_artifact("player", platform, &path));
            }
        }
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
    } else {
        for platform in python {
            let Some(spec) = pite_export::python_bundle::pinned(platform) else {
                continue;
            };
            let path = pite_export::python_bundle::archive_path(&spec);
            if path.is_file() {
                std::fs::remove_file(&path)?;
                removed.push(removed_artifact("python", platform, &path));
            }
        }
        for platform in player {
            if let Some(path) = pite_export::player::find_cached_player(platform) {
                std::fs::remove_file(&path)?;
                removed.push(removed_artifact("player", platform, &path));
            }
        }
    }

    if removed.is_empty() {
        ui.info(
            Spans::new()
                .plain("Nothing to remove in ")
                .strong(dir.display().to_string())
                .plain("."),
        );
    } else {
        let mut table = Table::new(&["KIND", "PLATFORM", "ARTIFACT"]);
        for artifact in &removed {
            table = table.row(vec![
                Cell::plain(capitalize(artifact.kind)),
                Cell::plain(pite_export::platform_label(&artifact.platform)),
                Cell::strong(artifact.detail.as_str()),
            ]);
        }
        ui.section("Removed artifacts");
        ui.table(&table);
    }
    if !ui.quiet() {
        ui.blank();
        if removed.is_empty() {
            ui.bar("Toolchain clean", "nothing to remove", BarTone::Faded);
        } else {
            ui.bar(
                "Toolchain clean",
                &format!(
                    "{} artifact{} removed",
                    removed.len(),
                    if removed.len() == 1 { "" } else { "s" }
                ),
                BarTone::Success,
            );
        }
    }

    Outcome::completed(CleanData {
        schema_version: SCHEMA_VERSION,
        dir: dir.display().to_string(),
        removed,
    })
}

fn removed_artifact(kind: &'static str, platform: &str, path: &std::path::Path) -> Artifact {
    Artifact {
        kind,
        platform: platform.to_string(),
        status: "removed",
        detail: path.display().to_string(),
    }
}
