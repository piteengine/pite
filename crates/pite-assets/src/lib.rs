// SPDX-License-Identifier: MIT OR Apache-2.0
//! Asset scanning, UID tracking, rename reconciliation, and reimporting for Pite.
//!
//! A single `assets.pite.toml` manifest file at the project root maps `res://` paths
//! to persistent, opaque 64-bit hexadecimal identifiers (UIDs). Scans are content-aware:
//! identical file hashes preserve their UID across filesystem renames and moves, while
//! modified file bytes trigger reimporting.

pub mod atlas;

pub use atlas::{atlas_report, load_sidecar, resolve_sprite_source, sheet_path};
pub use atlas::{AtlasUse, SpriteSource};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The asset registry manifest filename located at the project root.
pub const MANIFEST_FILE: &str = "assets.pite.toml";

/// 8-byte PNG signature every valid `.png` file must start with.
const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Opaque UID generator state ensuring uniqueness within and across asset scans.
static UID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Trait implemented by asset importers to validate and ingest specific file types.
pub trait Importer: Send + Sync {
    /// Returns true if this importer handles files with the given file extension (case-insensitive).
    fn match_ext(&self, ext: &str) -> bool;

    /// Validates and imports the asset at the specified filesystem path.
    fn import(&self, path: &str) -> Result<()>;
}

/// Asset importer for PNG raster images.
pub struct PngImporter;

impl Importer for PngImporter {
    fn match_ext(&self, ext: &str) -> bool {
        ext.eq_ignore_ascii_case("png")
    }

    fn import(&self, path: &str) -> Result<()> {
        let bytes = std::fs::read(path).with_context(|| format!("asset not found: {path}"))?;
        if bytes.len() < PNG_SIG.len() || bytes[..PNG_SIG.len()] != PNG_SIG {
            anyhow::bail!("not a PNG asset: {path}");
        }
        Ok(())
    }
}

/// Asset importer for WAV audio files.
pub struct WavImporter;

impl Importer for WavImporter {
    fn match_ext(&self, ext: &str) -> bool {
        ext.eq_ignore_ascii_case("wav")
    }

    fn import(&self, path: &str) -> Result<()> {
        let bytes = std::fs::read(path).with_context(|| format!("asset not found: {path}"))?;
        if bytes.len() < 12 || bytes[0..4] != *b"RIFF" || bytes[8..12] != *b"WAVE" {
            anyhow::bail!("not a WAV asset: {path}");
        }
        Ok(())
    }
}

/// Asset importer for Ogg Vorbis audio files.
pub struct OggImporter;

impl Importer for OggImporter {
    fn match_ext(&self, ext: &str) -> bool {
        ext.eq_ignore_ascii_case("ogg")
    }

    fn import(&self, path: &str) -> Result<()> {
        let bytes = std::fs::read(path).with_context(|| format!("asset not found: {path}"))?;
        if bytes.len() < 4 || bytes[0..4] != *b"OggS" {
            anyhow::bail!("not an OGG asset: {path}");
        }
        Ok(())
    }
}

/// Asset importer for MP3 audio files.
pub struct Mp3Importer;

impl Importer for Mp3Importer {
    fn match_ext(&self, ext: &str) -> bool {
        ext.eq_ignore_ascii_case("mp3")
    }

    fn import(&self, path: &str) -> Result<()> {
        let bytes = std::fs::read(path).with_context(|| format!("asset not found: {path}"))?;
        let is_id3 = bytes.len() >= 3 && &bytes[0..3] == b"ID3";
        let is_sync = bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0;
        if !is_id3 && !is_sync {
            anyhow::bail!("not an MP3 asset: {path}");
        }
        Ok(())
    }
}

/// Asset importer for FLAC audio files.
pub struct FlacImporter;

impl Importer for FlacImporter {
    fn match_ext(&self, ext: &str) -> bool {
        ext.eq_ignore_ascii_case("flac")
    }

