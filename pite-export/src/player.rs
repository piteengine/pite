// SPDX-License-Identifier: MIT OR Apache-2.0
//! Export templates: lean `pite-player` binaries built natively per platform
//! by CI and attached to each release. Export fetches the pinned template for
//! the target platform (checksum-verified, cached under the toolchain dir),
//! so any machine can export for any platform with no compiler installed.
//! `--binary` stays as the offline escape hatch.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Release tag carrying the templates, bumped each milestone. Pinned like
/// the Python builds: an exact match is the whole version-skew policy — a
/// stale template would boot scenes the editor no longer writes.
pub const TEMPLATE_TAG: &str = "v0.1-m5";

const RELEASE_BASE: &str = "https://github.com/piteengine/pite/releases/download";

/// One platform's published player binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerSpec {
    pub platform: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
}

// Checksums are backfilled from the first tagged release publishing these
// artifacts; until then the placeholder makes every use fail loudly.
const LINUX: PlayerSpec = PlayerSpec {
    platform: "linux",
    file: "pite-player-linux",
    sha256: "REPLACE_WITH_RELEASE_CHECKSUM",
};

const WINDOWS: PlayerSpec = PlayerSpec {
    platform: "windows",
    file: "pite-player-windows.exe",
    sha256: "REPLACE_WITH_RELEASE_CHECKSUM",
};

/// The published template for `platform`, or `None` when we ship none.
pub fn pinned(platform: &str) -> Option<PlayerSpec> {
    match platform {
        "linux" => Some(LINUX),
        "windows" => Some(WINDOWS),
        _ => None,
    }
}

/// Player binary file name on its own platform.
pub fn bin_name(platform: &str) -> &'static str {
    if platform == "windows" {
        "pite-player.exe"
    } else {
        "pite-player"
    }
}

fn cached_path(spec: &PlayerSpec) -> PathBuf {
    crate::python_bundle::toolchain_dir()
        .join("player")
        .join(spec.platform)
        .join(TEMPLATE_TAG)
        .join(spec.file)
}

/// Fetch the pinned template into the toolchain cache and verify it.
/// A cached file is re-verified every time: a bad cache is never trusted.
pub fn ensure_cached(spec: &PlayerSpec) -> Result<PathBuf> {
    if spec.sha256 == "REPLACE_WITH_RELEASE_CHECKSUM" {
        bail!(
            "no published player template for {} {} yet (first tagged release \
             publishes it); pass --binary with a locally built pite-player instead",
            spec.platform,
            TEMPLATE_TAG
        );
    }
    let path = cached_path(spec);
    if path.is_file() {
        verify(spec, &path)?;
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let url = format!("{RELEASE_BASE}/{TEMPLATE_TAG}/{}", spec.file);
    super::python_bundle::download(&url, &path)?;
    if let Err(e) = verify(spec, &path) {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok(path)
}

/// Checksum gate. Never warns, never repairs: a mismatch is fatal.
fn verify(spec: &PlayerSpec, path: &Path) -> Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let actual = super::python_bundle::sha256_hex(&bytes);
    if actual.eq_ignore_ascii_case(spec.sha256) {
        return Ok(());
    }
    bail!(
        "checksum mismatch for {}: expected {}, got {}. Delete the cached file and retry.",
        path.display(),
        spec.sha256,
        actual
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpublished_template_fails_loudly_with_the_escape_hatch() {
        let _guard = crate::SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pite-tpl-{}", std::process::id()));
        std::env::set_var("PITE_TOOLCHAIN_DIR", &dir);
        let err = ensure_cached(&LINUX).unwrap_err().to_string();
        assert!(err.contains("--binary"), "got: {err}");
        std::env::remove_var("PITE_TOOLCHAIN_DIR");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cached_template_hit_skips_the_network() {
        let _guard = crate::SERIAL.lock().unwrap();
        let mut spec = LINUX;
        spec.sha256 = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        let dir = std::env::temp_dir().join(format!("pite-tplhit-{}", std::process::id()));
        std::env::set_var("PITE_TOOLCHAIN_DIR", &dir);
        let dest = cached_path(&spec);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, b"test").unwrap();
        assert_eq!(ensure_cached(&spec).unwrap(), dest);
        std::env::remove_var("PITE_TOOLCHAIN_DIR");
        std::fs::remove_dir_all(&dir).ok();
    }
}
