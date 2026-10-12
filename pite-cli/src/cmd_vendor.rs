// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite vendor`: pin third-party Python packages into `vendor/`.
//!
//! Policy (see docs): zipped wheels by default, unpacked when the wheel
//! carries native modules (`.so` can't load from a zip) or `--unpacked`.
//! Absent form means zipped; the lock records version + artifact hash only.
//! Transitive deps resolve at vendor-time via pip and each gets a row.
//! Nothing here touches the network except `add` (pip download).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Project-relative vendor directory.
pub const VENDOR_DIR: &str = "vendor";

/// PEP 503-ish normalization for comparing distribution names.
pub fn normalize_dist(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for c in name.chars() {
        if c == '-' || c == '_' || c == '.' {
            if !dash {
                out.push('-');
            }
            dash = true;
        } else {
            out.push(c.to_ascii_lowercase());
            dash = false;
        }
    }
    out
}

/// `(distribution, version)` from a wheel filename, or `None` when the name
/// doesn't parse. Wheel layout: `{dist}-{ver}(-{build})?-…`.
pub fn parse_wheel_filename(file_name: &str) -> Option<(String, String)> {
    let stem = file_name.strip_suffix(".whl")?;
    let mut parts = stem.split('-');
    let dist = parts.next()?.to_string();
    let version = parts.next()?.to_string();
    if dist.is_empty() || version.is_empty() {
        return None;
    }
    Some((dist, version))
}

