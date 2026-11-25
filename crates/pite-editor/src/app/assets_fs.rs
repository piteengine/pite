// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewFileKind {
    Empty,
    Script,
    Scene,
}

#[derive(Debug, Clone)]
pub enum AssetModal {
    NewFolder {
        parent_rel: String,
        name: String,
        error: Option<String>,
    },
    NewFile {
        parent_rel: String,
        name: String,
        kind: NewFileKind,
        error: Option<String>,
    },
    NewAtlas {
        parent_rel: String,
        png_rel: String,
        atlas_name: String,
        use_grid: bool,
        tile_w: u32,
        tile_h: u32,
        assign_node: Option<String>,
        error: Option<String>,
    },
    Rename {
        rel_path: String,
        new_name: String,
        error: Option<String>,
    },
    ConfirmDelete {
        rel_path: String,
        is_dir: bool,
    },
}

#[derive(Debug, Clone)]
pub struct AssetClipboardEntry {
    pub rel_path: String,
    pub is_cut: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDndPayload {
    pub rel_path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub rel_path: String,
    #[allow(dead_code)]
    pub is_dir: bool,
}

fn split_stem_ext(name: &str) -> (&str, String) {
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    (stem, ext)
}

/// List children of `rel_dir` inside `root`. Returns `(folders, files)`
/// both sorted alphabetically by lowercase name. Empty folders are preserved.
pub fn list_asset_dir(root: &Path, rel_dir: &str) -> (Vec<DirEntry>, Vec<DirEntry>) {
    let fs_dir = if rel_dir.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel_dir.replace('/', std::path::MAIN_SEPARATOR_STR))
    };
    let Ok(entries) = std::fs::read_dir(&fs_dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let child_rel = if rel_dir.is_empty() {
            name.clone()
        } else {
            format!("{rel_dir}/{name}")
        };
        if is_dir {
            if name == "target"
                || name == "dist"
                || name == ".git"
                || name == pite_scene::cache::CACHE_DIR_NAME
            {
                continue;
            }
            folders.push(DirEntry {
                name,
                rel_path: child_rel,
                is_dir: true,
            });
        } else {
            files.push(DirEntry {
                name,
                rel_path: child_rel,
                is_dir: false,
            });
        }
    }
    folders.sort_by_key(|a| a.name.to_lowercase());
    files.sort_by_key(|a| a.name.to_lowercase());
    (folders, files)
}

/// Resolves a non-colliding path in `parent`: if `name` already exists,
/// suffixes it with `stem (1).ext`, `stem (2).ext`, etc.
pub fn resolve_unique_path(parent: &Path, name: &str) -> PathBuf {
    let target = parent.join(name);
    if !target.exists() {
        return target;
    }
    let (stem, ext) = split_stem_ext(name);
    let mut count = 1;
    loop {
        let candidate = format!("{stem} ({count}){ext}");
        let cand_path = parent.join(&candidate);
        if !cand_path.exists() {
            return cand_path;
        }
        count += 1;
    }
}

/// Resolves a duplicate name for `path`: `stem (copy).ext`, `stem (copy 2).ext`.
pub fn resolve_duplicate_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let (stem, ext) = split_stem_ext(name);
    let mut count = 1;
    loop {
        let candidate = if count == 1 {
            format!("{stem} (copy){ext}")
        } else {
            format!("{stem} (copy {count}){ext}")
        };
        let cand_path = parent.join(&candidate);
        if !cand_path.exists() {
            return cand_path;
        }
        count += 1;
    }
}

/// Recursively copies a file or directory tree.
pub fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let sub_src = entry.path();
            let sub_dst = dst.join(entry.file_name());
            copy_recursive(&sub_src, &sub_dst)?;
        }
        Ok(())
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        Ok(())
    }
}

/// Recursively collects rename pairs for all files within a directory tree.
fn collect_dir_renames(
    root: &Path,
    dir: &Path,
    old_base_rel: &str,
    new_base_rel: &str,
    renames: &mut Vec<(String, String)>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_dir_renames(root, &path, old_base_rel, new_base_rel, renames)?;
        } else {
            let rel = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if let Some(sub) = rel.strip_prefix(old_base_rel) {
                let new_rel = format!("{new_base_rel}{sub}");
                renames.push((format!("res://{rel}"), format!("res://{new_rel}")));
            }
        }
    }
    Ok(())
}

