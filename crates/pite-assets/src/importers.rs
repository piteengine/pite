// SPDX-License-Identifier: MIT OR Apache-2.0
//! Built-in asset format validators and importer trait.

use std::path::PathBuf;

use crate::error::{AssetError, Result};

/// 8-byte PNG signature every valid `.png` file must start with.
pub(crate) const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Trait implemented by asset importers to validate and ingest specific file types.
pub trait Importer: Send + Sync {
    /// Returns true if this importer handles files with the given file extension (case-insensitive).
    fn match_ext(&self, ext: &str) -> bool;

    /// Validates and imports the asset at the specified filesystem path.
    fn import(&self, path: &str) -> Result<()>;
}

fn read_asset_bytes(path: &str) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            AssetError::NotFound(PathBuf::from(path))
        } else {
            AssetError::Io {
                op: "read",
                path: PathBuf::from(path),
                source,
            }
        }
    })
}

/// Asset importer for PNG raster images.
pub struct PngImporter;

impl Importer for PngImporter {
    fn match_ext(&self, ext: &str) -> bool {
        ext.eq_ignore_ascii_case("png")
    }

    fn import(&self, path: &str) -> Result<()> {
        let bytes = read_asset_bytes(path)?;
        if bytes.len() < PNG_SIG.len() || bytes[..PNG_SIG.len()] != PNG_SIG {
            return Err(AssetError::InvalidFormat {
                format: "PNG",
                path: PathBuf::from(path),
            });
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
        let bytes = read_asset_bytes(path)?;
        if bytes.len() < 12 || bytes[0..4] != *b"RIFF" || bytes[8..12] != *b"WAVE" {
            return Err(AssetError::InvalidFormat {
                format: "WAV",
                path: PathBuf::from(path),
            });
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
        let bytes = read_asset_bytes(path)?;
        if bytes.len() < 4 || bytes[0..4] != *b"OggS" {
            return Err(AssetError::InvalidFormat {
                format: "OGG",
                path: PathBuf::from(path),
            });
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
        let bytes = read_asset_bytes(path)?;
        let is_id3 = bytes.len() >= 3 && &bytes[0..3] == b"ID3";
        let is_sync = bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0;
        if !is_id3 && !is_sync {
            return Err(AssetError::InvalidFormat {
                format: "MP3",
                path: PathBuf::from(path),
            });
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
        let bytes = read_asset_bytes(path)?;
        if bytes.len() < 4 || bytes[0..4] != *b"fLaC" {
            return Err(AssetError::InvalidFormat {
                format: "FLAC",
                path: PathBuf::from(path),
            });
        }
        Ok(())
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
