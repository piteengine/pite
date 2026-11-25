// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sprite-sheet atlases: frame metadata from a TOML sidecar (`.ptatlas`) plus
//! the UV math the sprite pipeline needs.
//!
//! The frame table lives here because the renderer owns UVs and batches by
//! texture key — one sheet means one texture, so every frame of it lands in
//! the same draw call. `pite-assets` reuses [`parse_atlas`] to validate the
//! same files for `check`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Current format version for `.ptatlas` files.
pub const FORMAT_VERSION: u32 = 1;

/// Sidecar suffix appended to a sheet's file stem.
pub const SIDECAR_EXT: &str = "ptatlas";

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

/// One animation sequence inside the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasAnimation {
    pub frames: Vec<String>,
    pub fps: f32,
    pub looping: bool,
}

/// A parsed sheet: its texture file plus named frames and animations.
#[derive(Debug, Clone, PartialEq)]
pub struct Atlas {
    pub texture: String,
    pub size: (u32, u32),
    pub frames: BTreeMap<String, AtlasFrame>,
    pub animations: BTreeMap<String, AtlasAnimation>,
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

    /// Animation lookup that names the alternatives instead of failing silently.
    pub fn animation(&self, name: &str) -> Result<&AtlasAnimation> {
        self.animations.get(name).ok_or_else(|| {
            anyhow::anyhow!(
                "atlas {:?} has no animation {name:?} (available: {})",
                self.texture,
                if self.animations.is_empty() {
                    "none".to_string()
                } else {
                    self.animation_names().join(", ")
                }
            )
        })
    }

    pub fn animation_names(&self) -> Vec<&str> {
        self.animations.keys().map(String::as_str).collect()
    }
}

fn default_format_version() -> u32 {
    FORMAT_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAtlas {
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    pub texture: String,
    pub size: [u32; 2],
    pub frames: BTreeMap<String, RawFrame>,
    #[serde(default)]
    pub animations: BTreeMap<String, RawAnimation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAnimation {
    pub frames: Vec<String>,
    #[serde(default = "default_fps")]
    pub fps: f32,
    #[serde(default = "default_looping", rename = "loop", alias = "looping")]
    pub looping: bool,
}

fn default_fps() -> f32 {
    10.0
}

fn default_looping() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFrame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
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

/// Parse atlas TOML. Empty sheets, zero sizes, and frames outside the sheet
/// are errors — a bad atlas must never reach the GPU as silent garbage.
pub fn parse_atlas(text: &str) -> Result<Atlas> {
    let raw: RawAtlas =
        toml::from_str(text).context("atlas is not valid TOML (texture/size/frames)")?;
    if raw.format_version != FORMAT_VERSION {
        anyhow::bail!(
            "unsupported atlas format_version {} (engine supports {FORMAT_VERSION})",
            raw.format_version
        );
    }
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
    let mut animations = BTreeMap::new();
    let available_frames = if frames.is_empty() {
        "none".to_string()
    } else {
        frames
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    for (name, anim) in raw.animations {
        if anim.frames.is_empty() {
            anyhow::bail!("atlas animation {name:?} declares no frames");
        }
        if anim.fps <= 0.0 || !anim.fps.is_finite() {
            anyhow::bail!(
                "atlas animation {name:?} fps ({}) must be positive",
                anim.fps
            );
        }
        for frame_name in &anim.frames {
            if !frames.contains_key(frame_name) {
                anyhow::bail!(
                    "atlas animation {name:?} references nonexistent frame {frame_name:?} (available: {available_frames})"
                );
            }
        }
        animations.insert(
            name,
            AtlasAnimation {
                frames: anim.frames,
                fps: anim.fps,
                looping: anim.looping,
            },
        );
    }
    Ok(Atlas {
        texture: raw.texture,
        size: sheet,
        frames,
        animations,
    })
}

pub fn load_atlas(path: &Path) -> Result<Atlas> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read atlas {}", path.display()))?;
    parse_atlas(&text).with_context(|| format!("cannot parse atlas {}", path.display()))
}

/// Migrates a legacy `.atlas.json` string to `.ptatlas` TOML format.
pub fn migrate_legacy_json_to_toml(json_text: &str) -> Result<String> {
    let mut raw: RawAtlas =
        serde_json::from_str(json_text).context("atlas is not valid JSON (texture/size/frames)")?;
    raw.format_version = FORMAT_VERSION;
    if raw.texture.trim().is_empty() {
        anyhow::bail!("atlas has an empty `texture`");
    }
    if raw.size[0] == 0 || raw.size[1] == 0 {
        anyhow::bail!("atlas size {:?} must be positive", raw.size);
    }
    if raw.frames.is_empty() {
        anyhow::bail!("atlas {:?} declares no frames", raw.texture);
    }
    toml::to_string_pretty(&raw).context("failed to serialize migrated atlas to TOML")
}

/// Sidecar path for a sheet: `sheet.png` -> `sheet.ptatlas` beside it.
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

/// The sheet a sidecar describes: `sheet.ptatlas` -> `sheet.png`.
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

/// Serialize an [`Atlas`] into canonical pretty TOML after validating every frame and animation.
pub fn serialize_atlas(atlas: &Atlas) -> Result<String> {
    let raw = RawAtlas {
        format_version: FORMAT_VERSION,
        texture: atlas.texture.clone(),
        size: [atlas.size.0, atlas.size.1],
        frames: atlas
            .frames
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    RawFrame {
                        x: v.x,
                        y: v.y,
                        w: v.w,
                        h: v.h,
                    },
                )
            })
            .collect(),
        animations: atlas
            .animations
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    RawAnimation {
                        frames: v.frames.clone(),
                        fps: v.fps,
                        looping: v.looping,
                    },
                )
            })
            .collect(),
    };
    let text = toml::to_string_pretty(&raw).context("failed to serialize atlas to TOML")?;
    parse_atlas(&text)?;
    Ok(text)
}