/// Moves an asset on disk and returns the list of `(old_res, new_res)` reference pairs.
/// Enforces directory cycle and path-sandboxing checks.
pub fn move_asset_rel(
    root: &Path,
    src_rel: &str,
    dst_dir_rel: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let src_norm = pite_core::parse_rel_subpath(src_rel)?;
    let src_fs = pite_core::resolve_res(
        root,
        &format!("res://{}", src_norm.to_string_lossy().replace('\\', "/")),
    )?;
    if !src_fs.exists() {
        anyhow::bail!("Source path does not exist: {src_rel}");
    }
    let filename = src_norm
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid source path: {src_rel}"))?;

    let dst_dir_norm = if dst_dir_rel.is_empty() {
        String::new()
    } else {
        pite_core::parse_rel_subpath(dst_dir_rel)?
            .to_string_lossy()
            .replace('\\', "/")
    };
    let src_rel_clean = src_norm.to_string_lossy().replace('\\', "/");

    // Cycle check: cannot move a folder into itself or a descendant
    if src_fs.is_dir() {
        let prefix = format!("{src_rel_clean}/");
        if dst_dir_norm == src_rel_clean || dst_dir_norm.starts_with(&prefix) {
            anyhow::bail!("Cannot move folder into itself or its subdirectory");
        }
    }

    let dst_rel = if dst_dir_norm.is_empty() {
        filename.to_string()
    } else {
        format!("{dst_dir_norm}/{filename}")
    };
    if dst_rel == src_rel_clean {
        return Ok(Vec::new());
    }

    let dst_fs = pite_core::resolve_res(root, &format!("res://{dst_rel}"))?;
    if dst_fs.exists() {
        anyhow::bail!("Destination already exists: {dst_rel}");
    }

    let mut renames = Vec::new();
    if src_fs.is_dir() {
        collect_dir_renames(root, &src_fs, &src_rel_clean, &dst_rel, &mut renames)?;
    } else {
        renames.push((format!("res://{src_rel_clean}"), format!("res://{dst_rel}")));
    }

    std::fs::rename(&src_fs, &dst_fs)?;
    Ok(renames)
}

/// Renames an asset on disk and returns the list of `(old_res, new_res)` reference pairs.
pub fn rename_asset_rel(
    root: &Path,
    old_rel: &str,
    new_name: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    pite_core::validate_entry_name(new_name)?;
    let old_norm = pite_core::parse_rel_subpath(old_rel)?;
    let old_rel_clean = old_norm.to_string_lossy().replace('\\', "/");
    let old_fs = pite_core::resolve_res(root, &format!("res://{old_rel_clean}"))?;
    if !old_fs.exists() {
        anyhow::bail!("Path does not exist: {old_rel}");
    }
    let parent = old_norm.parent().unwrap_or(Path::new(""));
    let new_rel = if parent.as_os_str().is_empty() {
        new_name.to_string()
    } else {
        format!("{}/{new_name}", parent.to_string_lossy().replace('\\', "/"))
    };
    if new_rel == old_rel_clean {
        return Ok(Vec::new());
    }
    let new_fs = pite_core::resolve_res(root, &format!("res://{new_rel}"))?;
    if new_fs.exists() {
        anyhow::bail!("An item named '{new_name}' already exists");
    }

    let mut renames = Vec::new();
    if old_fs.is_dir() {
        collect_dir_renames(root, &old_fs, &old_rel_clean, &new_rel, &mut renames)?;
    } else {
        renames.push((format!("res://{old_rel_clean}"), format!("res://{new_rel}")));
    }

    std::fs::rename(&old_fs, &new_fs)?;
    Ok(renames)
}

/// Opens the operating system's file manager at the specified path.
pub fn show_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg("/select,")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(Path::new("."))
        };
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}
