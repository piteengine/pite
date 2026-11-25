// SPDX-License-Identifier: MIT OR Apache-2.0
//! Asset scanning, rename reconciliation, and reimporting for Pite.
//!
//! A single `assets.pite.toml` manifest file at the project root maps `res://` paths
//! to content hashes and file metadata. Scans are content-aware:
//! identical file hashes track filesystem renames and moves, while
//! modified file bytes trigger reimporting.

pub mod atlas;
mod error;
mod importers;
mod reimport;
#[cfg(test)]
mod tests;

pub use atlas::{
    atlas_report, atlas_report_structured, load_sidecar, resolve_sprite_source, sheet_path,
    AnimationUse, AtlasIssue, AtlasUse, SpriteSource,
};
pub use error::{AssetError, Result, SpriteSourceError};
pub use importers::{
    default_importers, FlacImporter, Importer, Mp3Importer, OggImporter, PngImporter, WavImporter,
};
pub use reimport::{
    apply_renames, apply_renames_detailed, find_legacy_atlases, migrate_legacy_atlases,
    played_assets, reimport_project, replace_res_ref, ReimportReport, RenameRewrite,
};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The asset registry manifest filename located at the project root.
pub const MANIFEST_FILE: &str = "assets.pite.toml";

/// A recorded asset entry in the project manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetEntry {
    /// Project-relative resource path (e.g., `"res://sprites/player.png"`).
    pub path: String,
    /// Hexadecimal FNV-1a content hash computed during the last scan.
    pub hash: String,
    /// File size in bytes.
    pub size: u64,
}

/// On-disk manifest shape: a diffable `[[asset]]` array sorted by path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ManifestDoc {
    #[serde(default)]
    asset: Vec<AssetEntry>,
}

/// In-memory asset registry mapping resource paths to [`AssetEntry`] records.
#[derive(Debug, Default, Clone)]
pub struct AssetRegistry {
    by_path: HashMap<String, AssetEntry>,
}

/// Backward-compatibility alias for [`AssetRegistry`].
pub type UidRegistry = AssetRegistry;

impl AssetRegistry {
    /// Registers or updates the asset entry for a given resource path.
    pub fn register(&mut self, path: impl Into<String>, hash: impl Into<String>, size: u64) {
        let path = path.into();
        let hash = hash.into();
        self.by_path
            .insert(path.clone(), AssetEntry { path, hash, size });
    }

    /// Retrieves the asset entry corresponding to a given `res://` path, if registered.
    pub fn get(&self, path: &str) -> Option<&AssetEntry> {
        self.by_path.get(path)
    }

    /// Retrieves the content hash corresponding to a given `res://` path, if registered.
    pub fn hash(&self, path: &str) -> Option<&str> {
        self.by_path.get(path).map(|e| e.hash.as_str())
    }

    /// Iterates over all registered asset entries.
    pub fn iter(&self) -> impl Iterator<Item = &AssetEntry> {
        self.by_path.values()
    }

    /// Returns the total number of registered assets.
    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    /// Returns true if the registry contains no assets.
    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }
}

/// FNV-1a 64-bit, rendered as 16 lowercase hex chars. Hashes file bytes
/// with zero extra dependencies.
fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 14695981039346656037;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

/// One importer-matched file found under the project root.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub res_path: String,
    pub fs_path: PathBuf,
    pub hash: String,
    pub size: u64,
}

/// Pure diff between the last manifest and the current file set.
/// Every vector is sorted for stable, diffable output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanReport {
    pub scanned: usize,
    pub added: Vec<String>,
    pub renamed: Vec<(String, String)>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

/// Directory names never descended into during any walk.
pub(crate) fn is_skipped_dir(name: &str) -> bool {
    name == "target" || name == "dist" || name == ".git"
}

/// `res://`-rooted forward-slash path for `fs_path` under `root`.
pub(crate) fn res_path_for(root: &Path, fs_path: &Path) -> Result<String> {
    let rel = fs_path
        .strip_prefix(root)
        .map_err(|_| AssetError::OutsideRoot {
            path: fs_path.to_path_buf(),
            root: root.to_path_buf(),
        })?;
    let joined = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    Ok(format!("res://{joined}"))
}

/// Recursive walk of `root`, skipping hidden files/dirs (any component
/// starting with `.`), `target/`, `dist/`, `.git/`. Keeps only files
/// whose lowercase extension matches a [`default_importers`] entry.
pub fn scan_files(root: &Path) -> Result<Vec<ScannedFile>> {
    let importers = default_importers();
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|source| AssetError::Io {
            op: "read dir",
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| AssetError::Io {
                op: "read entry in",
                path: dir.clone(),
                source,
            })?;
            let fs_path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if fs_path.is_dir() {
                if is_skipped_dir(&name) {
                    continue;
                }
                stack.push(fs_path);
                continue;
            }
            if !fs_path.is_file() {
                continue;
            }
            let ext = fs_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !importers.iter().any(|i| i.match_ext(&ext)) {
                continue;
            }
            let bytes = std::fs::read(&fs_path).map_err(|source| AssetError::Io {
                op: "read",
                path: fs_path.clone(),
                source,
            })?;
            out.push(ScannedFile {
                res_path: res_path_for(root, &fs_path)?,
                fs_path,
                hash: fnv1a_hex(&bytes),
                size: bytes.len() as u64,
            });
        }
    }
    out.sort_by(|a, b| a.res_path.cmp(&b.res_path));
    Ok(out)
}