/// Validate and write `atlas` to `path` as `.ptatlas` TOML.
pub fn save_atlas(path: &Path, atlas: &Atlas) -> Result<()> {
    let text = serialize_atlas(atlas)?;
    std::fs::write(path, text).with_context(|| format!("cannot write atlas {}", path.display()))
}

/// Read image dimensions `(width, height)` from a sheet PNG without decoding full pixel buffers.
pub fn sheet_dimensions(path: &Path) -> Result<(u32, u32)> {
    image::image_dimensions(path)
        .with_context(|| format!("cannot read image dimensions of {}", path.display()))
}

/// Decode a sheet image into `(width, height, rgba8)` for editor canvas preview and slicing.
pub fn load_sheet_rgba(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let img = image::open(path)
        .with_context(|| format!("cannot load sheet image {}", path.display()))?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Ok((w, h, img.into_raw()))
}

/// Parameters for cutting a uniform grid of frames from a sprite sheet.
#[derive(Debug, Clone)]
pub struct GridSliceSpec {
    pub sheet_size: (u32, u32),
    pub tile_w: u32,
    pub tile_h: u32,
    pub offset_x: u32,
    pub offset_y: u32,
    pub spacing_x: u32,
    pub spacing_y: u32,
    pub prefix: String,
    pub skip_transparent: bool,
}

fn cell_has_opaque_pixel(rgba: &[u8], sheet_w: u32, rect: AtlasFrame) -> bool {
    let sw = sheet_w as usize;
    for py in rect.y..rect.y + rect.h {
        let row_start = (py as usize) * sw;
        for px in rect.x..rect.x + rect.w {
            let alpha_idx = (row_start + px as usize) * 4 + 3;
            if rgba.get(alpha_idx).copied().unwrap_or(0) > 0 {
                return true;
            }
        }
    }
    false
}

