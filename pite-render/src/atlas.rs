// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sprite-sheet atlases: frame metadata from a JSON sidecar plus the UV math
//! the sprite pipeline needs.
//!
//! The frame table lives here because the renderer owns UVs and batches by
//! texture key — one sheet means one texture, so every frame of it lands in
//! the same draw call. `pite-assets` reuses [`parse_atlas`] to validate the
//! same files for `check`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Sidecar suffix appended to a sheet's file stem.
pub const SIDECAR_EXT: &str = "atlas.json";

/// One frame inside the sheet, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasFrame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl AtlasFrame {
    /// Normalized UV rect `(u0, v0, u1, v1)` inside a sheet of `sheet` pixels.
    /// `v` grows downward, matching the sprite quad's top-left origin.
    pub fn uv(&self, sheet: (u32, u32)) -> [f32; 4] {
        let (sw, sh) = (sheet.0 as f32, sheet.1 as f32);
        [
            self.x as f32 / sw,
            self.y as f32 / sh,
            (self.x + self.w) as f32 / sw,
            (self.y + self.h) as f32 / sh,
        ]
    }

    fn fits(&self, sheet: (u32, u32)) -> bool {
        self.w > 0
            && self.h > 0
            && self.x.saturating_add(self.w) <= sheet.0
            && self.y.saturating_add(self.h) <= sheet.1
    }
}

/// A parsed sheet: its texture file plus named frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atlas {
    pub texture: String,
    pub size: (u32, u32),
    pub frames: BTreeMap<String, AtlasFrame>,
}

impl Atlas {
    /// Frame lookup that names the alternatives instead of failing silently.
    pub fn frame(&self, name: &str) -> Result<AtlasFrame> {
        self.frames.get(name).copied().ok_or_else(|| {
            anyhow::anyhow!(
                "atlas {:?} has no frame {name:?} (available: {})",
                self.texture,
                if self.frames.is_empty() {
                    "none".to_string()
                } else {
                    self.names().join(", ")
                }
            )
        })
    }

    pub fn names(&self) -> Vec<&str> {
        self.frames.keys().map(String::as_str).collect()
    }
}

#[derive(Debug, Deserialize)]
struct RawAtlas {
    texture: String,
    size: [u32; 2],
    frames: BTreeMap<String, RawFrame>,
}

#[derive(Debug, Deserialize)]
struct RawFrame {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl From<RawFrame> for AtlasFrame {
    fn from(raw: RawFrame) -> Self {
        AtlasFrame {
            x: raw.x,
            y: raw.y,
            w: raw.w,
            h: raw.h,
        }
    }
}

/// Parse atlas JSON. Empty sheets, zero sizes, and frames outside the sheet
/// are errors — a bad atlas must never reach the GPU as silent garbage.
pub fn parse_atlas(text: &str) -> Result<Atlas> {
    let raw: RawAtlas =
        serde_json::from_str(text).context("atlas is not valid JSON (texture/size/frames)")?;
    if raw.texture.trim().is_empty() {
        anyhow::bail!("atlas has an empty `texture`");
    }
    if raw.size[0] == 0 || raw.size[1] == 0 {
        anyhow::bail!("atlas size {:?} must be positive", raw.size);
    }
    if raw.frames.is_empty() {
        anyhow::bail!("atlas {:?} declares no frames", raw.texture);
    }
    let sheet = (raw.size[0], raw.size[1]);
    let mut frames = BTreeMap::new();
    for (name, frame) in raw.frames {
        let frame = AtlasFrame::from(frame);
        if !frame.fits(sheet) {
            anyhow::bail!(
                "atlas frame {name:?} ({},{} {}x{}) does not fit the {sheet:?} sheet",
                frame.x,
                frame.y,
                frame.w,
                frame.h
            );
        }
        frames.insert(name, frame);
    }
    Ok(Atlas {
        texture: raw.texture,
        size: sheet,
        frames,
    })
}

pub fn load_atlas(path: &Path) -> Result<Atlas> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read atlas {}", path.display()))?;
    parse_atlas(&text).with_context(|| format!("cannot parse atlas {}", path.display()))
}

