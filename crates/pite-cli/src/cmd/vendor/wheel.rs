// SPDX-License-Identifier: MIT OR Apache-2.0
//! Wheel naming, inspection, RECORD parsing, and deterministic hashing helpers.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

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
        let name = entry.name()?;
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
        let name = entry.name()?.to_string();
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
                if let Ok(name) = item.name() {
                    if name == format!("{module}.py") || name.starts_with(&format!("{module}/")) {
                        return true;
                    }
                }
            }
        }
    }
    false
}
