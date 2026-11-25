// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sandboxed `res://` and project-relative resource path resolution.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Errors returned when validating or resolving `res://` or project-relative paths.
#[derive(Debug, Error)]
pub enum ResPathError {
    #[error("path `{path}` must use a `res://` URI")]
    MissingScheme { path: String },

    #[error("`res://` path must not be empty")]
    EmptyPath,

    #[error("path `{path}` uses `res://` but no project root is known")]
    MissingProjectRoot { path: String },

    #[error("path `{path}` contains a null byte")]
    NullByte { path: String },

    #[error("path `{path}` must not start with a root or prefix")]
    AbsoluteOrPrefix { path: String },

    #[error("path `{path}` attempts directory traversal outside the project root")]
    Traversal { path: String },

    #[error("path `{path}` must be a single file or directory name")]
    InvalidName { path: String },

    #[error("path `{path}` escapes project root `{root}` via symlink `{resolved}`")]
    SymlinkEscape {
        path: String,
        root: PathBuf,
        resolved: PathBuf,
    },

    #[error("failed to inspect path `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Validates and normalizes the relative portion of a `res://` URI.
///
/// Rejects empty paths, null bytes, leading slashes/backslashes, Windows
/// drive prefixes, and any `..` traversal segments.
pub fn parse_res_rel(res_path: &str) -> std::result::Result<PathBuf, ResPathError> {
    if res_path.contains('\0') {
        return Err(ResPathError::NullByte {
            path: res_path.to_string(),
        });
    }
    let rel = res_path
        .strip_prefix("res://")
        .ok_or_else(|| ResPathError::MissingScheme {
            path: res_path.to_string(),
        })?;
    if rel.is_empty() {
        return Err(ResPathError::EmptyPath);
    }
    if rel.starts_with('/') || rel.starts_with('\\') {
        return Err(ResPathError::AbsoluteOrPrefix {
            path: res_path.to_string(),
        });
    }

    let mut normalized = PathBuf::new();
    for segment in rel.split(['/', '\\']) {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(ResPathError::Traversal {
                path: res_path.to_string(),
            });
        }
        for comp in Path::new(segment).components() {
            match comp {
                Component::Normal(part) => normalized.push(part),
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(ResPathError::Traversal {
                        path: res_path.to_string(),
                    });
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(ResPathError::AbsoluteOrPrefix {
                        path: res_path.to_string(),
                    });
                }
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(ResPathError::EmptyPath);
    }
    Ok(normalized)
}

/// Validates a relative path (without `res://` scheme) inside a project root.
pub fn parse_rel_subpath(rel_path: &str) -> std::result::Result<PathBuf, ResPathError> {
    if rel_path.contains('\0') {
        return Err(ResPathError::NullByte {
            path: rel_path.to_string(),
        });
    }
    if rel_path.is_empty() {
        return Err(ResPathError::EmptyPath);
    }
    parse_res_rel(&format!("res://{rel_path}"))
}

/// Validates a single path entry name (no separators, `.`, `..`, or null bytes).
pub fn validate_entry_name(name: &str) -> std::result::Result<(), ResPathError> {
    if name.contains('\0') {
        return Err(ResPathError::NullByte {
            path: name.to_string(),
        });
    }
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(ResPathError::InvalidName {
            path: name.to_string(),
        });
    }
    let mut comps = Path::new(name).components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(ResPathError::InvalidName {
            path: name.to_string(),
        }),
    }
}

fn lexically_normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::Normal(part) => out.push(part),
        }
    }
    Some(out)
}

fn canonicalize_deepest_existing(
    candidate: &Path,
) -> std::result::Result<(PathBuf, PathBuf), std::io::Error> {
    let mut tail = PathBuf::new();
    let mut cur = candidate;
    loop {
        if cur.exists() {
            let canon = std::fs::canonicalize(cur)?;
            return Ok((canon, tail));
        }
        if let Ok(meta) = std::fs::symlink_metadata(cur) {
            if meta.file_type().is_symlink() {
                // Dangling symlink: resolve target relative to its parent to check containment.
                let target = std::fs::read_link(cur)?;
                let parent = cur.parent().unwrap_or_else(|| Path::new("."));
                let joined = if target.is_absolute() {
                    target
                } else {
                    parent.join(target)
                };
                let normalized = lexically_normalize(&joined).unwrap_or(joined);
                return Ok((normalized, tail));
            }
        }
        let Some(name) = cur.file_name() else {
            return Ok((cur.to_path_buf(), tail));
        };
        let mut next_tail = PathBuf::from(name);
        if !tail.as_os_str().is_empty() {
            next_tail.push(&tail);
        }
        tail = next_tail;
        match cur.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => cur = parent,
            _ => return Ok((PathBuf::from("."), tail)),
        }
    }
}

fn ensure_contained(
    project_dir: &Path,
    candidate: &Path,
    original: &str,
) -> std::result::Result<(), ResPathError> {
    if !project_dir.exists() {
        return Ok(());
    }
    let root_canon = std::fs::canonicalize(project_dir).map_err(|source| ResPathError::Io {
        path: project_dir.to_path_buf(),
        source,
    })?;
    let (base_canon, tail) =
        canonicalize_deepest_existing(candidate).map_err(|source| ResPathError::Io {
            path: candidate.to_path_buf(),
            source,
        })?;
    let resolved = if tail.as_os_str().is_empty() {
        base_canon
    } else {
        base_canon.join(tail)
    };
    if !resolved.starts_with(&root_canon) {
        return Err(ResPathError::SymlinkEscape {
            path: original.to_string(),
            root: root_canon,
            resolved,
        });
    }
    Ok(())
}