/// Load the manifest; a missing file means an empty registry, not an error.
pub fn load_registry(root: &Path) -> Result<AssetRegistry> {
    let path = root.join(MANIFEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AssetRegistry::default())
        }
        Err(source) => {
            return Err(AssetError::Io {
                op: "read",
                path,
                source,
            });
        }
    };
    if text.trim().is_empty() {
        return Ok(AssetRegistry::default());
    }
    let doc: ManifestDoc =
        toml::from_str(&text).map_err(|source| AssetError::TomlParse { path, source })?;
    let mut reg = AssetRegistry::default();
    for entry in doc.asset {
        reg.by_path.insert(entry.path.clone(), entry);
    }
    Ok(reg)
}

/// Save the manifest with entries always sorted by path (diffable).
pub fn save_registry(root: &Path, reg: &AssetRegistry) -> Result<()> {
    let mut entries: Vec<&AssetEntry> = reg.iter().collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let doc = ManifestDoc {
        asset: entries.into_iter().cloned().collect(),
    };
    let text = toml::to_string(&doc)?;
    let path = root.join(MANIFEST_FILE);
    std::fs::write(&path, text).map_err(|source| AssetError::Io {
        op: "write",
        path,
        source,
    })?;
    Ok(())
}

/// Pure manifest-vs-files diff, no IO. Same path + same hash is unchanged;
/// same path + new hash is `changed`; a manifest path missing from
/// disk but shadowed by an unclaimed file with equal (hash, size) is a
/// `rename`; otherwise `removed` / `added`.
pub fn reconcile(old: &AssetRegistry, files: &[ScannedFile]) -> (AssetRegistry, ScanReport) {
    let mut next = AssetRegistry::default();
    let mut added = Vec::new();
    let mut renamed = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    let mut files_by_path: HashMap<&str, &ScannedFile> = HashMap::new();
    for file in files {
        files_by_path.insert(file.res_path.as_str(), file);
    }

    let mut old_paths: Vec<&str> = old.by_path.keys().map(String::as_str).collect();
    old_paths.sort();

    let mut claimed: HashSet<&str> = HashSet::new();
    let mut accounted: HashSet<&str> = HashSet::new();

    for old_path in &old_paths {
        if let Some(entry) = old.by_path.get(*old_path) {
            if let Some(file) = files_by_path.get(*old_path) {
                accounted.insert(*old_path);
                claimed.insert(file.res_path.as_str());
                if file.hash == entry.hash {
                    next.by_path.insert(
                        (*old_path).to_string(),
                        AssetEntry {
                            path: (*old_path).to_string(),
                            hash: file.hash.clone(),
                            size: file.size,
                        },
                    );
                } else {
                    changed.push((*old_path).to_string());
                    next.by_path.insert(
                        (*old_path).to_string(),
                        AssetEntry {
                            path: (*old_path).to_string(),
                            hash: file.hash.clone(),
                            size: file.size,
                        },
                    );
                }
            }
        }
    }

    let mut orphans: Vec<&AssetEntry> = old_paths
        .iter()
        .filter(|p| !accounted.contains(**p))
        .filter_map(|p| old.by_path.get(*p))
        .collect();
    orphans.sort_by(|a, b| a.path.cmp(&b.path));

    let mut unclaimed: Vec<&ScannedFile> = files
        .iter()
        .filter(|f| !claimed.contains(f.res_path.as_str()))
        .collect();
    unclaimed.sort_by(|a, b| a.res_path.cmp(&b.res_path));

    let mut used: HashSet<usize> = HashSet::new();
    let mut still_orphaned: Vec<&AssetEntry> = Vec::new();
    for orphan in orphans {
        let mut hit: Option<usize> = None;
        for (idx, file) in unclaimed.iter().enumerate() {
            if used.contains(&idx) {
                continue;
            }
            if file.hash == orphan.hash && file.size == orphan.size {
                hit = Some(idx);
                break;
            }
        }
        match hit {
            Some(idx) => {
                used.insert(idx);
                let file = unclaimed[idx];
                renamed.push((orphan.path.clone(), file.res_path.clone()));
                next.by_path.insert(
                    file.res_path.clone(),
                    AssetEntry {
                        path: file.res_path.clone(),
                        hash: file.hash.clone(),
                        size: file.size,
                    },
                );
            }
            None => still_orphaned.push(orphan),
        }
    }

    for orphan in still_orphaned {
        removed.push(orphan.path.clone());
    }

    for (idx, file) in unclaimed.iter().enumerate() {
        if used.contains(&idx) {
            continue;
        }
        added.push(file.res_path.clone());
        next.by_path.insert(
            file.res_path.clone(),
            AssetEntry {
                path: file.res_path.clone(),
                hash: file.hash.clone(),
                size: file.size,
            },
        );
    }

    added.sort();
    renamed.sort();
    removed.sort();
    changed.sort();

    (
        next,
        ScanReport {
            scanned: files.len(),
            added,
            renamed,
            removed,
            changed,
        },
    )
}

/// Load + scan + reconcile + save in one step.
pub fn scan_project(root: &Path) -> Result<(AssetRegistry, ScanReport)> {
    let old = load_registry(root)?;
    let files = scan_files(root)?;
    let (next, report) = reconcile(&old, &files);
    save_registry(root, &next)?;
    Ok((next, report))
}
