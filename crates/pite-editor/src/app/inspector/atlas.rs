// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui;
use pite_render::atlas::{
    grid_slice_frames, load_atlas, load_sheet_rgba, save_atlas, sheet_dimensions,
    sheet_for_sidecar, Atlas, AtlasFrame, GridSliceSpec,
};

#[derive(Debug, Clone)]
pub(super) enum CanvasDrag {
    DrawNew {
        start: (u32, u32),
        frame_name: String,
    },
    MoveFrame {
        name: String,
        orig: AtlasFrame,
        anchor_px: (i32, i32),
    },
    ResizeFrame {
        name: String,
        orig: AtlasFrame,
    },
    Pan {
        last_pos: egui::Pos2,
    },
}

#[derive(Clone)]
pub struct AtlasEditorState {
    pub path: PathBuf,
    pub res_path: String,
    pub atlas: Atlas,
    pub dirty: bool,
    pub sheet_rgba: Option<(u32, u32, Vec<u8>)>,
    pub sheet_tex: Option<egui::TextureHandle>,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub selected_frame: Option<String>,
    pub frame_rename_buf: String,
    pub frame_filter: String,
    pub range_anchor: Option<String>,
    pub(super) drag_mode: Option<CanvasDrag>,
    pub slice_by_cols: bool,
    pub slice_tile_w: u32,
    pub slice_tile_h: u32,
    pub slice_cols: u32,
    pub slice_rows: u32,
    pub slice_offset_x: u32,
    pub slice_offset_y: u32,
    pub slice_spacing_x: u32,
    pub slice_spacing_y: u32,
    pub slice_prefix: String,
    pub slice_skip_transparent: bool,
    pub slice_replace: bool,
    pub selected_anim: Option<String>,
    pub anim_new_name: String,
    pub anim_rename_buf: String,
    pub anim_playing: bool,
    pub anim_elapsed: f64,
    pub anim_frame_idx: usize,
    pub(super) last_tick: Option<Instant>,
    pub last_error: Option<String>,
}

impl AtlasEditorState {
    /// Load an existing `.ptatlas` sidecar (and decode its sibling `.png` if present).
    pub fn from_path(path: PathBuf, res_path: String) -> anyhow::Result<Self> {
        let mut atlas = load_atlas(&path)?;
        let sheet_path = resolve_sheet_path(&path, &atlas.texture);
        let sheet_rgba = load_sheet_rgba(&sheet_path).ok();
        if let Some((w, h, _)) = &sheet_rgba {
            if *w > 0 && *h > 0 {
                atlas.size = (*w, *h);
            }
        }
        let first_frame = atlas.frames.keys().next().cloned();
        let first_anim = atlas.animations.keys().next().cloned();
        let default_tile_w = (atlas.size.0 / 4).max(16).min(atlas.size.0.max(1));
        let default_tile_h = (atlas.size.1 / 4).max(16).min(atlas.size.1.max(1));
        Ok(Self {
            path,
            res_path,
            atlas,
            dirty: false,
            sheet_rgba,
            sheet_tex: None,
            zoom: 0.0,
            pan: egui::Vec2::ZERO,
            frame_rename_buf: first_frame.clone().unwrap_or_default(),
            selected_frame: first_frame.clone(),
            frame_filter: String::new(),
            range_anchor: first_frame,
            drag_mode: None,
            slice_by_cols: false,
            slice_tile_w: default_tile_w,
            slice_tile_h: default_tile_h,
            slice_cols: 4,
            slice_rows: 1,
            slice_offset_x: 0,
            slice_offset_y: 0,
            slice_spacing_x: 0,
            slice_spacing_y: 0,
            slice_prefix: "frame_".to_string(),
            slice_skip_transparent: true,
            slice_replace: true,
            anim_rename_buf: first_anim.clone().unwrap_or_default(),
            selected_anim: first_anim,
            anim_new_name: String::new(),
            anim_playing: true,
            anim_elapsed: 0.0,
            anim_frame_idx: 0,
            last_tick: None,
            last_error: None,
        })
    }

