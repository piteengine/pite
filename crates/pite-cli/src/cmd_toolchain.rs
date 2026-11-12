// SPDX-License-Identifier: MIT OR Apache-2.0
use anyhow::Result;
use clap::Subcommand;

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
    /// Remove cached toolchain archives and player binaries.
    Clean,
}

pub fn run(action: ToolchainAction) -> Result<()> {
    match action {
        ToolchainAction::Status => status(),
        ToolchainAction::Download {
            platform,
            player,
            python,
        } => download(platform.as_deref(), player, python),
        ToolchainAction::Verify => verify(),
        ToolchainAction::Clean => clean(),
    }
}

fn status() -> Result<()> {
    println!(
        "Toolchain directory: {}",
        pite_export::python_bundle::toolchain_dir().display()
    );
    println!("\nPython Runtimes (CPython 3.12):");
    for &plat in pite_export::SUPPORTED_PLATFORMS {
        if let Some(spec) = pite_export::python_bundle::pinned(plat) {
            let path = pite_export::python_bundle::archive_path(&spec);
            if path.is_file() {
                println!("  {plat:<10} cached   ({})", path.display());
            } else {
                println!("  {plat:<10} missing  ({})", spec.file);
            }
        }
    }
    println!("\nPlayer Binaries (pite-player):");
    for &plat in pite_export::SUPPORTED_PLATFORMS {
        if let Some(path) = pite_export::player::find_cached_player(plat) {
            println!("  {plat:<10} cached   ({})", path.display());
        } else {
            let bin_name = pite_export::player::player_bin_name(plat);
            println!("  {plat:<10} missing  ({bin_name})");
        }
    }
    Ok(())
}

fn download(platform: Option<&str>, player_only: bool, python_only: bool) -> Result<()> {
    let platforms: Vec<&str> = match platform {
        Some(p) => vec![p],
        None => pite_export::SUPPORTED_PLATFORMS.to_vec(),
    };
    let dl_python = !player_only;
    let dl_player = !python_only;

    if dl_python {
        println!("Downloading Python runtime(s)...");
        for &plat in &platforms {
            if let Some(spec) = pite_export::python_bundle::pinned(plat) {
                let path = pite_export::python_bundle::ensure_archive(&spec)?;
                println!("python ({plat}): ready at {}", path.display());
            }
        }
    }

    if dl_player {
        println!("Downloading player binary(ies)...");
        for &plat in &platforms {
            let path = pite_export::player::ensure_player_binary(plat)?;
            println!("player ({plat}): ready at {}", path.display());
        }
    }

    println!("Toolchain download complete.");
    Ok(())
}

fn verify() -> Result<()> {
    let mut verified = 0;
    let mut bad = 0;
    println!("Verifying Python runtimes...");
    for &plat in pite_export::SUPPORTED_PLATFORMS {
        if let Some(spec) = pite_export::python_bundle::pinned(plat) {
            let path = pite_export::python_bundle::archive_path(&spec);
            if path.is_file() {
                match pite_export::python_bundle::verify(&spec, &path) {
                    Ok(()) => {
                        println!("  {plat:<10} ok ({})", path.display());
                        verified += 1;
                    }
                    Err(e) => {
                        eprintln!("  {plat:<10} MISMATCH ({e:#})");
                        bad += 1;
                    }
                }
            } else {
                println!("  {plat:<10} not cached (skip)");
            }
        }
    }

    println!("\nVerifying player binaries...");
    for &plat in pite_export::SUPPORTED_PLATFORMS {
        if let Some(path) = pite_export::player::find_cached_player(plat) {
            if path.is_file()
                && std::fs::metadata(&path)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false)
            {
                println!("  {plat:<10} ok ({})", path.display());
                verified += 1;
            } else {
                eprintln!("  {plat:<10} INVALID ({})", path.display());
                bad += 1;
            }
        } else {
            println!("  {plat:<10} not cached (skip)");
        }
    }

    if bad > 0 {
        anyhow::bail!("{bad} toolchain artifact(s) failed verification");
    }
    if verified == 0 {
        println!("\nNo toolchain artifacts are currently cached. Use 'pite toolchain download' to fetch them.");
    } else {
        println!("\nAll {verified} cached toolchain artifact(s) verified successfully.");
    }
    Ok(())
}

fn clean() -> Result<()> {
    let dir = pite_export::python_bundle::toolchain_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
        println!("Removed toolchain cache: {}", dir.display());
    } else {
        println!("Toolchain cache is already empty ({})", dir.display());
    }
    Ok(())
}
