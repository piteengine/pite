// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed errors for `pite-scene`.

use std::path::PathBuf;

use thiserror::Error;

/// Errors returned when parsing, caching, instantiating, or saving scenes and tilemaps.
#[derive(Debug, Error)]
pub enum SceneError {
    #[error("cannot {op} scene file {}: {source}", path.display())]
    Io {
        op: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot parse scene {}: invalid UTF-8 ({source})", path.display())]
    InvalidUtf8 {
        path: PathBuf,
        #[source]
        source: std::str::Utf8Error,
    },

    #[error("cannot parse scene {}: {source}", path.display())]
    ParseFile {
        path: PathBuf,
        #[source]
        source: Box<SceneError>,
    },

    #[error("scene is not valid TOML: {0}")]
    InvalidToml(#[source] toml::de::Error),

    #[error("scene does not match schema: {0}")]
    InvalidSchema(#[source] toml::de::Error),

    #[error("cannot serialize scene: {0}")]
    Encode(#[from] toml::ser::Error),

    #[error("unsupported scene format_version {found} (engine supports {supported})")]
    UnsupportedFormatVersion { found: u32, supported: u32 },

    #[error(transparent)]
    Core(#[from] pite_core::CoreError),

    #[error("{context}: {source}")]
    TreeInsert {
        context: String,
        #[source]
        source: pite_core::CoreError,
    },

    #[error(transparent)]
    ResPath(#[from] pite_core::ResPathError),

    #[error("override {key:?} must be \"node_id.prop\" in {}", path.display())]
    InvalidOverrideKey { key: String, path: PathBuf },

    #[error("override target {target:?} not found in {}", path.display())]
    UnknownOverrideTarget { target: String, path: PathBuf },

    #[error("scene cache is corrupt ({reason})")]
    CacheCorrupt { reason: String },

    #[error("scene cache has cache version {found} (engine uses {supported})")]
    CacheVersionMismatch { found: u32, supported: u32 },

    #[error("scene cache has scene format_version {found} (engine uses {supported})")]
    CacheFormatMismatch { found: u32, supported: u32 },

    #[error("invalid tilemap TOML: {0}")]
    InvalidTilemapToml(#[source] toml::de::Error),

    #[error("cannot parse tilemap {}: {source}", path.display())]
    ParseTilemap {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("cannot {op} tilemap {}: {source}", path.display())]
    TilemapIo {
        op: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Convenience result alias for `pite-scene`.
pub type Result<T> = std::result::Result<T, SceneError>;
