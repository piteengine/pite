// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed errors for `pite-assets`.

use std::path::PathBuf;

use thiserror::Error;

/// Errors returned when importing, scanning, reconciling, or validating assets.
#[derive(Debug, Error)]
pub enum AssetError {
    #[error("cannot {op} {}: {source}", path.display())]
    Io {
        op: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("asset not found: {0}")]
    NotFound(PathBuf),

    #[error("not a {format} asset: {}", path.display())]
    InvalidFormat { format: &'static str, path: PathBuf },

    #[error("path {} outside root {}", path.display(), root.display())]
    OutsideRoot { path: PathBuf, root: PathBuf },

    #[error("cannot parse {}: {source}", path.display())]
    TomlParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("cannot serialize asset manifest: {0}")]
    TomlEncode(#[from] toml::ser::Error),

    #[error(transparent)]
    ResPath(#[from] pite_core::ResPathError),

    #[error("{0}")]
    SpriteSource(&'static str),

    #[error("{context}: {message}")]
    Atlas { context: String, message: String },
}

/// Convenience result alias for `pite-assets`.
pub type Result<T> = std::result::Result<T, AssetError>;
