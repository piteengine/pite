// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reference rewriting, legacy atlas migration, and project reimport pipeline.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AssetError, Result};
use crate::importers::default_importers;
use crate::{is_skipped_dir, res_path_for, save_registry, scan_project, AssetRegistry};

/// Rewrite result for one `(old, new)` rename.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenameRewrite {
    pub old: String,
    pub new: String,
    /// Total quote-wrapped references replaced across all files.
    pub references: usize,
    /// `res://`-style paths of the files that were rewritten.
    pub files: Vec<String>,
}

/// Outcome of [`reimport_project`]: importer results plus ref rewrites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReimportReport {
    pub reimported: Vec<String>,
    pub failed: Vec<String>,
    pub refs_updated: Vec<String>,
    /// Per-rename reference counts, in the order the renames were applied.
    #[serde(default)]
    pub rewrites: Vec<RenameRewrite>,
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
    let mut touched: Vec<String> = apply_renames_detailed(root, renames)?
        .into_iter()
        .flat_map(|rewrite| rewrite.files)
        .collect();
    touched.sort();
    touched.dedup();
    Ok(touched)
}

/// [`apply_renames`], keeping per-rename reference counts and file lists.
pub fn apply_renames_detailed(
    root: &Path,
    renames: &[(String, String)],
) -> Result<Vec<RenameRewrite>> {
    let mut rewrites: Vec<RenameRewrite> = renames
        .iter()
        .map(|(old, new)| RenameRewrite {
            old: old.clone(),
            new: new.clone(),
            references: 0,
            files: Vec::new(),
        })
        .collect();
    if renames.is_empty() {
        return Ok(rewrites);
    }
    let mut targets: Vec<(String, PathBuf)> = Vec::new();
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
            if ext != "ptscn" && ext != "py" && ext != "ptmap" {
                continue;
            }
            targets.push((res_path_for(root, &fs_path)?, fs_path));
        }
    }
    targets.sort_by(|a, b| a.0.cmp(&b.0));

    for (res_path, fs_path) in &targets {
        let text = match std::fs::read_to_string(fs_path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let mut current = text;
        let mut counts = Vec::with_capacity(renames.len());
        let mut total = 0usize;
        for (old, new) in renames {
            let (next_text, count) = replace_res_ref(&current, old, new);
            current = next_text;
            counts.push(count);
            total += count;
        }
        if total == 0 {
            continue;
        }
        std::fs::write(fs_path, current).map_err(|source| AssetError::Io {
            op: "write",
            path: fs_path.clone(),
            source,
        })?;
        for (rewrite, count) in rewrites.iter_mut().zip(counts) {
            if count > 0 {
                rewrite.references += count;
                rewrite.files.push(res_path.clone());
            }
        }
    }
    for rewrite in &mut rewrites {
        rewrite.files.sort();
    }
    Ok(rewrites)
}

/// Finds legacy `.atlas.json` sidecar files under `root`, returning their relative paths.
pub fn find_legacy_atlases(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if !is_skipped_dir(&name) {
                    stack.push(path);
                }
                continue;
            }
            if name.ends_with(".atlas.json") {
                if let Ok(rel) = path.strip_prefix(root) {
                    let rel_str = rel
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/");
                    found.push(rel_str);
                }
            }
        }
    }
    found.sort();
    found
}

/// Migrates legacy `.atlas.json` sidecar files in the project to `.ptatlas` TOML.
/// Converts the content to `.ptatlas` format, removes the old file, and returns
/// rename pairs `("res://path/sheet.atlas.json", "res://path/sheet.ptatlas")`.
pub fn migrate_legacy_atlases(root: &Path) -> Result<Vec<(String, String)>> {
    let mut renames = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if !is_skipped_dir(&name) {
                    stack.push(path);
                }
                continue;
            }
            if let Some(stem) = name.strip_suffix(".atlas.json") {
                let text = std::fs::read_to_string(&path).map_err(|source| AssetError::Io {
                    op: "read legacy atlas",
                    path: path.clone(),
                    source,
                })?;
                let toml_text =
                    pite_render::atlas::migrate_legacy_json_to_toml(&text).map_err(|source| {
                        AssetError::Atlas {
                            context: format!("migrate legacy atlas {}", path.display()),
                            message: format!("{source:#}"),
                        }
                    })?;
                let ptatlas_path = path.with_file_name(format!("{stem}.ptatlas"));
                std::fs::write(&ptatlas_path, toml_text).map_err(|source| AssetError::Io {
                    op: "write migrated atlas",
                    path: ptatlas_path.clone(),
                    source,
                })?;
                let _ = std::fs::remove_file(&path);
                let old_res = res_path_for(root, &path)?;
                let new_res = res_path_for(root, &ptatlas_path)?;
                renames.push((old_res, new_res));
            }
        }
    }
    renames.sort();
    Ok(renames)
}

/// Scan, re-run the matching importer over every `changed`/`added` path
/// (one failure never aborts the rest), rewrite refs for `renamed`
/// paths, save, and return the fresh registry plus the report.
pub fn reimport_project(root: &Path) -> Result<(AssetRegistry, ReimportReport)> {
    let atlas_renames = migrate_legacy_atlases(root)?;
    let (reg, scan) = scan_project(root)?;
    let importers = default_importers();
    let mut targets: Vec<&String> = scan.changed.iter().chain(scan.added.iter()).collect();
    targets.sort();

    let mut reimported = Vec::new();
    let mut failed = Vec::new();
    for res_path in targets {
        let Ok(fs_path) = pite_core::resolve_res(root, res_path) else {
            failed.push(res_path.clone());
            continue;
        };
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

    let mut all_renames = scan.renamed.clone();
    all_renames.extend(atlas_renames);
    let rewrites = apply_renames_detailed(root, &all_renames)?;
    let refs_updated: Vec<String> = {
        let mut files: Vec<String> = rewrites
            .iter()
            .flat_map(|rewrite| rewrite.files.iter().cloned())
            .collect();
        files.sort();
        files.dedup();
        files
    };
    save_registry(root, &reg)?;
    Ok((
        reg,
        ReimportReport {
            reimported,
            failed,
            refs_updated,
            rewrites,
        },
    ))
}

/// Extract every `pite.play("...")` / `pite.play('...')` literal from
/// Python source.
///
/// A byte scan, not a parser: it strips line comments (`#` to EOL) and
/// then looks for `play("` or `play('` preceded by a non-identifier
/// character, so `x = display("...")` is correctly *not* matched. The
/// close quote must be on the same line. Anything dynamic — f-strings,
/// concatenation, variables — is out of scope, so a miss here is safe
/// noise, not a bug: both callers use this to verify that literal
/// references exist, not to prove the set is complete.
pub fn played_assets(text: &str) -> Vec<&str> {
    // body — exact same code as before
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        for marker in ["play(\"", "play('"] {
            let mut rest = line;
            while let Some(pos) = rest.find(marker) {
                let before = &rest[..pos];
                if before.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    rest = &rest[pos + marker.len()..];
                    continue;
                }
                let quote = marker.as_bytes()[marker.len() - 1] as char;
                rest = &rest[pos + marker.len()..];
                if let Some(end) = rest.find(quote) {
                    out.push(&rest[..end]);
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
            }
        }
    }
    out
}