/// Resolves a `res://` URI strictly inside `project_dir`, rejecting traversal
/// (`..`), absolute/prefixed paths, null bytes, and escaping symlinks.
pub fn resolve_res(
    project_dir: &Path,
    res_path: &str,
) -> std::result::Result<PathBuf, ResPathError> {
    let rel = parse_res_rel(res_path)?;
    let candidate = project_dir.join(rel);
    ensure_contained(project_dir, &candidate, res_path)?;
    Ok(candidate)
}

/// Resolves a resource or scene reference inside a project (or standalone file when
/// `project_dir` is `None`).
///
/// - `res://...` requires `project_dir` and is sandboxed via [`resolve_res`].
/// - Relative non-`res://` paths inside a project are resolved from `base_dir` and
///   verified not to escape `project_dir`.
/// - Absolute non-`res://` paths are rejected whenever `project_dir` is `Some`.
pub fn resolve_project_ref(
    project_dir: Option<&Path>,
    base_dir: &Path,
    reference: &str,
) -> std::result::Result<PathBuf, ResPathError> {
    if reference.contains('\0') {
        return Err(ResPathError::NullByte {
            path: reference.to_string(),
        });
    }
    if reference.is_empty() {
        return Err(ResPathError::EmptyPath);
    }
    if reference.starts_with("res://") {
        let root = project_dir.ok_or_else(|| ResPathError::MissingProjectRoot {
            path: reference.to_string(),
        })?;
        return resolve_res(root, reference);
    }

    let p = Path::new(reference);
    if let Some(root) = project_dir {
        if p.is_absolute() || reference.starts_with('/') || reference.starts_with('\\') {
            return Err(ResPathError::AbsoluteOrPrefix {
                path: reference.to_string(),
            });
        }
        let joined = base_dir.join(p);
        let Some(norm_candidate) = lexically_normalize(&joined) else {
            return Err(ResPathError::Traversal {
                path: reference.to_string(),
            });
        };
        let norm_root = lexically_normalize(root).unwrap_or_else(|| root.to_path_buf());
        if !norm_candidate.starts_with(&norm_root) {
            return Err(ResPathError::Traversal {
                path: reference.to_string(),
            });
        }
        ensure_contained(root, &norm_candidate, reference)?;
        Ok(norm_candidate)
    } else if p.is_absolute() {
        Ok(p.to_path_buf())
    } else {
        Ok(base_dir.join(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmpdir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("pite-path-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_empty_missing_and_traversal_res_paths() {
        let dir = tmpdir("reject");
        assert!(matches!(
            resolve_res(&dir, "scenes/main.ptscn"),
            Err(ResPathError::MissingScheme { .. })
        ));
        assert!(matches!(
            resolve_res(&dir, "res://"),
            Err(ResPathError::EmptyPath)
        ));
        assert!(matches!(
            resolve_res(&dir, "res:///etc/passwd"),
            Err(ResPathError::AbsoluteOrPrefix { .. })
        ));
        assert!(matches!(
            resolve_res(&dir, "res://\\etc\\passwd"),
            Err(ResPathError::AbsoluteOrPrefix { .. })
        ));
        assert!(matches!(
            resolve_res(&dir, "res://../secret.txt"),
            Err(ResPathError::Traversal { .. })
        ));
        assert!(matches!(
            resolve_res(&dir, "res://assets/../../secret.txt"),
            Err(ResPathError::Traversal { .. })
        ));
        assert!(matches!(
            resolve_res(&dir, "res://assets\\..\\..\\secret.txt"),
            Err(ResPathError::Traversal { .. })
        ));
        assert!(matches!(
            resolve_res(&dir, "res://a\0b"),
            Err(ResPathError::NullByte { .. })
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolves_valid_res_and_in_project_relative_refs() {
        let dir = tmpdir("valid");
        let scenes_nested = dir.join("scenes").join("nested");
        std::fs::create_dir_all(&scenes_nested).unwrap();

        let p = resolve_res(&dir, "res://scenes/main.ptscn").unwrap();
        assert_eq!(p, dir.join("scenes").join("main.ptscn"));

        // Relative `../sub.ptscn` from `scenes/nested` stays inside `dir`.
        let rel = resolve_project_ref(Some(&dir), &scenes_nested, "../sub.ptscn").unwrap();
        assert_eq!(rel, dir.join("scenes").join("sub.ptscn"));

        // Escaping `dir` with `../../..` fails.
        assert!(matches!(
            resolve_project_ref(Some(&dir), &scenes_nested, "../../../etc/passwd"),
            Err(ResPathError::Traversal { .. })
        ));
        // Absolute non-res path inside a project fails.
        assert!(matches!(
            resolve_project_ref(Some(&dir), &scenes_nested, "/etc/passwd"),
            Err(ResPathError::AbsoluteOrPrefix { .. })
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_project_root() {
        let root = tmpdir("sym-root");
        let outside = tmpdir("sym-out");
        std::fs::write(outside.join("secret.txt"), b"secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        let err = resolve_res(&root, "res://link/secret.txt").unwrap_err();
        assert!(
            matches!(err, ResPathError::SymlinkEscape { .. }),
            "expected SymlinkEscape, got {err:?}"
        );

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn validates_single_entry_names() {
        assert!(validate_entry_name("hero.png").is_ok());
        assert!(validate_entry_name("").is_err());
        assert!(validate_entry_name(".").is_err());
        assert!(validate_entry_name("..").is_err());
        assert!(validate_entry_name("../evil").is_err());
        assert!(validate_entry_name("a/b").is_err());
        assert!(validate_entry_name("a\\b").is_err());
    }
}
