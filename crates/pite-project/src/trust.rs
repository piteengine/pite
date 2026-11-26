// SPDX-License-Identifier: MIT OR Apache-2.0
//! Per-user workspace trust persistence (`trusted_workspaces.toml`).
//!
//! Stores trusted project/workspace root paths outside the repository in the
//! user's Pite configuration directory (`$XDG_CONFIG_HOME/pite` or
//! `$HOME/.config/pite`) so untrusted projects cannot self-approve execution.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ProjectError, Result};

/// Filename of the per-user trusted workspace registry.
pub const TRUST_STORE_FILE: &str = "trusted_workspaces.toml";

/// Serialized list of trusted workspace directory paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedWorkspaces {
    #[serde(default)]
    pub trusted: BTreeSet<String>,
}

#[cfg(windows)]
fn fallback_config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".config")))
}

#[cfg(not(windows))]
fn fallback_config_dir() -> Option<PathBuf> {
    None
}

/// Resolves the per-user Pite configuration directory (`<config>/pite`).
///
/// Honors `PITE_CONFIG_DIR` when non-empty, then `XDG_CONFIG_HOME`, then
/// `HOME/.config` (and `APPDATA` / `USERPROFILE/.config` on Windows).
pub fn config_dir() -> Option<PathBuf> {
    if let Some(override_dir) = std::env::var_os("PITE_CONFIG_DIR").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(override_dir));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(fallback_config_dir)?;
    Some(base.join("pite"))
}

/// Returns the default path to `trusted_workspaces.toml`, if a user config
/// directory can be determined from the environment.
pub fn trust_store_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(TRUST_STORE_FILE))
}

/// Determines the workspace root for trust checks: `project_dir` when inside a
/// `pite.toml` project, otherwise the scene's parent directory.
pub fn workspace_root(scene: &Path, project_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = project_dir {
        return dir.to_path_buf();
    }
    match scene.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Canonicalizes `workspace` for stable matching across symlinks and relative
/// segments, falling back to the raw path if canonicalization fails.
pub fn normalize_workspace_path(workspace: &Path) -> String {
    let resolved = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    resolved.to_string_lossy().into_owned()
}

/// Loads the trusted workspace registry from `store_path`.
///
/// Missing files yield an empty registry.
///
/// # Errors
/// Returns [`ProjectError::Io`] if reading fails for a reason other than
/// `NotFound`, or [`ProjectError::TomlParse`] if the file contains invalid TOML.
pub fn load_trusted_workspaces_from(store_path: &Path) -> Result<TrustedWorkspaces> {
    let text = match std::fs::read_to_string(store_path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TrustedWorkspaces::default());
        }
        Err(source) => {
            return Err(ProjectError::Io {
                path: store_path.to_path_buf(),
                source,
            });
        }
    };
    toml::from_str(&text).map_err(|source| ProjectError::TomlParse {
        path: store_path.to_path_buf(),
        source,
    })
}

/// Persists `store` as deterministic pretty-printed TOML at `store_path`.
///
/// # Errors
/// Returns [`ProjectError::Io`] on directory creation or write failure, or
/// [`ProjectError::TomlEncode`] if serialization fails.
pub fn save_trusted_workspaces_to(store_path: &Path, store: &TrustedWorkspaces) -> Result<()> {
    if let Some(parent) = store_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ProjectError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let mut text = toml::to_string_pretty(store)?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    std::fs::write(store_path, text).map_err(|source| ProjectError::Io {
        path: store_path.to_path_buf(),
        source,
    })
}

/// Returns `true` if `workspace` is recorded in `store_path`.
///
/// Fails closed (`false`) if `store_path` is missing or unreadable/corrupt.
pub fn is_workspace_trusted_in(store_path: &Path, workspace: &Path) -> bool {
    let Ok(store) = load_trusted_workspaces_from(store_path) else {
        return false;
    };
    let key = normalize_workspace_path(workspace);
    store.trusted.contains(&key)
}

/// Records `workspace` as trusted in `store_path` (idempotent).
///
/// # Errors
/// Returns an error if loading, creating parent directories, or writing fails.
pub fn trust_workspace_in(store_path: &Path, workspace: &Path) -> Result<()> {
    let mut store = load_trusted_workspaces_from(store_path)?;
    let key = normalize_workspace_path(workspace);
    if store.trusted.insert(key) {
        save_trusted_workspaces_to(store_path, &store)?;
    }
    Ok(())
}

/// Removes `workspace` from `store_path`, returning `true` if it was present.
///
/// # Errors
/// Returns an error if reading or writing `store_path` fails.
pub fn untrust_workspace_in(store_path: &Path, workspace: &Path) -> Result<bool> {
    let mut store = load_trusted_workspaces_from(store_path)?;
    let key = normalize_workspace_path(workspace);
    let removed = store.trusted.remove(&key);
    if removed {
        save_trusted_workspaces_to(store_path, &store)?;
    }
    Ok(removed)
}

/// Returns `true` if `workspace` is recorded in the user's default trust store.
pub fn is_workspace_trusted(workspace: &Path) -> bool {
    trust_store_path()
        .map(|store| is_workspace_trusted_in(&store, workspace))
        .unwrap_or(false)
}

/// Records `workspace` as trusted in the user's default trust store.
///
/// # Errors
/// Returns [`ProjectError::Io`] if no user config directory is available or
/// persisting the trust store fails.
pub fn trust_workspace(workspace: &Path) -> Result<()> {
    let store = trust_store_path().ok_or_else(|| ProjectError::Io {
        path: PathBuf::from(TRUST_STORE_FILE),
        source: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot resolve user config directory for workspace trust store",
        ),
    })?;
    trust_workspace_in(&store, workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-trust-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn trust_round_trip_and_untrust() {
        let root = tmpdir("roundtrip");
        let store = root.join("config").join(TRUST_STORE_FILE);
        let project = root.join("my_game");
        std::fs::create_dir_all(&project).unwrap();

        assert!(!is_workspace_trusted_in(&store, &project));

        trust_workspace_in(&store, &project).unwrap();
        assert!(is_workspace_trusted_in(&store, &project));
        // Relative dot segment resolves to the same canonical workspace.
        assert!(is_workspace_trusted_in(&store, &project.join(".")));

        // Idempotent second trust call keeps single entry.
        trust_workspace_in(&store, &project).unwrap();
        let loaded = load_trusted_workspaces_from(&store).unwrap();
        assert_eq!(loaded.trusted.len(), 1);

        assert!(untrust_workspace_in(&store, &project).unwrap());
        assert!(!is_workspace_trusted_in(&store, &project));
        assert!(!untrust_workspace_in(&store, &project).unwrap());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn corrupt_trust_store_fails_closed_and_errors_loudly() {
        let root = tmpdir("corrupt");
        let store = root.join(TRUST_STORE_FILE);
        let project = root.join("proj");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(&store, "trusted = 123\n").unwrap();

        assert!(!is_workspace_trusted_in(&store, &project));
        let err = load_trusted_workspaces_from(&store).unwrap_err();
        assert!(err.to_string().contains("cannot parse"), "got {err:#}");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn workspace_root_prefers_project_dir_else_scene_parent() {
        let proj = Path::new("/tmp/game");
        let scene = Path::new("/tmp/game/scenes/main.ptscn");
        assert_eq!(workspace_root(scene, Some(proj)), proj);
        assert_eq!(workspace_root(scene, None), Path::new("/tmp/game/scenes"));
        assert_eq!(
            workspace_root(Path::new("main.ptscn"), None),
            Path::new(".")
        );
    }
}
