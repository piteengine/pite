// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bundling a pinned CPython 3.12 so an exported game runs on a machine with
//! no Python installed.
//!
//! Shape follows the "embedded" distribution: the shared library plus a stdlib
//! **zip** and nothing else. `pite` is a native module registered from Rust, so
//! no `site-packages` ship. The archive is cached under the toolchain dir
//! (`PITE_TOOLCHAIN_DIR`, default `~/.cache/pite/toolchains`) and never vendored
//! in the repo; a checksum mismatch or an offline machine is a loud error, and
//! `--no-bundle-python` keeps the older system-Python gate.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::sha256::{crc32, sha256_file};

/// Pinned upstream build: astral-sh/python-build-standalone, `install_only`.
/// Checksums come from the release's `SHA256SUMS`.
pub const PINNED_BUILD: &str = "20250317";
pub const PINNED_PYTHON: &str = "3.12.9";

/// Every member path in these archives sits under this directory.
pub const ARCHIVE_ROOT: &str = "python";

const RELEASE_BASE: &str = "https://github.com/astral-sh/python-build-standalone/releases/download";

/// One platform's pinned artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleSpec {
    pub platform: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
    /// Shared library inside the archive, relative to [`ARCHIVE_ROOT`].
    pub lib: &'static str,
    /// Stdlib directory inside the archive, relative to [`ARCHIVE_ROOT`].
    pub stdlib: &'static str,
    /// Stdlib zip name used by the interpreter.
    pub zip_name: &'static str,
    /// Loader config file the interpreter reads from beside [`Self::lib`].
    /// `None` where `PYTHONPATH` alone is enough to find the stdlib zip.
    pub pth_name: Option<&'static str>,
}

/// The two `install_only` archives do not agree on layout: Linux nests the
/// stdlib under `lib/python3.12/`, Windows puts it straight in `Lib/`. Windows
/// also ships `python3.dll` next to `python312.dll`, but the first is a 56 KB
/// forwarder onto the second, so only the versioned library can be staged.
const LINUX: BundleSpec = BundleSpec {
    platform: "linux",
    file: "cpython-3.12.9+20250317-x86_64-unknown-linux-gnu-install_only.tar.gz",
    sha256: "ef382fb88cbb41a3b0801690bd716b8a1aec07a6c6471010bcc6bd14cd575226",
    lib: "lib/libpython3.12.so.1.0",
    stdlib: "lib/python3.12",
    zip_name: "python312.zip",
    pth_name: None,
};

const WINDOWS: BundleSpec = BundleSpec {
    platform: "windows",
    file: "cpython-3.12.9+20250317-x86_64-pc-windows-msvc-install_only.tar.gz",
    sha256: "d15361fd202dd74ae9c3eece1abdab7655f1eba90bf6255cad1d7c53d463ed4d",
    lib: "python312.dll",
    stdlib: "Lib",
    zip_name: "python312.zip",
    pth_name: Some("python312._pth"),
};

/// The pinned artifact for `platform`, or `None` when we do not ship one.
pub fn pinned(platform: &str) -> Option<BundleSpec> {
    match platform {
        "linux" => Some(LINUX),
        "windows" => Some(WINDOWS),
        _ => None,
    }
}

/// Every platform whose bundle artifact is pinned and checksummed.
pub fn pinned_platforms() -> Vec<&'static str> {
    vec!["linux", "windows"]
}

/// Where downloaded toolchains live. Never inside the project or repo.
pub fn toolchain_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PITE_TOOLCHAIN_DIR") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".pitetoolchain"));
    base.join("pite").join("toolchains")
}

pub fn archive_path(spec: &BundleSpec) -> PathBuf {
    toolchain_dir()
        .join(spec.platform)
        .join(PINNED_PYTHON)
        .join(&spec.file)
}

/// Fetch the pinned archive into the toolchain cache and verify it.
/// A cached file is re-verified every time: a bad cache is never trusted.
pub fn ensure_archive(spec: &BundleSpec) -> Result<PathBuf> {
    let path = archive_path(spec);
    if path.is_file() {
        verify(spec, &path)?;
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let url = format!("{RELEASE_BASE}/{PINNED_BUILD}/{}", spec.file);
    download(&url, &path)?;
    if let Err(e) = verify(spec, &path) {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok(path)
}

/// Checksum gate. Never warns, never repairs: a mismatch is fatal.
pub fn verify(spec: &BundleSpec, path: &Path) -> Result<()> {
    let actual = sha256_file(path)?;
    if actual.eq_ignore_ascii_case(spec.sha256) {
        return Ok(());
    }
    if spec.sha256 == "REPLACE_WINDOWS_SHA256" {
        bail!(
            "no pinned checksum recorded for {} (expected placeholder); refusing to use {}",
            spec.file,
            path.display()
        );
    }
    bail!(
        "checksum mismatch for {}: expected {}, got {}. Delete the cached file and retry.",
        path.display(),
        spec.sha256,
        actual
    )
}

fn download(url: &str, dest: &Path) -> Result<()> {
    let tool = downloader()?;
    let result = std::process::Command::new(&tool)
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--retry",
            "3",
            "--retry-delay",
            "3",
        ])
        .arg(url)
        .arg("--output")
        .arg(dest)
        .status();
    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => bail!(
            "download of {url} failed ({}). Export needs network access to fetch the \
             pinned Python; retry with network, or pass --no-bundle-python to keep the \
             system-Python requirement.",
            status
        ),
        Err(e) => bail!(
            "cannot run {tool} to download {url}: {e}. Export needs network access to \
             fetch the pinned Python; or pass --no-bundle-python to keep the \
             system-Python requirement."
        ),
    }
}