    fn import(&self, path: &str) -> Result<()> {
        let bytes = std::fs::read(path).with_context(|| format!("asset not found: {path}"))?;
        if bytes.len() < 4 || bytes[0..4] != *b"fLaC" {
            anyhow::bail!("not a FLAC asset: {path}");
        }
        Ok(())
    }
}

/// A recorded asset entry in the project manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetEntry {
    /// Project-relative resource path (e.g., `"res://sprites/player.png"`).
    pub path: String,
    /// Stable opaque 16-character hexadecimal identifier.
    pub uid: String,
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

/// In-memory asset UID registry mapping resource paths to [`AssetEntry`] records.
#[derive(Debug, Default, Clone)]
pub struct UidRegistry {
    by_path: HashMap<String, AssetEntry>,
}

impl UidRegistry {
    /// Registers or updates the UID assigned to a given resource path.
    pub fn register(&mut self, path: impl Into<String>, uid: impl Into<String>) {
        let path = path.into();
        let uid = uid.into();
        match self.by_path.get_mut(&path) {
            Some(entry) => entry.uid = uid,
            None => {
                self.by_path.insert(
                    path.clone(),
                    AssetEntry {
                        path,
                        uid,
                        hash: String::new(),
                        size: 0,
                    },
                );
            }
        }
    }

    /// Retrieves the UID corresponding to a given `res://` path, if registered.
    pub fn get(&self, path: &str) -> Option<&str> {
        self.by_path.get(path).map(|e| e.uid.as_str())
    }