/// True when the wheel ships native modules, which forces unpacked storage:
/// the dynamic loader can't reach `.so` files inside a zip.
pub fn wheel_has_native(wheel: &Path) -> Result<bool> {
    let file = std::fs::File::open(wheel)
        .with_context(|| format!("cannot open wheel {}", wheel.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("cannot read wheel {} as a zip", wheel.display()))?;
    for i in 0..zip.len() {
        let entry = zip
            .by_index(i)
            .with_context(|| format!("cannot list entry {i} in {}", wheel.display()))?;
        let name = entry.name();
        if name.ends_with(".so")
            || name.ends_with(".pyd")
            || name.ends_with(".dll")
            || name.ends_with(".dylib")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Top-level install entries of a wheel (first path components), minus
/// `dist-info`. Errors on `.data/` layouts, whose install destinations the
/// engine doesn't model — pick a version without one.
pub fn wheel_top_levels(wheel: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(wheel)
        .with_context(|| format!("cannot open wheel {}", wheel.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("cannot read wheel {} as a zip", wheel.display()))?;
    let mut tops = Vec::new();
    for i in 0..zip.len() {
        let entry = zip
            .by_index(i)
            .with_context(|| format!("cannot list entry {i} in {}", wheel.display()))?;
        let name = entry.name().to_string();
        let top = name.split('/').next().unwrap_or("").to_string();
        if top.is_empty() || top.ends_with(".dist-info") {
            continue;
        }
        if top.ends_with(".data") {
            anyhow::bail!(
                "wheel {} has a .data/ layout, which vendor/ doesn't model; pick another version",
                wheel.display()
            );
        }
        if !tops.contains(&top) {
            tops.push(top);
        }
    }
    Ok(tops)
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(hex_hash(&bytes))
}

fn hex_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// File rows of a `RECORD` file: installed paths, `/`-separated.
pub fn read_record(record: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(record)
        .with_context(|| format!("cannot read {}", record.display()))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let path = line.split(',').next().unwrap_or("").trim().to_string();
        if !path.is_empty() {
            out.push(path);
        }
    }
    Ok(out)
}

/// Canonical hash of unpacked files: sorted `path + NUL + bytes` chained.
/// Deterministic regardless of walk or zip order.
pub fn hash_unpacked(vendor_dir: &Path, rel_paths: &[String]) -> Result<String> {
    let mut sorted = rel_paths.to_vec();
    sorted.sort_unstable();
    let mut hasher = Sha256::new();
    for rel in &sorted {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        let bytes = std::fs::read(vendor_dir.join(rel))
            .with_context(|| format!("cannot read {}", vendor_dir.join(rel).display()))?;
        hasher.update(&bytes);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// `vendor/{dist}-{version}.dist-info`, with `-` escaped as `_` per the
/// wheel filename rules.
pub fn find_dist_info(vendor_dir: &Path, dist: &str, version: &str) -> Result<PathBuf> {
    let escaped = dist.replace('-', "_");
    let dir = vendor_dir.join(format!("{escaped}-{version}.dist-info"));
    if dir.is_dir() {
        Ok(dir)
    } else {
        anyhow::bail!("no unpacked {dist} {version} in {}", vendor_dir.display())
    }
}

/// Wheel file matching distribution + version exactly.
pub fn find_wheel(vendor_dir: &Path, dist: &str, version: &str) -> Result<PathBuf> {
    let mut hits = Vec::new();
    let entries = std::fs::read_dir(vendor_dir)
        .with_context(|| format!("cannot list {}", vendor_dir.display()))?;
    for entry in entries {
        let entry =
            entry.with_context(|| format!("cannot list entry in {}", vendor_dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".whl") {
            continue;
        }
        if let Some((d, v)) = parse_wheel_filename(&name) {
            if normalize_dist(&d) == dist && v == version {
                hits.push(entry.path());
            }
        }
    }
    match hits.len() {
        0 => anyhow::bail!("no {dist} {version} wheel in {}", vendor_dir.display()),
        1 => Ok(hits.remove(0)),
        _ => anyhow::bail!("ambiguous {dist} {version} wheels: {hits:?}"),
    }
}

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    pite_project::find_project_root(&cwd)
        .with_context(|| "pite vendor must run inside a project (no pite.toml above cwd)")
}

fn pip_program() -> String {
    if let Some(bin) = std::env::var_os("PITE_PIP_BIN") {
        return bin.to_string_lossy().into_owned();
    }
    "pip".to_string()
}

/// `pip download` honoring the offline test seam: `PITE_PIP_FIND_LINKS`
/// switches to `--no-index` plus a local directory.
fn pip_download(spec: &str, dest: &Path) -> Result<()> {
    let mut cmd = std::process::Command::new(pip_program());
    cmd.arg("download").arg(spec).arg("-d").arg(dest);
    cmd.arg("--only-binary=:all:")
        .arg("--python-version")
        .arg("3.12");
    if let Some(links) = std::env::var_os("PITE_PIP_FIND_LINKS") {
        cmd.arg("--no-index").arg("--find-links").arg(links);
    }
    let output = cmd
        .output()
        .with_context(|| "cannot run pip (install pip or set PITE_PIP_BIN to a pip binary)")?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = tail.lines().collect();
        let tail = tail
            .iter()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev();
        anyhow::bail!(
            "pip download {spec:?} failed:\n{}",
            tail.cloned().collect::<Vec<_>>().join("\n")
        );
    }
    Ok(())
}

fn parse_spec(spec: &str) -> Result<String> {
    match spec.split_once("==") {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            Ok(format!("{}=={}", name.trim(), version.trim()))
        }
        None if !spec.is_empty() && !spec.contains(['>', '<', '~', '!', ';', ' ']) => {
            Ok(spec.to_string())
        }
        _ => anyhow::bail!("pin the spec with == (got {spec:?})"),
    }
}

pub fn add(spec: &str, zipped: bool, unpacked: bool) -> Result<()> {
    if zipped && unpacked {
        anyhow::bail!("--zipped and --unpacked conflict; pick one");
    }
    let root = project_root()?;
    let mut manifest = pite_project::load_manifest(&root)?;
    let pinned = parse_spec(spec)?;
    let requested = normalize_dist(pinned.split("==").next().unwrap_or(&pinned));
    let tmp = std::env::temp_dir().join(format!("pite-vendor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    pip_download(&pinned, &tmp)?;
    let mut wheels: Vec<PathBuf> = std::fs::read_dir(&tmp)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "whl"))
        .collect();
    wheels.sort_unstable();
    if wheels.is_empty() {
        anyhow::bail!("pip downloaded nothing for {spec:?}");
    }
    let vendor_dir = root.join(VENDOR_DIR);
    std::fs::create_dir_all(&vendor_dir)?;
    let mut added: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for wheel in &wheels {
        let file_name = wheel
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (dist_raw, version) = parse_wheel_filename(&file_name)
            .with_context(|| format!("cannot parse wheel filename {file_name:?}"))?;
        let dist = normalize_dist(&dist_raw);
        let native = wheel_has_native(wheel)?;
        let keep_zipped = if unpacked {
            false
        } else if zipped {
            if native {
                anyhow::bail!(
                    "{dist_raw} ships native modules, which can't load from a zip; drop --zipped"
                );
            }
            true
        } else {
            !native
        };
        let sha = sha256_file(wheel)?;
        if keep_zipped {
            let dest = vendor_dir.join(&file_name);
            if dest.is_file() && sha256_file(&dest)? == sha {
                // Re-adding the same bytes is a no-op, not an error.
            } else {
                std::fs::copy(wheel, &dest).with_context(|| {
                    format!("cannot stage {} into {}", wheel.display(), dest.display())
                })?;
            }
        } else {
            let tops = wheel_top_levels(wheel)?;
            for top in &tops {
                if vendor_dir.join(top).exists() {
                    anyhow::bail!(
                        "{dist_raw} wants vendor/{top}, which already exists; remove the owner first"
                    );
                }
            }
            let file = std::fs::File::open(wheel)?;
            let mut zip = zip::ZipArchive::new(file)?;
            zip.extract(&vendor_dir).with_context(|| {
                format!(
                    "cannot unpack {} into {}",
                    wheel.display(),
                    vendor_dir.display()
                )
            })?;
            manifest.vendor.insert(
                dist.clone(),
                pite_project::VendorEntry {
                    version: version.clone(),
                    sha256: tree_hash_for(&vendor_dir, &dist, &version)?,
                    direct: dist == requested,
                },
            );
            added.insert(dist.clone(), (version.clone(), false));
            continue;
        }
        manifest.vendor.insert(
            dist.clone(),
            pite_project::VendorEntry {
                version: version.clone(),
                sha256: sha,
                direct: dist == requested,
            },
        );
        added.insert(dist, (version, true));
    }
    pite_project::save_manifest(&root, &manifest)?;
    let _ = std::fs::remove_dir_all(&tmp);
    for (dist, (version, was_zipped)) in &added {
        println!(
            "vendor: {dist} {version} ({})",
            if *was_zipped { "zipped" } else { "unpacked" }
        );
    }
    Ok(())
}

fn tree_hash_for(vendor_dir: &Path, dist: &str, version: &str) -> Result<String> {
    let info = find_dist_info(vendor_dir, dist, version)?;
    let rels = read_record(&info.join("RECORD"))?;
    hash_unpacked(vendor_dir, &rels)
}

/// True when `module` resolves inside `vendor_dir`: an unpacked tree or a
/// top-level entry of a zipped wheel. Used by `pite check` so vendored
/// imports stay silent.
pub fn vendor_provides(vendor_dir: &Path, module: &str) -> bool {
    if vendor_dir.join(format!("{module}.py")).is_file()
        || vendor_dir.join(module).join("__init__.py").is_file()
    {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(vendor_dir) else {
        return false;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        if !entry.path().extension().is_some_and(|e| e == "whl") {
            continue;
        }
        let Ok(file) = std::fs::File::open(entry.path()) else {
            continue;
        };
        let Ok(mut zip) = zip::ZipArchive::new(file) else {
            continue;
        };
        for i in 0..zip.len() {
            if let Ok(item) = zip.by_index(i) {
                let name = item.name();
                if name == format!("{module}.py") || name.starts_with(&format!("{module}/")) {
                    return true;
                }
            }
        }
    }
    false
}

pub fn remove(name: &str) -> Result<()> {
    let root = project_root()?;
    let mut manifest = pite_project::load_manifest(&root)?;
    let dist = normalize_dist(name);
    let entry = manifest
        .vendor
        .get(&dist)
        .with_context(|| format!("{name:?} is not vendored"))?
        .clone();
    let vendor_dir = root.join(VENDOR_DIR);
    if let Ok(wheel) = find_wheel(&vendor_dir, &dist, &entry.version) {
        std::fs::remove_file(&wheel)
            .with_context(|| format!("cannot delete {}", wheel.display()))?;
    } else {
        let info = find_dist_info(&vendor_dir, &dist, &entry.version)?;
        for rel in read_record(&info.join("RECORD"))? {
            let p = vendor_dir.join(&rel);
            if p.is_file() {
                std::fs::remove_file(&p)
                    .with_context(|| format!("cannot delete {}", p.display()))?;
            }
        }
        remove_empty_parents(&info, &vendor_dir);
    }
    manifest.vendor.remove(&dist);
    pite_project::save_manifest(&root, &manifest)?;
    println!("vendor: removed {dist}");
    Ok(())
}

fn remove_empty_parents(dir: &Path, stop: &Path) {
    let mut dir = dir.to_path_buf();
    while dir.as_path() != stop && dir.starts_with(stop) {
        match std::fs::remove_dir(&dir) {
            Ok(()) => {}
            Err(_) => return,
        }
        if !dir.pop() {
            return;
        }
    }
}

pub fn list() -> Result<()> {
    let root = project_root()?;
    let manifest = pite_project::load_manifest(&root)?;
    let vendor_dir = root.join(VENDOR_DIR);
    let mut names: Vec<&String> = manifest.vendor.keys().collect();
    names.sort_unstable();
    for name in names {
        let entry = &manifest.vendor[name];
        let present = find_wheel(&vendor_dir, name, &entry.version).is_ok()
            || find_dist_info(&vendor_dir, name, &entry.version).is_ok();
        println!(
            "{} {} {} {}",
            name,
            entry.version,
            if entry.direct { "direct" } else { "transitive" },
            if present { "ok" } else { "MISSING" }
        );
    }
    Ok(())
}

pub fn verify() -> Result<()> {
    let root = project_root()?;
    let manifest = pite_project::load_manifest(&root)?;
    let vendor_dir = root.join(VENDOR_DIR);
    let mut bad = 0;
    let mut names: Vec<&String> = manifest.vendor.keys().collect();
    names.sort_unstable();
    for name in names {
        let entry = &manifest.vendor[name];
        let hash = find_wheel(&vendor_dir, name, &entry.version)
            .and_then(|w| sha256_file(&w))
            .or_else(|_| tree_hash_for(&vendor_dir, name, &entry.version));
        match hash {
            Ok(h) if h == entry.sha256 => println!("{name} ok"),
            Ok(_) => {
                println!("{name} MISMATCH");
                bad += 1;
            }
            Err(e) => {
                println!("{name} MISSING ({e:#})");
                bad += 1;
            }
        }
    }
    if bad > 0 {
        anyhow::bail!("{bad} vendored package(s) failed verification");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// `add`/`remove` mutate the process cwd and env: never run together.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn make_wheel(dir: &Path, dist: &str, version: &str, native: bool) -> PathBuf {
        let name = format!("{dist}-{version}-py3-none-any.whl");
        let path = dir.join(&name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file(format!("{dist}/__init__.py"), opts).unwrap();
        zip.write_all(b"x = 1\n").unwrap();
        if native {
            zip.start_file(format!("{dist}/_n.so"), opts).unwrap();
            zip.write_all(b"\x7fELF").unwrap();
        }
        let dist_info = format!("{dist}-{version}.dist-info");
        for f in ["METADATA", "WHEEL", "RECORD"] {
            zip.start_file(format!("{dist_info}/{f}"), opts).unwrap();
        }
        zip.write_all(b"").unwrap();
        zip.finish().unwrap();
        path
    }

    #[test]
    fn wheel_helpers_behave() {
        assert_eq!(normalize_dist("Pillow"), "pillow");
        assert_eq!(normalize_dist("a_b.c-d"), "a-b-c-d");
        assert_eq!(
            parse_wheel_filename("demo_pkg-1.0-py3-none-any.whl"),
            Some(("demo_pkg".to_string(), "1.0".to_string()))
        );
        assert_eq!(parse_wheel_filename("nope.zip"), None);
        let dir = std::env::temp_dir().join(format!("pite-vt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pure = make_wheel(&dir, "pure", "1.0", false);
        let nat = make_wheel(&dir, "nat", "2.0", true);
        assert!(!wheel_has_native(&pure).unwrap());
        assert!(wheel_has_native(&nat).unwrap());
        assert_eq!(wheel_top_levels(&pure).unwrap(), vec!["pure".to_string()]);
        assert!(parse_spec("a==1.0").is_ok());
        assert!(parse_spec("a>=1.0").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vendor_provides_sees_trees_and_zips() {
        let dir = std::env::temp_dir().join(format!("pite-vp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!vendor_provides(&dir, "anything"));
        std::fs::create_dir_all(dir.join("treepkg")).unwrap();
        std::fs::write(dir.join("treepkg").join("__init__.py"), b"x=1").unwrap();
        std::fs::write(dir.join("mod.py"), b"y=2").unwrap();
        make_wheel(&dir, "zippkg", "1.0", false);
        assert!(vendor_provides(&dir, "treepkg"));
        assert!(vendor_provides(&dir, "mod"));
        assert!(vendor_provides(&dir, "zippkg"));
        assert!(!vendor_provides(&dir, "missing"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fake `pip download SPEC -d DIR …`: copies fixture wheels, ignoring
    /// flags. Drives `add` end to end with no network.
    const FAKE_PIP: &str = "#!/bin/sh\ndest=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-d\" ]; then dest=\"$a\"; fi\n  prev=\"$a\"\ndone\ncp \"$PITE_FAKE_WHEELS\"/*.whl \"$dest\"/\n";

    fn fake_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-vendor-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        dir
    }

    #[test]
    fn add_verify_remove_round_trip() {
        let _guard = SERIAL.lock().unwrap();
        let wheels = std::env::temp_dir().join(format!("pite-vw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wheels);
        std::fs::create_dir_all(&wheels).unwrap();
        make_wheel(&wheels, "demo", "1.0", false);
        make_wheel(&wheels, "natpkg", "2.0", true);
        let fake = wheels.join("pip");
        std::fs::write(&fake, FAKE_PIP).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dir = fake_project("roundtrip");
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::env::set_var("PITE_PIP_BIN", &fake);
        std::env::set_var("PITE_FAKE_WHEELS", &wheels);
        add("demo==1.0", false, false).unwrap();
        let manifest = pite_project::load_manifest(&dir).unwrap();
        assert!(manifest.vendor["demo"].direct);
        assert!(!manifest.vendor["natpkg"].direct);
        assert!(dir
            .join("vendor")
            .join("demo-1.0-py3-none-any.whl")
            .is_file());
        assert!(dir
            .join("vendor")
            .join("natpkg")
            .join("__init__.py")
            .is_file());
        verify().unwrap();
        std::fs::write(
            dir.join("vendor").join("demo-1.0-py3-none-any.whl"),
            b"corrupt",
        )
        .unwrap();
        assert!(verify().is_err());
        std::env::remove_var("PITE_PIP_BIN");
        std::env::remove_var("PITE_FAKE_WHEELS");
        std::env::set_current_dir(&prev).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&wheels);
    }

    #[test]
    fn remove_deletes_artifact_and_row() {
        let _guard = SERIAL.lock().unwrap();
        let wheels = std::env::temp_dir().join(format!("pite-vw2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wheels);
        std::fs::create_dir_all(&wheels).unwrap();
        make_wheel(&wheels, "solo", "3.0", false);
        let fake = wheels.join("pip");
        std::fs::write(&fake, FAKE_PIP).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dir = fake_project("remove");
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::env::set_var("PITE_PIP_BIN", &fake);
        std::env::set_var("PITE_FAKE_WHEELS", &wheels);
        add("solo", false, false).unwrap();
        remove("solo").unwrap();
        let manifest = pite_project::load_manifest(&dir).unwrap();
        assert!(manifest.vendor.is_empty());
        assert!(!dir
            .join("vendor")
            .join("solo-3.0-py3-none-any.whl")
            .exists());
        assert!(remove("solo").is_err());
        std::env::remove_var("PITE_PIP_BIN");
        std::env::remove_var("PITE_FAKE_WHEELS");
        std::env::set_current_dir(&prev).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&wheels);
    }
}