/// Sidecar path for a sheet: `sheet.png` -> `sheet.atlas.json` beside it.
pub fn sidecar_for(sheet: &Path) -> PathBuf {
    let stem = sheet
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("sheet");
    sheet
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!("{stem}.{SIDECAR_EXT}"))
}

/// The sheet a sidecar describes: `sheet.atlas.json` -> `sheet.png`.
pub fn sheet_for_sidecar(sidecar: &Path) -> PathBuf {
    let name = sidecar
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let stem = name
        .strip_suffix(&format!(".{SIDECAR_EXT}"))
        .unwrap_or(name);
    sidecar
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!("{stem}.png"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: &str = r#"{
        "texture": "sheet.png",
        "size": [64, 32],
        "frames": {
            "player": { "x": 0, "y": 0, "w": 32, "h": 32 },
            "enemy": { "x": 32, "y": 0, "w": 32, "h": 32 },
            "coin": { "x": 0, "y": 16, "w": 16, "h": 16 }
        }
    }"#;

    #[test]
    fn parses_named_frames() {
        let atlas = parse_atlas(SHEET).unwrap();
        assert_eq!(atlas.texture, "sheet.png");
        assert_eq!(atlas.size, (64, 32));
        assert_eq!(atlas.frames.len(), 3);
        assert_eq!(
            atlas.frame("player").unwrap(),
            AtlasFrame {
                x: 0,
                y: 0,
                w: 32,
                h: 32
            }
        );
        assert_eq!(
            atlas.frame("enemy").unwrap(),
            AtlasFrame {
                x: 32,
                y: 0,
                w: 32,
                h: 32
            }
        );
        assert_eq!(atlas.names(), vec!["coin", "enemy", "player"]);
    }

    #[test]
    fn missing_frame_names_the_alternatives() {
        let atlas = parse_atlas(SHEET).unwrap();
        let err = atlas.frame("ghost").unwrap_err().to_string();
        assert!(err.contains("no frame \"ghost\""), "got: {err}");
        assert!(err.contains("player"), "got: {err}");
    }

    #[test]
    fn uv_math_maps_pixels_to_normalized_rects() {
        let atlas = parse_atlas(SHEET).unwrap();
        let player = atlas.frame("player").unwrap().uv(atlas.size);
        assert_eq!(player, [0.0, 0.0, 0.5, 1.0]);
        let enemy = atlas.frame("enemy").unwrap().uv(atlas.size);
        assert_eq!(enemy, [0.5, 0.0, 1.0, 1.0]);
        let coin = atlas.frame("coin").unwrap().uv(atlas.size);
        assert_eq!(coin, [0.0, 0.5, 0.25, 1.0]);
    }

    #[test]
    fn bad_atlases_fail_loudly() {
        assert!(parse_atlas("not json").is_err());
        assert!(parse_atlas(r#"{"texture":"","size":[1,1],"frames":{}}"#).is_err());
        assert!(parse_atlas(r#"{"texture":"a.png","size":[0,8],"frames":{}}"#).is_err());
        assert!(parse_atlas(r#"{"texture":"a.png","size":[8,8],"frames":{}}"#).is_err());
        let out_of_bounds = r#"{"texture":"a.png","size":[8,8],
            "frames":{"big":{"x":4,"y":4,"w":8,"h":8}}}"#;
        let err = parse_atlas(out_of_bounds).unwrap_err().to_string();
        assert!(err.contains("does not fit"), "got: {err}");
    }

    #[test]
    fn sidecar_and_sheet_paths_round_trip() {
        let sheet = Path::new("/game/assets/sheet.png");
        let sidecar = sidecar_for(sheet);
        assert_eq!(sidecar, PathBuf::from("/game/assets/sheet.atlas.json"));
        assert_eq!(sheet_for_sidecar(&sidecar), sheet);
    }
}