/// Generate named [`AtlasFrame`] cells in row-major order matching `spec`.
/// When `spec.skip_transparent` is true and `rgba` is supplied, empty cells
/// whose pixels all have zero alpha are omitted.
pub fn grid_slice_frames(
    spec: &GridSliceSpec,
    rgba: Option<&[u8]>,
) -> Result<Vec<(String, AtlasFrame)>> {
    if spec.tile_w == 0 || spec.tile_h == 0 {
        anyhow::bail!("tile dimensions must be positive");
    }
    let (sw, sh) = spec.sheet_size;
    if sw == 0 || sh == 0 {
        anyhow::bail!("sheet dimensions must be positive");
    }
    let step_x = spec.tile_w.saturating_add(spec.spacing_x);
    let step_y = spec.tile_h.saturating_add(spec.spacing_y);
    let mut rects = Vec::new();
    let mut y = spec.offset_y;
    while y.saturating_add(spec.tile_h) <= sh {
        let mut x = spec.offset_x;
        while x.saturating_add(spec.tile_w) <= sw {
            let rect = AtlasFrame {
                x,
                y,
                w: spec.tile_w,
                h: spec.tile_h,
            };
            let keep = if spec.skip_transparent {
                rgba.is_none_or(|buf| cell_has_opaque_pixel(buf, sw, rect))
            } else {
                true
            };
            if keep {
                rects.push(rect);
            }
            x = x.saturating_add(step_x);
        }
        y = y.saturating_add(step_y);
    }
    let width = if rects.len() >= 10 {
        (rects.len() - 1).to_string().len()
    } else {
        1
    };
    let prefix = if spec.prefix.is_empty() {
        "frame_"
    } else {
        spec.prefix.as_str()
    };
    Ok(rects
        .into_iter()
        .enumerate()
        .map(|(i, rect)| (format!("{prefix}{i:0width$}"), rect))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: &str = r#"
format_version = 1
texture = "sheet.png"
size = [64, 32]

[frames.player]
x = 0
y = 0
w = 32
h = 32

[frames.enemy]
x = 32
y = 0
w = 32
h = 32

[frames.coin]
x = 0
y = 16
w = 16
h = 16
"#;

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
        assert!(parse_atlas("not toml = ").is_err());
        assert!(parse_atlas("texture = \"\"\nsize = [1, 1]\n").is_err());
        assert!(parse_atlas("format_version = 2\ntexture = \"a.png\"\nsize = [8, 8]\n[frames.a]\nx = 0\ny = 0\nw = 4\nh = 4\n").is_err());
        assert!(parse_atlas("texture = \"a.png\"\nsize = [0, 8]\n").is_err());
        assert!(parse_atlas("texture = \"a.png\"\nsize = [8, 8]\n").is_err());
        let out_of_bounds = r#"format_version = 1
texture = "a.png"
size = [8, 8]

[frames.big]
x = 4
y = 4
w = 8
h = 8
"#;
        let err = parse_atlas(out_of_bounds).unwrap_err().to_string();
        assert!(err.contains("does not fit"), "got: {err}");
    }

    #[test]
    fn sidecar_and_sheet_paths_round_trip() {
        let sheet = Path::new("/game/assets/sheet.png");
        let sidecar = sidecar_for(sheet);
        assert_eq!(sidecar, PathBuf::from("/game/assets/sheet.ptatlas"));
        assert_eq!(sheet_for_sidecar(&sidecar), sheet);
    }

    #[test]
    fn parses_animations_and_validates_frames() {
        let toml_src = r#"
format_version = 1
texture = "sheet.png"
size = [64, 32]

[frames.f1]
x = 0
y = 0
w = 32
h = 32

[frames.f2]
x = 32
y = 0
w = 32
h = 32

[animations.walk]
frames = ["f1", "f2"]
fps = 12.0
loop = true

[animations.jump]
frames = ["f1"]
fps = 8.0
loop = false
"#;
        let atlas = parse_atlas(toml_src).unwrap();
        assert_eq!(atlas.animation_names(), vec!["jump", "walk"]);
        let walk = atlas.animation("walk").unwrap();
        assert_eq!(walk.frames, vec!["f1", "f2"]);
        assert_eq!(walk.fps, 12.0);
        assert!(walk.looping);

        let jump = atlas.animation("jump").unwrap();
        assert_eq!(jump.frames, vec!["f1"]);
        assert_eq!(jump.fps, 8.0);
        assert!(!jump.looping);

        let err = atlas.animation("idle").unwrap_err().to_string();
        assert!(err.contains("no animation \"idle\""), "got: {err}");
        assert!(err.contains("walk"), "got: {err}");
    }

    #[test]
    fn invalid_animations_fail_loudly() {
        let missing_frame = r#"
format_version = 1
texture = "sheet.png"
size = [64, 32]

[frames.f1]
x = 0
y = 0
w = 32
h = 32

[animations.walk]
frames = ["ghost"]
fps = 10.0
"#;
        let err = parse_atlas(missing_frame).unwrap_err().to_string();
        assert!(err.contains("nonexistent frame \"ghost\""), "got: {err}");
        assert!(err.contains("f1"), "got: {err}");

        let empty_frames = r#"
format_version = 1
texture = "sheet.png"
size = [64, 32]

[frames.f1]
x = 0
y = 0
w = 32
h = 32

[animations.walk]
frames = []
fps = 10.0
"#;
        let err = parse_atlas(empty_frames).unwrap_err().to_string();
        assert!(err.contains("declares no frames"), "got: {err}");

        let zero_fps = r#"
format_version = 1
texture = "sheet.png"
size = [64, 32]

[frames.f1]
x = 0
y = 0
w = 32
h = 32

[animations.walk]
frames = ["f1"]
fps = 0.0
"#;
        let err = parse_atlas(zero_fps).unwrap_err().to_string();
        assert!(err.contains("must be positive"), "got: {err}");
    }

    #[test]
    fn test_migrate_legacy_json_to_toml() {
        let legacy_json = r#"{
            "texture": "sheet.png",
            "size": [64, 32],
            "frames": {
                "player": { "x": 0, "y": 0, "w": 32, "h": 32 }
            },
            "animations": {
                "walk": {
                    "frames": ["player"],
                    "fps": 10.0,
                    "loop": true
                }
            }
        }"#;
        let migrated_toml = migrate_legacy_json_to_toml(legacy_json).unwrap();
        assert!(migrated_toml.contains("format_version = 1"));
        assert!(migrated_toml.contains("texture = \"sheet.png\""));
        let atlas = parse_atlas(&migrated_toml).unwrap();
        assert_eq!(atlas.texture, "sheet.png");
        assert_eq!(atlas.size, (64, 32));
        assert_eq!(atlas.frames.len(), 1);
        assert_eq!(atlas.animations.len(), 1);
    }

    #[test]
    fn save_atlas_round_trips_and_validates() {
        let dir = std::env::temp_dir().join(format!("pite_atlas_save_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hero.ptatlas");

        let mut atlas = parse_atlas(SHEET).unwrap();
        atlas.animations.insert(
            "run".to_string(),
            AtlasAnimation {
                frames: vec!["player".to_string(), "enemy".to_string()],
                fps: 12.0,
                looping: true,
            },
        );
        save_atlas(&path, &atlas).unwrap();
        let reloaded = load_atlas(&path).unwrap();
        assert_eq!(reloaded, atlas);

        atlas.frames.clear();
        assert!(save_atlas(&path, &atlas).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sheet_dimensions_and_rgba_read_temp_png() {
        let dir = std::env::temp_dir().join(format!("pite_atlas_png_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sheet.png");

        let mut img = image::RgbaImage::new(32, 16);
        img.put_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        img.save(&path).unwrap();

        assert_eq!(sheet_dimensions(&path).unwrap(), (32, 16));
        let (w, h, rgba) = load_sheet_rgba(&path).unwrap();
        assert_eq!((w, h), (32, 16));
        assert_eq!(rgba.len(), 32 * 16 * 4);
        assert_eq!(&rgba[(32 + 1) * 4..(32 + 1) * 4 + 4], &[255, 0, 0, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grid_slice_frames_handles_spacing_and_transparent_skip() {
        let mut rgba = vec![0u8; 34 * 16 * 4];
        // Cell (0,0) and cell (18,0) opaque; cell at x=0 opaque, second cell transparent.
        rgba[(2 * 34 + 2) * 4 + 3] = 255;

        let spec = GridSliceSpec {
            sheet_size: (34, 16),
            tile_w: 16,
            tile_h: 16,
            offset_x: 0,
            offset_y: 0,
            spacing_x: 2,
            spacing_y: 0,
            prefix: "hero_".to_string(),
            skip_transparent: false,
        };
        let all = grid_slice_frames(&spec, Some(&rgba)).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, "hero_0");
        assert_eq!(all[0].1.x, 0);
        assert_eq!(all[1].1.x, 18);

        let non_empty = grid_slice_frames(
            &GridSliceSpec {
                skip_transparent: true,
                ..spec
            },
            Some(&rgba),
        )
        .unwrap();
        assert_eq!(non_empty.len(), 1);
        assert_eq!(non_empty[0].0, "hero_0");
    }
}
