// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sprite-source rules (`texture` vs `atlas` + `frame`) and project-wide
//! atlas reference checks for `pite check`.
//!
//! Frame tables are parsed by `pite-render` (it owns UVs and batching); this
//! crate reuses that parser so a file can never be valid for the renderer and
//! invalid for the checker.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use pite_render::atlas::{self, Atlas};

/// Where a `Sprite2D` gets its pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpriteSource {
    /// No texture at all: the node draws nothing (today's behavior).
    Empty,
    Texture(String),
    Atlas { atlas: String, frame: String },
}

/// `texture` and `atlas` are mutually exclusive, and `frame` is meaningless
/// without an atlas. Every violation is an error, never a silent preference.
pub fn resolve_sprite_source(
    texture: Option<&str>,
    atlas_ref: Option<&str>,
    frame: Option<&str>,
) -> Result<SpriteSource> {
    match (texture, atlas_ref, frame) {
        (Some(_), Some(_), _) => bail!("`texture` and `atlas` are mutually exclusive"),
        (None, Some(_), None) => bail!("`atlas` requires a `frame`"),
        (Some(_), None, Some(_)) => bail!("`frame` requires an `atlas`"),
        (None, None, Some(_)) => bail!("`frame` requires an `atlas`"),
        (Some(texture), None, None) => Ok(SpriteSource::Texture(texture.to_string())),
        (None, Some(atlas_ref), Some(frame)) => Ok(SpriteSource::Atlas {
            atlas: atlas_ref.to_string(),
            frame: frame.to_string(),
        }),
        (None, None, None) => Ok(SpriteSource::Empty),
    }
}

/// One scene node drawing a named frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasUse {
    pub scene: String,
    pub node: String,
    pub atlas: String,
    pub frame: String,
}

fn atlas_path(root: &Path, atlas_ref: &str) -> PathBuf {
    match atlas_ref.strip_prefix("res://") {
        Some(rel) => root.join(rel),
        None => root.join(atlas_ref),
    }
}

/// Check every atlas reference in the project: unknown frames are errors
/// (naming the alternatives), frames nothing points at are warnings.
/// Output is sorted so repeated runs report identically.
pub fn atlas_report(root: &Path, uses: &[AtlasUse]) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut by_atlas: BTreeMap<&str, Vec<&AtlasUse>> = BTreeMap::new();
    for use_ in uses {
        by_atlas.entry(use_.atlas.as_str()).or_default().push(use_);
    }
    for (atlas_ref, uses) in by_atlas {
        let path = atlas_path(root, atlas_ref);
        let loaded = atlas::load_atlas(&path);
        let mut referenced: Vec<&str> = Vec::new();
        for use_ in &uses {
            match &loaded {
                Ok(atlas) => match atlas.frame(&use_.frame) {
                    Ok(_) => referenced.push(use_.frame.as_str()),
                    Err(e) => errors.push(format!(
                        "scene {}: node {:?} {e}",
                        use_.scene, use_.node
                    )),
                },
                Err(e) => errors.push(format!("scene {}: node {:?} {e:#}", use_.scene, use_.node)),
            }
        }
        if let Ok(atlas) = &loaded {
            for name in atlas.names() {
                if !referenced.contains(&name) {
                    warnings.push(format!(
                        "atlas {atlas_ref:?}: frame {name:?} is never referenced"
                    ));
                }
            }
        }
    }
    errors.sort();
    errors.dedup();
    warnings.sort();
    warnings.dedup();
    (errors, warnings)
}

/// The sheet an atlas sidecar describes, for callers that resolve paths.
pub fn sheet_path(root: &Path, atlas_ref: &str) -> PathBuf {
    atlas::sheet_for_sidecar(&atlas_path(root, atlas_ref))
}

/// Load and validate one atlas sidecar, naming the file on failure.
pub fn load_sidecar(root: &Path, atlas_ref: &str) -> Result<Atlas> {
    let path = atlas_path(root, atlas_ref);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read atlas {}", path.display()))?;
    atlas::parse_atlas(&text).with_context(|| format!("cannot parse atlas {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn project(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join(format!("pite-atlas-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(
            dir.join("assets").join("sheet.atlas.json"),
            r#"{"texture":"sheet.png","size":[64,32],
                "frames":{"player":{"x":0,"y":0,"w":32,"h":32},
                          "enemy":{"x":32,"y":0,"w":32,"h":32}}}"#,
        )
        .unwrap();
        dir
    }

    #[test]
    fn texture_and_atlas_are_mutually_exclusive() {
        let err = resolve_sprite_source(Some("res://a.png"), Some("res://sheet.atlas.json"), Some("player"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("mutually exclusive"), "got: {err}");
    }

    #[test]
    fn frame_requires_an_atlas_and_atlas_requires_a_frame() {
        let err = resolve_sprite_source(None, Some("res://sheet.atlas.json"), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("requires a `frame`"), "got: {err}");
        let err = resolve_sprite_source(None, None, Some("player"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("requires an `atlas`"), "got: {err}");
    }

    #[test]
    fn plain_texture_still_resolves() {
        assert_eq!(
            resolve_sprite_source(Some("res://a.png"), None, None).unwrap(),
            SpriteSource::Texture("res://a.png".to_string())
        );
        assert_eq!(resolve_sprite_source(None, None, None).unwrap(), SpriteSource::Empty);
        assert_eq!(
            resolve_sprite_source(None, Some("res://sheet.atlas.json"), Some("player")).unwrap(),
            SpriteSource::Atlas {
                atlas: "res://sheet.atlas.json".to_string(),
                frame: "player".to_string()
            }
        );
    }

    #[test]
    fn report_flags_unknown_frames_and_unreferenced_ones() {
        let dir = project("report");
        let uses = vec![
            AtlasUse {
                scene: "res://scenes/main.pitescene".to_string(),
                node: "player".to_string(),
                atlas: "res://assets/sheet.atlas.json".to_string(),
                frame: "player".to_string(),
            },
            AtlasUse {
                scene: "res://scenes/main.pitescene".to_string(),
                node: "ghost".to_string(),
                atlas: "res://assets/sheet.atlas.json".to_string(),
                frame: "ghost".to_string(),
            },
        ];
        let (errors, warnings) = atlas_report(&dir, &uses);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        assert!(errors[0].contains("no frame \"ghost\""), "got: {errors:?}");
        assert!(errors[0].contains("player"), "should list alternatives: {errors:?}");
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert!(warnings[0].contains("\"enemy\""), "got: {warnings:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_sidecar_is_an_error_not_a_skip() {
        let dir = project("missing");
        let uses = vec![AtlasUse {
            scene: "res://scenes/main.pitescene".to_string(),
            node: "player".to_string(),
            atlas: "res://assets/nope.atlas.json".to_string(),
            frame: "player".to_string(),
        }];
        let (errors, warnings) = atlas_report(&dir, &uses);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        assert!(errors[0].contains("cannot read atlas"), "got: {errors:?}");
        assert!(warnings.is_empty(), "got: {warnings:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sheet_path_follows_the_sidecar() {
        let dir = project("sheet");
        assert_eq!(
            sheet_path(&dir, "res://assets/sheet.atlas.json"),
            dir.join("assets").join("sheet.png")
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}