fn downloader() -> Result<&'static str> {
    for tool in ["curl", "curl.exe"] {
        if Command::new(tool)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return Ok(tool);
        }
    }
    bail!(
        "no downloader found (need curl). Fetch {} manually into {} , or pass \
         --no-bundle-python to keep the system-Python requirement.",
        LINUX.file,
        toolchain_dir().display()
    )
}

use std::process::Command;

/// What an exported directory needs to find the bundled interpreter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedBundle {
    pub root: PathBuf,
    pub lib: PathBuf,
    pub stdlib_zip: PathBuf,
}

/// Extract the shared library, pack the stdlib into a zip, and lay both out
/// under `dest` (`<out>/python`). `site-packages` is deliberately dropped.
pub fn stage(spec: &BundleSpec, archive: &Path, dest: &Path) -> Result<StagedBundle> {
    let work = dest.join(".staging");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).with_context(|| format!("cannot create {}", work.display()))?;
    unpack(spec, archive, &work)?;

    let lib_src = work.join(ARCHIVE_ROOT).join(&spec.lib);
    if !lib_src.is_file() {
        bail!(
            "archive {} does not contain {ARCHIVE_ROOT}/{}",
            archive.display(),
            spec.lib
        );
    }
    let lib_dir = dest.join("lib");
    std::fs::create_dir_all(&lib_dir)?;
    let lib_name = lib_src
        .file_name()
        .expect("lib file has a name")
        .to_string_lossy()
        .into_owned();
    let lib = lib_dir.join(&lib_name);
    std::fs::copy(&lib_src, &lib).with_context(|| format!("cannot copy {}", lib_src.display()))?;

    let stdlib_dir = work.join(ARCHIVE_ROOT).join(&spec.stdlib);
    if !stdlib_dir.is_dir() {
        bail!(
            "archive {} does not contain {ARCHIVE_ROOT}/{}",
            archive.display(),
            spec.stdlib
        );
    }
    let stdlib_zip = dest.join(spec.zip_name);
    zip_dir(&stdlib_dir, &stdlib_zip)?;
    let _ = std::fs::remove_dir_all(&work);
    Ok(StagedBundle {
        root: dest.to_path_buf(),
        lib,
        stdlib_zip,
    })
}

/// Directories never shipped. `pite` registers its module from Rust, so an
/// exported game needs the stdlib and nothing else — no installer, no REPL
/// tooling, no virtualenv support. `venv` is the bulky one (8 MB of the
/// Windows stdlib) and is dead weight for an embedded interpreter.
const EXCLUDED: &[&str] = &[
    "site-packages",
    "test",
    "idlelib",
    "tkinter",
    "ensurepip",
    "__pycache__",
    "venv",
    "lib2to3",
    "pydoc_data",
    "turtledemo",
    "msilib",
];

/// Versionless ABI forwarders. They dispatch *to* the versioned interpreter
/// rather than being one, so a copy of these cannot satisfy the loader.
#[cfg(test)]
const FORWARDERS: &[&str] = &["python3.dll", "libpython3.so", "libpython3.dylib"];

fn unpack(_spec: &BundleSpec, archive: &Path, work: &Path) -> Result<()> {
    let tar = if cfg!(windows) { "tar.exe" } else { "tar" };
    let out = std::process::Command::new(tar)
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(work)
        .output();
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => bail!(
            "cannot unpack {} with tar ({}); install tar or pass --no-bundle-python",
            archive.display(),
            o.status
        ),
        Err(e) => bail!(
            "cannot run {tar} to unpack {}: {e}; or pass --no-bundle-python",
            archive.display()
        ),
    }
}