    /// Create and write a valid `.ptatlas` beside `png_path` using real PNG pixel dimensions.
    pub fn create_from_png(
        png_path: &Path,
        ptatlas_path: &Path,
        res_path: String,
        grid: Option<(u32, u32)>,
    ) -> anyhow::Result<Self> {
        let sheet_rgba = load_sheet_rgba(png_path).ok();
        let (sw, sh) = match &sheet_rgba {
            Some((w, h, _)) => (*w, *h),
            None => sheet_dimensions(png_path)?,
        };
        if sw == 0 || sh == 0 {
            anyhow::bail!("PNG image {} has zero dimensions", png_path.display());
        }
        let tex_name = png_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("sheet.png")
            .to_string();

        let mut frames = BTreeMap::new();
        if let Some((tw, th)) = grid {
            if tw > 0 && th > 0 {
                let spec = GridSliceSpec {
                    sheet_size: (sw, sh),
                    tile_w: tw.min(sw),
                    tile_h: th.min(sh),
                    offset_x: 0,
                    offset_y: 0,
                    spacing_x: 0,
                    spacing_y: 0,
                    prefix: "frame_".to_string(),
                    skip_transparent: true,
                };
                let sliced =
                    grid_slice_frames(&spec, sheet_rgba.as_ref().map(|(_, _, b)| b.as_slice()))?;
                for (name, rect) in sliced {
                    frames.insert(name, rect);
                }
            }
        }
        if frames.is_empty() {
            frames.insert(
                "full".to_string(),
                AtlasFrame {
                    x: 0,
                    y: 0,
                    w: sw,
                    h: sh,
                },
            );
        }

        let atlas = Atlas {
            texture: tex_name,
            size: (sw, sh),
            frames,
            animations: BTreeMap::new(),
        };
        if let Some(parent) = ptatlas_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        save_atlas(ptatlas_path, &atlas)?;
        Self::from_path(ptatlas_path.to_path_buf(), res_path)
    }

    /// Generate a unique frame name with `base` prefix.
    pub fn next_frame_name(&self, base: &str) -> String {
        let mut idx = self.atlas.frames.len();
        loop {
            let candidate = format!("{base}{idx}");
            if !self.atlas.frames.contains_key(&candidate) {
                return candidate;
            }
            idx += 1;
        }
    }

    pub fn select_frame(&mut self, name: Option<String>) {
        self.frame_rename_buf = name.clone().unwrap_or_default();
        self.selected_frame = name;
    }

    /// Rename a frame and rewrite any matching animation entries to preserve links.
    pub fn rename_frame(&mut self, old: &str, new_name: &str) -> bool {
        let trimmed = new_name.trim();
        if trimmed.is_empty() || trimmed == old || self.atlas.frames.contains_key(trimmed) {
            return false;
        }
        let Some(frame) = self.atlas.frames.remove(old) else {
            return false;
        };
        self.atlas.frames.insert(trimmed.to_string(), frame);
        for anim in self.atlas.animations.values_mut() {
            for f in &mut anim.frames {
                if f == old {
                    *f = trimmed.to_string();
                }
            }
        }
        if self.selected_frame.as_deref() == Some(old) {
            self.selected_frame = Some(trimmed.to_string());
            self.frame_rename_buf = trimmed.to_string();
        }
        if self.range_anchor.as_deref() == Some(old) {
            self.range_anchor = Some(trimmed.to_string());
        }
        self.dirty = true;
        true
    }

    /// Delete a frame (and prune references from animations). Returns false if not found.
    pub fn delete_frame(&mut self, name: &str) -> bool {
        if self.atlas.frames.remove(name).is_none() {
            return false;
        }
        for anim in self.atlas.animations.values_mut() {
            anim.frames.retain(|f| f != name);
        }
        if self.selected_frame.as_deref() == Some(name) {
            let next = self.atlas.frames.keys().next().cloned();
            self.select_frame(next);
        }
        self.dirty = true;
        true
    }

    pub fn duplicate_frame(&mut self, name: &str) -> Option<String> {
        let frame = self.atlas.frames.get(name).copied()?;
        let mut copy_idx = 1usize;
        let new_name = loop {
            let cand = format!("{name}_copy{copy_idx}");
            if !self.atlas.frames.contains_key(&cand) {
                break cand;
            }
            copy_idx += 1;
        };
        self.atlas.frames.insert(new_name.clone(), frame);
        self.select_frame(Some(new_name.clone()));
        self.dirty = true;
        Some(new_name)
    }

    pub fn add_full_image_frame(&mut self) -> String {
        let name = if self.atlas.frames.contains_key("full") {
            self.next_frame_name("full_")
        } else {
            "full".to_string()
        };
        let (sw, sh) = self.atlas.size;
        self.atlas.frames.insert(
            name.clone(),
            AtlasFrame {
                x: 0,
                y: 0,
                w: sw.max(1),
                h: sh.max(1),
            },
        );
        self.select_frame(Some(name.clone()));
        self.dirty = true;
        name
    }

