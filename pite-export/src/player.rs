// SPDX-License-Identifier: MIT OR Apache-2.0
//! Resolving, caching, and downloading `pite-player` binaries for export.
//!
//! When exporting a project, the engine looks for a player binary matching the
//! target platform next to the engine binary. If one is not present (for example,
//! when running from a bare `cargo install` or when cross-exporting without having
//! pre-downloaded other platform bundles), the fallback downloads the platform's
//! player release archive from GitHub releases and unpacks it into the toolchain
//! cache (`~/.cache/pite/toolchains/players/` or `PITE_PLAYER_DIR`).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Player binary file name on its own platform, always carrying the platform name.
pub fn player_bin_name(platform: &str) -> &'static str {
    match platform {
        "windows" => "pite-player-windows.exe",
        "macos" => "pite-player-macos",
        "linux" => "pite-player-linux",
        _ => "pite-player",
    }
}

/// Candidate player binary names for `platform`.
pub fn candidate_player_names(platform: &str, _host_os: &str) -> Vec<String> {
    match platform {
        "windows" => vec![
            "pite-player-windows.exe".to_string(),
            "pite-player.exe".to_string(),
        ],
        "macos" => vec!["pite-player-macos".to_string(), "pite-player".to_string()],
        "linux" => vec!["pite-player-linux".to_string(), "pite-player".to_string()],
        _ => vec![format!("pite-player-{platform}"), "pite-player".to_string()],
    }
}

/// Where player binaries live in cache. Never inside the project or repo.
pub fn player_cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PITE_PLAYER_DIR") {
        return PathBuf::from(dir);
    }
    crate::python_bundle::toolchain_dir().join("players")
}

/// Subdirectory in cache for a specific version and platform.
pub fn player_dir(platform: &str) -> PathBuf {
    let version =
        std::env::var("PITE_VERSION").unwrap_or_else(|_| format!("v{}", env!("CARGO_PKG_VERSION")));
    player_cache_dir().join(version).join(platform)
}

/// Ensure that `path` has executable permissions on Unix systems.
pub fn ensure_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mode = meta.permissions().mode();
            if mode & 0o111 != 0o111 {
                let _ =
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o755));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Look for an already cached player binary for `platform`.
pub fn find_cached_player(platform: &str) -> Option<PathBuf> {
    let version =
        std::env::var("PITE_VERSION").unwrap_or_else(|_| format!("v{}", env!("CARGO_PKG_VERSION")));
    let bin_name = player_bin_name(platform);

    // 1. Check in platform-specific dir: <cache>/<version>/<platform>/<bin_name>
    let specific = player_cache_dir()
        .join(&version)
        .join(platform)
        .join(bin_name);
    if specific.is_file() {
        ensure_executable(&specific);
        return Some(specific);
    }

    // 2. Check if allplatforms was extracted into <cache>/<version>/
    let version_dir = player_cache_dir().join(&version);
    for name in candidate_player_names(platform, std::env::consts::OS) {
        let p = version_dir.join(&name);
        if p.is_file() {
            ensure_executable(&p);
            return Some(p);
        }
    }

    // 3. Check directly in player_cache_dir()
    for name in candidate_player_names(platform, std::env::consts::OS) {
        let p = player_cache_dir().join(&name);
        if p.is_file() {
            ensure_executable(&p);
            return Some(p);
        }
    }

    None
}

/// Release archive filename for `platform` at `tag`.
pub fn player_archive_name(platform: &str, tag: &str) -> String {
    if platform == "windows" {
        format!("pite-player-{tag}-windows.zip")
    } else {
        format!("pite-player-{tag}-{platform}.tar.gz")
    }
}

/// Download URL for the player archive for `platform` at `tag`.
pub fn player_download_url(platform: &str, tag: &str) -> String {
    if let Ok(url) = std::env::var("PITE_PLAYER_URL") {
        return url;
    }
    let base = std::env::var("PITE_RELEASE_BASE")
        .unwrap_or_else(|_| "https://github.com/piteengine/pite/releases/download".to_string());
    let archive = player_archive_name(platform, tag);
    format!("{base}/{tag}/{archive}")
}