fn zip_dir(dir: &Path, zip_path: &Path) -> Result<()> {
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    collect(dir, dir, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut count = 0u16;
    for (name, path) in &entries {
        let data =
            std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        let crc = crc32(&data);
        let offset = out.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // stored
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
        count += 1;
    }
    let dir_offset = out.len() as u32;
    let dir_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&dir_size.to_le_bytes());
    out.extend_from_slice(&dir_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    std::fs::write(zip_path, out).with_context(|| format!("cannot write {}", zip_path.display()))
}

fn collect(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if EXCLUDED.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect(base, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .expect("entry under base")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
    Ok(())
}

/// Env the launcher must set for a staged bundle. Proved on Linux: the
/// process runs with no Python on `PATH`.
pub fn launcher_env(bundle: &StagedBundle) -> Vec<(String, String)> {
    vec![
        (
            "LD_LIBRARY_PATH".to_string(),
            bundle
                .lib
                .parent()
                .unwrap_or(&bundle.root)
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "PYTHONPATH".to_string(),
            bundle.stdlib_zip.to_string_lossy().into_owned(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("pite-bundle-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pinned_specs_exist_for_supported_platforms() {
        for platform in pinned_platforms() {
            let spec = pinned(platform).expect("pinned");
            assert_eq!(spec.platform, platform);
            assert!(
                !spec.sha256.starts_with("REPLACE"),
                "{platform} must ship a real checksum"
            );
            assert!(spec.file.starts_with("cpython-3.12"));
        }
        assert!(pinned("macos").is_none());
    }

    #[test]
    fn checksum_mismatch_is_loud_and_leaves_nothing_usable() {
        let dir = tmpdir("sum");
        let archive = dir.join("fake.tar.gz");
        std::fs::write(&archive, b"not the pinned archive").unwrap();
        let err = verify(&LINUX, &archive).unwrap_err().to_string();
        assert!(err.contains("checksum mismatch"), "got: {err}");
        assert!(err.contains(LINUX.sha256), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn offline_download_is_loud_and_suggests_the_opt_out() {
        let _guard = crate::SERIAL.lock().unwrap();
        let mut spec = LINUX;
        spec.file = "cpython-offline-probe.tar.gz";
        let dir = tmpdir("offline");
        std::env::set_var("PITE_TOOLCHAIN_DIR", &dir);
        let err = ensure_archive(&spec).unwrap_err().to_string();
        assert!(err.contains("--no-bundle-python"), "got: {err}");
        std::env::remove_var("PITE_TOOLCHAIN_DIR");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stdlib_zip_excludes_site_packages() {
        let dir = tmpdir("zip");
        let stdlib = dir.join("stdlib");
        std::fs::create_dir_all(stdlib.join("site-packages")).unwrap();
        std::fs::create_dir_all(stdlib.join("__pycache__")).unwrap();
        std::fs::write(stdlib.join("os.py"), b"import sys\n").unwrap();
        std::fs::write(stdlib.join("site-packages").join("junk.py"), b"x = 1\n").unwrap();
        std::fs::write(stdlib.join("__pycache__").join("os.pyc"), b"junk").unwrap();
        let zip = dir.join("python312.zip");
        zip_dir(&stdlib, &zip).unwrap();
        let out = std::process::Command::new("python3")
            .arg("-c")
            .arg("import zipfile,sys; print(sorted(zipfile.ZipFile(sys.argv[1]).namelist()))")
            .arg(&zip)
            .output()
            .expect("python3 available to list the zip");
        let listing = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(listing.contains("os.py"), "got: {listing}");
        assert!(!listing.contains("site-packages"), "got: {listing}");
        assert!(!listing.contains("__pycache__"), "got: {listing}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The archive layout is an assumption `stage()` bails on halfway through a
    /// user's export, and the Windows one was wrong: `install_only` puts the
    /// stdlib at `Lib/` there, not `lib/python3.12/`. Read the real archive when
    /// it is cached so a layout change fails here instead of in the wild.
    #[test]
    fn pinned_layouts_match_the_cached_real_archives() {
        for platform in pinned_platforms() {
            let spec = pinned(platform).expect("pinned");
            let archive = archive_path(&spec);
            if !archive.is_file() {
                eprintln!(
                    "skipped {platform}: nothing cached at {} (run an export once to populate it)",
                    archive.display()
                );
                continue;
            }
            let out = std::process::Command::new("tar")
                .arg("-tf")
                .arg(&archive)
                .output()
                .expect("tar runs");
            assert!(out.status.success(), "cannot list {}", archive.display());
            let listing = String::from_utf8_lossy(&out.stdout);
            let lib = format!("{ARCHIVE_ROOT}/{}", spec.lib);
            let stdlib = format!("{ARCHIVE_ROOT}/{}", spec.stdlib);
            let has = |needle: &str| {
                listing
                    .lines()
                    .map(|l| l.trim_end_matches('/'))
                    .any(|l| l == needle || l.starts_with(&format!("{needle}/")))
            };
            assert!(has(&lib), "{platform}: archive has no interpreter at {lib}");
            assert!(
                has(&stdlib),
                "{platform}: archive has no stdlib at {stdlib}"
            );
            let stem = spec.lib.rsplit('/').next().unwrap_or(spec.lib);
            assert!(
                !FORWARDERS.contains(&stem),
                "{platform}: {} is an ABI forwarder, not the interpreter \
                 (it dispatches to the versioned library instead of being one)",
                spec.lib
            );
        }
    }

    #[test]
    fn launcher_env_points_at_lib_and_stdlib() {
        let bundle = StagedBundle {
            root: PathBuf::from("/game"),
            lib: PathBuf::from("/game/python/lib/libpython3.12.so.1.0"),
            stdlib_zip: PathBuf::from("/game/python/python312.zip"),
        };
        let env = launcher_env(&bundle);
        assert!(env.contains(&("LD_LIBRARY_PATH".into(), "/game/python/lib".into())));
        assert!(env.contains(&("PYTHONPATH".into(), "/game/python/python312.zip".into())));
    }
}
