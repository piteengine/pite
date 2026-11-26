// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed errors for `pite-project`.

use std::path::PathBuf;

use thiserror::Error;

/// Errors produced when loading, saving, or scaffolding a Pite project.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot parse {path}: {source}")]
    TomlParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("cannot encode manifest as TOML: {0}")]
    TomlEncode(#[from] toml::ser::Error),

    #[error("cannot extract project template archive: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("{} already exists", path.display())]
    AlreadyExists { path: PathBuf },

    #[error("unknown template {name:?} (supported: \"minimal-2d\", \"gem-collector\", or omit for blank)")]
    UnknownTemplate { name: String },

    #[error(transparent)]
    ResPath(#[from] pite_core::ResPathError),

    #[error("{0}")]
    Validation(String),
}

/// Convenience result alias for `pite-project`.
pub type Result<T> = std::result::Result<T, ProjectError>;