/// Download the player archive and unpack it into the cache directory.
pub fn download_and_extract_player(platform: &str) -> Result<PathBuf> {
    let tag =
        std::env::var("PITE_VERSION").unwrap_or_else(|_| format!("v{}", env!("CARGO_PKG_VERSION")));
    let target_dir = player_dir(platform);
    std::fs::create_dir_all(&target_dir)
        .with_context(|| format!("cannot create {}", target_dir.display()))?;

    let archive_name = player_archive_name(platform, &tag);
    let archive_path = target_dir.join(&archive_name);
    let url = player_download_url(platform, &tag);

    eprintln!("info: downloading player binary for {platform} ({archive_name})...");
    tracing::info!(platform = %platform, url = %url, "downloading pite-player binary");
    download_file(&url, &archive_path, platform)?;
    eprintln!("info: extracting player binary for {platform}...");

    let extract_res = if archive_name.ends_with(".zip") {
        unpack_zip(&archive_path, &target_dir)
    } else {
        unpack_tar(&archive_path, &target_dir)
    };

    let _ = std::fs::remove_file(&archive_path);
    extract_res?;

    let bin_name = player_bin_name(platform);
    let bin_path = target_dir.join(bin_name);
    if bin_path.is_file() {
        ensure_executable(&bin_path);
        return Ok(bin_path);
    }

    for name in candidate_player_names(platform, std::env::consts::OS) {
        let p = target_dir.join(&name);
        if p.is_file() {
            ensure_executable(&p);
            return Ok(p);
        }
    }

    bail!(
        "unpacked player archive for {platform} into {}, but could not find {bin_name}",
        target_dir.display()
    )
}

/// Ensure a player binary is available for `platform`, checking cache first and
/// downloading if absent.
pub fn ensure_player_binary(platform: &str) -> Result<PathBuf> {
    if let Some(cached) = find_cached_player(platform) {
        return Ok(cached);
    }
    download_and_extract_player(platform)
}

fn download_file(url: &str, dest: &Path, platform: &str) -> Result<()> {
    use std::io::IsTerminal;
    if let Some(path_str) = url.strip_prefix("file://") {
        let src = Path::new(path_str);
        std::fs::copy(src, dest)
            .with_context(|| format!("cannot copy {} to {}", src.display(), dest.display()))?;
        return Ok(());
    }

    let tool = downloader()?;
    let _ = std::fs::remove_file(dest);
    let mut cmd = std::process::Command::new(tool);
    cmd.args(["--fail", "--location", "--retry", "3", "--retry-delay", "3"]);
    if std::io::stderr().is_terminal() {
        cmd.arg("--progress-bar");
    } else {
        cmd.args(["--silent", "--show-error"]);
    }
    let result = cmd.arg(url).arg("--output").arg(dest).status();
    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            let _ = std::fs::remove_file(dest);
            bail!(
                "download of {url} failed ({status}). Export needs a pite-player binary for \
                 {platform}; retry with network access, or pass --binary with a player binary.",
            );
        }
        Err(e) => {
            let _ = std::fs::remove_file(dest);
            bail!(
                "cannot run {tool} to download {url}: {e}. Export needs a pite-player binary for \
                 {platform}; or pass --binary with a player binary."
            );
        }
    }
}

fn downloader() -> Result<&'static str> {
    for tool in ["curl", "curl.exe"] {
        if std::process::Command::new(tool)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return Ok(tool);
        }
    }
    bail!("curl is required to download pite-player; install curl or pass --binary")
}

fn unpack_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)
        .with_context(|| format!("cannot open {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("cannot read zip {}", archive.display()))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let path = match entry.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };
        if entry.name().ends_with('/') {
            std::fs::create_dir_all(&path)?;
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&path)?;
            std::io::copy(&mut entry, &mut outfile)?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(())
}

fn unpack_tar(archive: &Path, dest: &Path) -> Result<()> {
    let tar = if cfg!(windows) { "tar.exe" } else { "tar" };
    let out = std::process::Command::new(tar)
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .output();
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => bail!(
            "cannot unpack {} with tar ({}); install tar or pass --binary",
            archive.display(),
            o.status
        ),
        Err(e) => bail!(
            "cannot run {tar} to unpack {}: {e}; pass --binary with a player binary",
            archive.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_bin_name_resolves_platform() {
        assert_eq!(player_bin_name("windows"), "pite-player-windows.exe");
        assert_eq!(player_bin_name("linux"), "pite-player-linux");
        assert_eq!(player_bin_name("macos"), "pite-player-macos");
    }

    #[test]
    fn candidate_player_names_includes_expected() {
        let cand_win = candidate_player_names("windows", "linux");
        assert!(cand_win.contains(&"pite-player.exe".to_string()));
        assert!(cand_win.contains(&"pite-player-windows.exe".to_string()));

        let cand_mac = candidate_player_names("macos", "linux");
        assert!(cand_mac.contains(&"pite-player-macos".to_string()));

        let cand_lin = candidate_player_names("linux", "windows");
        assert!(cand_lin.contains(&"pite-player-linux".to_string()));
    }

    #[test]
    fn archive_name_formats() {
        assert_eq!(
            player_archive_name("windows", "v0.3.0"),
            "pite-player-v0.3.0-windows.zip"
        );
        assert_eq!(
            player_archive_name("linux", "v0.3.0"),
            "pite-player-v0.3.0-linux.tar.gz"
        );
        assert_eq!(
            player_archive_name("macos", "v0.3.0"),
            "pite-player-v0.3.0-macos.tar.gz"
        );
    }
}