    /// Finds the `res://` path assigned to a given UID, if present in the registry.
    pub fn path_for_uid(&self, uid: &str) -> Option<&str> {
        self.by_path
            .values()
            .find(|e| e.uid == uid)
            .map(|e| e.path.as_str())
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

/// Returns the default list of built-in file importers (PNG, WAV, OGG, MP3, FLAC).
pub fn default_importers() -> Vec<Box<dyn Importer>> {
    vec![
        Box::new(PngImporter),
        Box::new(WavImporter),
        Box::new(OggImporter),
        Box::new(Mp3Importer),
        Box::new(FlacImporter),
    ]
}

/// FNV-1a 64-bit, rendered as 16 lowercase hex chars. Hashes file bytes
/// and mints uids alike, with zero extra dependencies.
fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 14695981039346656037;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

/// Mint an opaque 16-lowercase-hex uid bound to `path` plus wall-clock,
/// pid, and an atomic counter so back-to-back scans never collide.
fn new_uid(path: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let count = UID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    fnv1a_hex(format!("{path}:{nanos}:{pid}:{count}").as_bytes())
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

/// Outcome of [`reimport_project`]: importer results plus ref rewrites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReimportReport {
    pub reimported: Vec<String>,
    pub failed: Vec<String>,
    pub refs_updated: Vec<String>,
}

/// Directory names never descended into during any walk.
fn is_skipped_dir(name: &str) -> bool {
    name == "target" || name == "dist" || name == ".git"
}

/// `res://`-rooted forward-slash path for `fs_path` under `root`.
fn res_path_for(root: &Path, fs_path: &Path) -> Result<String> {
    let rel = fs_path
        .strip_prefix(root)
        .with_context(|| format!("path {} outside root {}", fs_path.display(), root.display()))?;
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
        let entries =
            std::fs::read_dir(&dir).with_context(|| format!("read dir {}", dir.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("read entry in {}", dir.display()))?;
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
            let bytes = std::fs::read(&fs_path)
                .with_context(|| format!("read asset {}", fs_path.display()))?;
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
pub fn load_registry(root: &Path) -> Result<UidRegistry> {
    let path = root.join(MANIFEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(UidRegistry::default()),
        Err(err) => {
            return Err(err).with_context(|| format!("read {}", path.display()));
        }
    };
    if text.trim().is_empty() {
        return Ok(UidRegistry::default());
    }
    let doc: ManifestDoc =
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    let mut reg = UidRegistry::default();
    for entry in doc.asset {
        reg.by_path.insert(entry.path.clone(), entry);
    }
    Ok(reg)
}

/// Save the manifest with entries always sorted by path (diffable).
pub fn save_registry(root: &Path, reg: &UidRegistry) -> Result<()> {
    let mut entries: Vec<&AssetEntry> = reg.iter().collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let doc = ManifestDoc {
        asset: entries.into_iter().cloned().collect(),
    };
    let text = toml::to_string(&doc).context("serialize asset manifest")?;
    std::fs::write(root.join(MANIFEST_FILE), text)
        .with_context(|| format!("write {}", root.join(MANIFEST_FILE).display()))?;
    Ok(())
}

/// Pure manifest-vs-files diff, no IO. Same path + same hash keeps the
/// uid; same path + new hash is `changed`; a manifest path missing from
/// disk but shadowed by an unclaimed file with equal (hash, size) is a
/// `rename` (uid travels); otherwise `removed` / `added` (fresh uid).
pub fn reconcile(old: &UidRegistry, files: &[ScannedFile]) -> (UidRegistry, ScanReport) {
    let mut next = UidRegistry::default();
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
                            uid: entry.uid.clone(),
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
                            uid: entry.uid.clone(),
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
                        uid: orphan.uid.clone(),
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
        let uid = new_uid(&file.res_path);
        added.push(file.res_path.clone());
        next.by_path.insert(
            file.res_path.clone(),
            AssetEntry {
                path: file.res_path.clone(),
                uid,
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
pub fn scan_project(root: &Path) -> Result<(UidRegistry, ScanReport)> {
    let old = load_registry(root)?;
    let files = scan_files(root)?;
    let (next, report) = reconcile(&old, &files);
    save_registry(root, &next)?;
    Ok((next, report))
}

/// Replace only quote-wrapped `res://` refs (`"res://old"`, `'res://old'`)
/// with the new ref. Bare, unquoted occurrences are left untouched.
pub fn replace_res_ref(text: &str, old: &str, new: &str) -> (String, usize) {
    let dq_old = format!("\"{old}\"");
    let dq_new = format!("\"{new}\"");
    let sq_old = format!("'{old}'");
    let sq_new = format!("'{new}'");
    let dq_count = text.matches(dq_old.as_str()).count();
    let after_dq = text.replace(dq_old.as_str(), dq_new.as_str());
    let sq_count = after_dq.matches(sq_old.as_str()).count();
    let out = after_dq.replace(sq_old.as_str(), sq_new.as_str());
    (out, dq_count + sq_count)
}

/// Rewrite every `*.ptscn` / `*.py` file under `root` (same skip
/// rules as [`scan_files`]) for each `(old, new)` rename. Only files
/// with at least one quote-wrapped hit are written back. Returns the
/// sorted `res://`-style paths of touched files.
pub fn apply_renames(root: &Path, renames: &[(String, String)]) -> Result<Vec<String>> {
    if renames.is_empty() {
        return Ok(Vec::new());
    }
    let mut targets: Vec<(String, PathBuf)> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).with_context(|| format!("read dir {}", dir.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("read entry in {}", dir.display()))?;
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
            if ext != "ptscn" && ext != "py" {
                continue;
            }
            targets.push((res_path_for(root, &fs_path)?, fs_path));
        }
    }
    targets.sort_by(|a, b| a.0.cmp(&b.0));

    let mut touched = Vec::new();
    for (res_path, fs_path) in &targets {
        let text = match std::fs::read_to_string(fs_path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let mut current = text;
        let mut total = 0usize;
        for (old, new) in renames {
            let (next_text, count) = replace_res_ref(&current, old, new);
            current = next_text;
            total += count;
        }
        if total > 0 {
            std::fs::write(fs_path, current)
                .with_context(|| format!("write {}", fs_path.display()))?;
            touched.push(res_path.clone());
        }
    }
    touched.sort();
    Ok(touched)
}

/// Scan, re-run the matching importer over every `changed`/`added` path
/// (one failure never aborts the rest), rewrite refs for `renamed`
/// paths, save, and return the fresh registry plus the report.
pub fn reimport_project(root: &Path) -> Result<(UidRegistry, ReimportReport)> {
    let (reg, scan) = scan_project(root)?;
    let importers = default_importers();
    let mut targets: Vec<&String> = scan.changed.iter().chain(scan.added.iter()).collect();
    targets.sort();

    let mut reimported = Vec::new();
    let mut failed = Vec::new();
    for res_path in targets {
        let rel = res_path.trim_start_matches("res://");
        let fs_path = root.join(rel);
        let ext = fs_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let importer = importers.iter().find(|i| i.match_ext(&ext));
        match importer {
            Some(importer) => match importer.import(fs_path.to_string_lossy().as_ref()) {
                Ok(()) => reimported.push(res_path.clone()),
                Err(_) => failed.push(res_path.clone()),
            },
            None => failed.push(res_path.clone()),
        }
    }
    reimported.sort();
    failed.sort();

    let refs_updated = apply_renames(root, &scan.renamed)?;
    save_registry(root, &reg)?;
    Ok((
        reg,
        ReimportReport {
            reimported,
            failed,
            refs_updated,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    fn test_dir(name: &str) -> PathBuf {
        let seq = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "pite-assets-{name}-{}-{nanos}-{seq}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    fn valid_png(extra: &[u8]) -> Vec<u8> {
        [PNG_SIG.as_slice(), extra].concat()
    }

    fn valid_wav() -> Vec<u8> {
        [
            b"RIFF".as_slice(),
            &[0x24, 0x00, 0x00, 0x00],
            b"WAVE".as_slice(),
            &[0x00, 0x01, 0x02, 0x03],
        ]
        .concat()
    }

    #[test]
    fn rename_keeps_uid() {
        let root = test_dir("rename");
        let old_fs = root.join("a.png");
        std::fs::write(&old_fs, valid_png(b"hello")).unwrap();

        let (reg1, rep1) = scan_project(&root).unwrap();
        assert_eq!(rep1.added, vec!["res://a.png".to_string()]);
        assert!(rep1.renamed.is_empty());
        let uid1 = reg1.get("res://a.png").unwrap().to_string();

        std::fs::rename(&old_fs, root.join("b.png")).unwrap();
        let (reg2, rep2) = scan_project(&root).unwrap();
        assert_eq!(
            rep2.renamed,
            vec![("res://a.png".to_string(), "res://b.png".to_string())]
        );
        assert!(rep2.added.is_empty());
        assert!(rep2.removed.is_empty());
        let uid2 = reg2.get("res://b.png").unwrap().to_string();
        assert_eq!(uid1, uid2);
        assert!(reg2.get("res://a.png").is_none());

        cleanup(&root);
    }

    #[test]
    fn removed_is_reported() {
        let root = test_dir("removed");
        std::fs::write(root.join("gone.png"), valid_png(b"x")).unwrap();
        let (_, rep1) = scan_project(&root).unwrap();
        assert_eq!(rep1.added.len(), 1);

        std::fs::remove_file(root.join("gone.png")).unwrap();
        let (reg2, rep2) = scan_project(&root).unwrap();
        assert_eq!(rep2.removed, vec!["res://gone.png".to_string()]);
        assert!(rep2.added.is_empty());
        assert!(reg2.get("res://gone.png").is_none());

        cleanup(&root);
    }

    #[test]
    fn reimport_runs_on_changed_content() {
        let root = test_dir("reimport");
        std::fs::write(root.join("sprite.png"), valid_png(b"v1")).unwrap();
        let (reg1, _) = scan_project(&root).unwrap();
        let hash1 = reg1
            .iter()
            .find(|e| e.path == "res://sprite.png")
            .unwrap()
            .hash
            .clone();

        std::fs::write(root.join("sprite.png"), valid_png(b"v2-longer")).unwrap();
        let (reg2, report) = reimport_project(&root).unwrap();
        assert_eq!(report.reimported, vec!["res://sprite.png".to_string()]);
        assert!(report.failed.is_empty());
        let hash2 = reg2
            .iter()
            .find(|e| e.path == "res://sprite.png")
            .unwrap()
            .hash
            .clone();
        assert_ne!(hash1, hash2);
        assert_eq!(hash2, fnv1a_hex(&valid_png(b"v2-longer")));

        cleanup(&root);
    }

    #[test]
    fn importers_reject_bad_magic() {
        let root = test_dir("magic");
        let bad_png = root.join("bad.png");
        std::fs::write(&bad_png, b"definitely not a png").unwrap();
        assert!(PngImporter
            .import(bad_png.to_string_lossy().as_ref())
            .is_err());
        assert!(PngImporter
            .import(root.join("missing.png").to_string_lossy().as_ref())
            .is_err());
        let good_png = root.join("good.png");
        std::fs::write(&good_png, valid_png(b"payload")).unwrap();
        assert!(PngImporter
            .import(good_png.to_string_lossy().as_ref())
            .is_ok());

        let bad_wav = root.join("bad.wav");
        std::fs::write(&bad_wav, b"RIFFxxNOTWAVE!").unwrap();
        assert!(WavImporter
            .import(bad_wav.to_string_lossy().as_ref())
            .is_err());
        let short_wav = root.join("short.wav");
        std::fs::write(&short_wav, b"RIFF").unwrap();
        assert!(WavImporter
            .import(short_wav.to_string_lossy().as_ref())
            .is_err());
        let good_wav = root.join("good.wav");
        std::fs::write(&good_wav, valid_wav()).unwrap();
        assert!(WavImporter
            .import(good_wav.to_string_lossy().as_ref())
            .is_ok());

        assert!(WavImporter.match_ext("wav"));
        assert!(WavImporter.match_ext("WAV"));
        assert!(!WavImporter.match_ext("png"));

        // Ogg tests
        let bad_ogg = root.join("bad.ogg");
        std::fs::write(&bad_ogg, b"NOT_OGG").unwrap();
        assert!(OggImporter
            .import(bad_ogg.to_string_lossy().as_ref())
            .is_err());
        let good_ogg = root.join("good.ogg");
        std::fs::write(&good_ogg, b"OggS\x00\x02extra").unwrap();
        assert!(OggImporter
            .import(good_ogg.to_string_lossy().as_ref())
            .is_ok());
        assert!(OggImporter.match_ext("ogg"));
        assert!(OggImporter.match_ext("OGG"));

        // MP3 tests
        let bad_mp3 = root.join("bad.mp3");
        std::fs::write(&bad_mp3, b"NOT_MP3").unwrap();
        assert!(Mp3Importer
            .import(bad_mp3.to_string_lossy().as_ref())
            .is_err());
        let good_mp3_id3 = root.join("good_id3.mp3");
        std::fs::write(&good_mp3_id3, b"ID3\x04\x00").unwrap();
        assert!(Mp3Importer
            .import(good_mp3_id3.to_string_lossy().as_ref())
            .is_ok());
        let good_mp3_sync = root.join("good_sync.mp3");
        std::fs::write(&good_mp3_sync, &[0xFF, 0xFB, 0x90, 0x64]).unwrap();
        assert!(Mp3Importer
            .import(good_mp3_sync.to_string_lossy().as_ref())
            .is_ok());
        assert!(Mp3Importer.match_ext("mp3"));
        assert!(Mp3Importer.match_ext("MP3"));

        // FLAC tests
        let bad_flac = root.join("bad.flac");
        std::fs::write(&bad_flac, b"NOT_FLAC").unwrap();
        assert!(FlacImporter
            .import(bad_flac.to_string_lossy().as_ref())
            .is_err());
        let good_flac = root.join("good.flac");
        std::fs::write(&good_flac, b"fLaC\x00\x00").unwrap();
        assert!(FlacImporter
            .import(good_flac.to_string_lossy().as_ref())
            .is_ok());
        assert!(FlacImporter.match_ext("flac"));
        assert!(FlacImporter.match_ext("FLAC"));

        let importers = default_importers();
        assert_eq!(importers.len(), 5);

        cleanup(&root);
    }

    #[test]
    fn replace_res_ref_only_touches_quoted() {
        let text = "texture = \"res://assets/player.png\"\n\
            pite.play('res://assets/player.png')\n\
            bare res://assets/player.png stays\n";
        let (out, count) =
            replace_res_ref(text, "res://assets/player.png", "res://assets/hero.png");
        assert_eq!(count, 2);
        assert!(out.contains("\"res://assets/hero.png\""));
        assert!(out.contains("'res://assets/hero.png'"));
        assert!(out.contains("bare res://assets/player.png stays"));
        assert!(!out.contains("\"res://assets/player.png\""));
        assert!(!out.contains("'res://assets/player.png'"));

        let (same, zero) = replace_res_ref("nothing here res://x", "res://x", "res://y");
        assert_eq!(zero, 0);
        assert_eq!(same, "nothing here res://x");
    }

    #[test]
    fn manifest_round_trip_is_sorted() {
        let root = test_dir("manifest");
        std::fs::write(root.join("b.png"), valid_png(b"b")).unwrap();
        std::fs::write(root.join("a.png"), valid_png(b"a")).unwrap();
        std::fs::write(root.join("hit.wav"), valid_wav()).unwrap();
        let (reg1, _) = scan_project(&root).unwrap();
        assert_eq!(reg1.len(), 3);
        assert!(!reg1.is_empty());

        let text = std::fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
        let pos_a = text.find("res://a.png").expect("a.png in manifest");
        let pos_b = text.find("res://b.png").expect("b.png in manifest");
        let pos_w = text.find("res://hit.wav").expect("hit.wav in manifest");
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_w);

        let reg2 = load_registry(&root).unwrap();
        assert_eq!(reg2.len(), 3);
        for entry in reg1.iter() {
            let again = reg2
                .iter()
                .find(|e| e.path == entry.path)
                .expect("entry survives round-trip");
            assert_eq!(again.uid, entry.uid);
            assert_eq!(again.hash, entry.hash);
            assert_eq!(again.size, entry.size);
        }

        let empty_root = test_dir("manifest-empty");
        let empty = load_registry(&empty_root).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        cleanup(&root);
        cleanup(&empty_root);
    }

    #[test]
    fn scan_skips_hidden_and_target() {
        let root = test_dir("skips");
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join(".hidden").join("s.png"), valid_png(b"s")).unwrap();
        std::fs::write(root.join("target").join("t.png"), valid_png(b"t")).unwrap();
        std::fs::write(root.join(".dot.png"), valid_png(b"d")).unwrap();
        std::fs::write(root.join("keep.png"), valid_png(b"k")).unwrap();
        std::fs::write(root.join("notes.txt"), b"not an asset").unwrap();

        let files = scan_files(&root).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.res_path.as_str()).collect();
        assert_eq!(paths, vec!["res://keep.png"]);

        cleanup(&root);
    }

    #[test]
    fn apply_renames_rewrites_scene_and_script() {
        let root = test_dir("renames");
        std::fs::write(root.join("old.png"), valid_png(b"o")).unwrap();
        let (_, rep1) = scan_project(&root).unwrap();
        assert_eq!(rep1.added, vec!["res://old.png".to_string()]);

        std::fs::rename(root.join("old.png"), root.join("new.png")).unwrap();
        std::fs::write(root.join("main.ptscn"), "texture = \"res://old.png\"\n").unwrap();
        std::fs::write(
            root.join("play.py"),
            "pite.play('res://old.png')\nbare res://old.png\n",
        )
        .unwrap();

        let (reg, report) = reimport_project(&root).unwrap();
        assert_eq!(
            report.refs_updated,
            vec!["res://main.ptscn".to_string(), "res://play.py".to_string()]
        );
        let scene = std::fs::read_to_string(root.join("main.ptscn")).unwrap();
        assert!(scene.contains("\"res://new.png\""));
        let script = std::fs::read_to_string(root.join("play.py")).unwrap();
        assert!(script.contains("'res://new.png'"));
        assert!(script.contains("bare res://old.png"));
        assert_eq!(
            reg.path_for_uid(reg.get("res://new.png").unwrap()),
            Some("res://new.png")
        );

        cleanup(&root);
    }
}