    pub fn apply_grid_slice(&mut self) -> anyhow::Result<usize> {
        let (sw, sh) = self.atlas.size;
        let (tile_w, tile_h) = if self.slice_by_cols {
            let cols = self.slice_cols.max(1);
            let rows = self.slice_rows.max(1);
            let avail_w = sw.saturating_sub(self.slice_offset_x);
            let avail_h = sh.saturating_sub(self.slice_offset_y);
            let w = (avail_w.saturating_sub(self.slice_spacing_x * (cols - 1)) / cols).max(1);
            let h = (avail_h.saturating_sub(self.slice_spacing_y * (rows - 1)) / rows).max(1);
            (w, h)
        } else {
            (self.slice_tile_w.max(1), self.slice_tile_h.max(1))
        };

        let spec = GridSliceSpec {
            sheet_size: (sw, sh),
            tile_w,
            tile_h,
            offset_x: self.slice_offset_x,
            offset_y: self.slice_offset_y,
            spacing_x: self.slice_spacing_x,
            spacing_y: self.slice_spacing_y,
            prefix: self.slice_prefix.clone(),
            skip_transparent: self.slice_skip_transparent,
        };
        let rgba = self.sheet_rgba.as_ref().map(|(_, _, b)| b.as_slice());
        let sliced = grid_slice_frames(&spec, rgba)?;
        if sliced.is_empty() {
            anyhow::bail!("Grid slice matched zero valid cells inside {sw}x{sh}");
        }
        let count = sliced.len();
        if self.slice_replace {
            self.atlas.frames.clear();
            for (name, rect) in sliced {
                self.atlas.frames.insert(name, rect);
            }
            for anim in self.atlas.animations.values_mut() {
                anim.frames.retain(|f| self.atlas.frames.contains_key(f));
            }
        } else {
            for (name, rect) in sliced {
                let unique = if self.atlas.frames.contains_key(&name) {
                    self.next_frame_name(&format!("{name}_"))
                } else {
                    name
                };
                self.atlas.frames.insert(unique, rect);
            }
        }
        let first = self.atlas.frames.keys().next().cloned();
        self.select_frame(first.clone());
        self.range_anchor = first;
        self.dirty = true;
        Ok(count)
    }

    /// Append sorted frames between `from` and `to` (inclusive) to `anim_name`.
    pub fn append_frame_range_to_anim(&mut self, anim_name: &str, from: &str, to: &str) -> usize {
        let keys: Vec<String> = self.atlas.frames.keys().cloned().collect();
        let Some(i1) = keys.iter().position(|k| k == from) else {
            return 0;
        };
        let Some(i2) = keys.iter().position(|k| k == to) else {
            return 0;
        };
        let (lo, hi) = if i1 <= i2 { (i1, i2) } else { (i2, i1) };
        let Some(anim) = self.atlas.animations.get_mut(anim_name) else {
            return 0;
        };
        let slice = &keys[lo..=hi];
        let added = slice.len();
        anim.frames.extend(slice.iter().cloned());
        if added > 0 {
            self.dirty = true;
        }
        added
    }

    pub fn rename_animation(&mut self, old: &str, new_name: &str) -> bool {
        let trimmed = new_name.trim();
        if trimmed.is_empty() || trimmed == old || self.atlas.animations.contains_key(trimmed) {
            return false;
        }
        let Some(anim) = self.atlas.animations.remove(old) else {
            return false;
        };
        self.atlas.animations.insert(trimmed.to_string(), anim);
        if self.selected_anim.as_deref() == Some(old) {
            self.selected_anim = Some(trimmed.to_string());
            self.anim_rename_buf = trimmed.to_string();
        }
        self.dirty = true;
        true
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        save_atlas(&self.path, &self.atlas)?;
        self.dirty = false;
        self.last_error = None;
        Ok(())
    }

    pub(super) fn ensure_sheet_texture(&mut self, ctx: &egui::Context) {
        if self.sheet_tex.is_some() {
            return;
        }
        let Some((w, h, rgba)) = &self.sheet_rgba else {
            return;
        };
        let img = egui::ColorImage::from_rgba_unmultiplied([*w as usize, *h as usize], rgba);
        let tex = ctx.load_texture(
            format!("atlas_sheet:{}", self.path.display()),
            img,
            egui::TextureOptions::NEAREST,
        );
        self.sheet_tex = Some(tex);
    }
}

fn resolve_sheet_path(sidecar: &Path, texture_field: &str) -> PathBuf {
    let sibling = sidecar
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(texture_field);
    if sibling.is_file() {
        sibling
    } else {
        sheet_for_sidecar(sidecar)
    }
}
