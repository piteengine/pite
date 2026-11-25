// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed errors for `pite-export`.

use std::path::PathBuf;

use thiserror::Error;

/// Errors produced when packaging and exporting standalone Pite distributions.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("unsupported export platform {platform:?} (supported: linux, windows, macos)")]
    UnsupportedPlatform { platform: String },

    #[error("{} exists; remove it or pass --out <dir>", path.display())]
    OutputExists { path: PathBuf },

    #[error(
        "no pinned Python build for {platform}; pass --no-bundle-python to require \
         system Python 3.12 instead"
    )]
    NoBundledPython { platform: String },

    #[error(
        "export needs system Python 3.12 on PATH (or PITE_PYTHON); \
         the target machine must have it too. Pass --skip-python-check \
         for cross-machine builds, or drop --no-bundle-python to ship a \
         pinned interpreter."
    )]
    MissingSystemPython,

    #[error("engine binary not found: {}", path.display())]
    BinaryNotFound { path: PathBuf },

    #[error("export references missing file: {}", path.display())]
    MissingReferencedFile { path: PathBuf },

    #[error("export cannot reach outside the project: {}", path.display())]
    OutsideProject { path: PathBuf },

    #[error("export cannot resolve {kind} {reference:?}")]
    UnresolvedReference {
        kind: &'static str,
        reference: String,
    },

    #[error("{0}")]
    Message(String),

    #[error(transparent)]
    Project(#[from] pite_project::ProjectError),

    #[error(transparent)]
    Scene(#[from] pite_scene::SceneError),

    #[error(transparent)]
    Asset(#[from] pite_assets::AssetError),

    #[error(transparent)]
    ResPath(#[from] pite_core::ResPathError),

    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),

    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

/// Convenience result alias for `pite-export`.
pub type Result<T> = std::result::Result<T, ExportError>;

pub(crate) fn io_err(context: impl Into<String>, source: std::io::Error) -> ExportError {
    ExportError::Io {
        context: context.into(),
        source,
    }
}
