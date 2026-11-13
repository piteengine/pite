// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::Result;
use pite_core::{global_position, Node, NodeTree, PropValue};
use pite_render::Renderer2D;

use super::physics::prop_num;
use super::GameSession;

/// Which draw call a node type produces. `draw_into` walks the tree once
/// through this, rather than three separate `filter(type_name == ...)`
/// passes. Adding a renderable type = one new arm here, no new loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderRole {
    Sprite,
    AnimatedSprite,
    Label,
    Button,
    TileMap,
}

pub(crate) fn render_role(type_name: &str) -> Option<RenderRole> {
    match type_name {
        "Sprite2D" => Some(RenderRole::Sprite),
        "AnimatedSprite2D" => Some(RenderRole::AnimatedSprite),
        "Label" => Some(RenderRole::Label),
        "Button" => Some(RenderRole::Button),
        "TileMap2D" => Some(RenderRole::TileMap),
        _ => None,
    }
}

/// One node's renderable payload, resolved under the tree lock and drawn
/// after it's released.
pub(crate) enum NodeDraw {
    Sprite {
        id: String,
        texture: String,
        atlas: Option<String>,
        frame: Option<String>,
        pos: (f64, f64),
        rotation: f32,
    },
    AnimatedSprite {
        id: String,
        atlas: String,
        animation: String,
        frame: i64,
        pos: (f64, f64),
        rotation: f32,
    },
    Text {
        text: String,
        size: f32,
        color: [u8; 4],
        pos: (f64, f64),
    },
    Button {
        text: String,
        size: f32,
        color: [u8; 4],
        pos: (f64, f64),
        extent: (f64, f64),
    },
    TileMap {
        id: String,
        pos: (f64, f64),
    },
}

pub(crate) fn extract_draw(node: &Node, tree: &NodeTree) -> Option<NodeDraw> {
    let role = render_role(&node.type_name)?;
    let pos = global_position(tree, &node.id);
    match role {
        RenderRole::Sprite => {
            let prop = |key: &str| match node.props.get(key) {
                Some(PropValue::Str(s)) if !s.is_empty() => Some(s.clone()),
                _ => None,
            };
            Some(NodeDraw::Sprite {
                id: node.id.to_string(),
                texture: prop("texture").unwrap_or_default(),
                atlas: prop("atlas"),
                frame: prop("frame"),
                pos,
                rotation: prop_num(node, "rotation"),
            })
        }
        RenderRole::AnimatedSprite => {
            let prop_str = |key: &str| match node.props.get(key) {
                Some(PropValue::Str(s)) if !s.is_empty() => s.clone(),
                _ => String::new(),
            };
            let frame = match node.props.get("frame") {
                Some(PropValue::Int(i)) => *i,
                Some(PropValue::Num(n)) => *n as i64,
                _ => 0,
            };
            Some(NodeDraw::AnimatedSprite {
                id: node.id.to_string(),
                atlas: prop_str("atlas"),
                animation: prop_str("animation"),
                frame,
                pos,
                rotation: prop_num(node, "rotation"),
            })
        }
        RenderRole::Label => {
            let text = match node.props.get("text") {
                Some(PropValue::Str(s)) if !s.is_empty() => s.clone(),
                _ => return None,
            };
            let size = match node.props.get("font_size") {
                Some(PropValue::Num(s)) => *s as f32,
                Some(PropValue::Int(s)) => *s as f32,
                _ => 16.0,
            };
            let color = match node.props.get("color") {
                Some(PropValue::Color(r, g, b, a)) => [*r, *g, *b, *a],
                _ => [255, 255, 255, 255],
            };
            Some(NodeDraw::Text {
                text,
                size,
                color,
                pos,
            })
        }
        RenderRole::Button => {
            let text = match node.props.get("text") {
                Some(PropValue::Str(s)) => s.clone(),
                _ => String::new(),
            };
            let size = match node.props.get("font_size") {
                Some(PropValue::Num(s)) => *s as f32,
                Some(PropValue::Int(s)) => *s as f32,
                _ => 16.0,
            };
            let color = match node.props.get("color") {
                Some(PropValue::Color(r, g, b, a)) => [*r, *g, *b, *a],
                _ => [51, 65, 85, 255],
            };
            let extent = button_size(node);
            Some(NodeDraw::Button {
                text,
                size,
                color,
                pos,
                extent,
            })
        }
        RenderRole::TileMap => Some(NodeDraw::TileMap {
            id: node.id.to_string(),
            pos,
        }),
    }
}

pub(crate) fn button_size(node: &pite_core::Node) -> (f64, f64) {
    match node.props.get("size") {
        Some(PropValue::Vec2(w, h)) => (*w, *h),
        _ => (120.0, 40.0),
    }
}

impl GameSession {
    pub fn draw_into(&mut self, r: &mut dyn Renderer2D) -> Result<()> {
        if !self.dirty_assets.is_empty() {
            for p in self.dirty_assets.drain() {
                self.atlas_cache.remove(&p);
                r.reload_asset(&p.to_string_lossy());
            }
        }
        let (cam, _) = self.camera_view();
        r.set_camera(cam.0, cam.1, self.effective_zoom());

        // One walk: every renderable node becomes a `NodeDraw`, drawn in tree
        // order (matching Godot's z-order — reorder the tree to reorder draws).
        // This replaces the previous three per-type filter loops; new
        // renderable types add an arm in `render_role`, not another pass.
        let draws: Vec<NodeDraw> = self
            .host
            .with_tree(|tree| tree.iter().filter_map(|n| extract_draw(n, tree)).collect());

        for draw in draws {
            match draw {
                NodeDraw::Sprite {
                    id,
                    texture,
                    atlas,
                    frame,
                    pos,
                    rotation,
                } => {
                    let has_texture = !texture.is_empty();
                    let atlas = atlas.filter(|s| !s.is_empty());
                    let frame = frame.filter(|s| !s.is_empty());
                    if has_texture && atlas.is_some() {
                        anyhow::bail!("node {id}: `texture` and `atlas` are mutually exclusive");
                    }
                    match (atlas, frame) {
                        (Some(atlas), Some(frame)) => {
                            let sidecar = self.resolve_path(&atlas);
                            let sheet = pite_render::atlas::sheet_for_sidecar(&sidecar);
                            r.draw_sprite_frame(
                                &sheet.to_string_lossy(),
                                Some(&frame),
                                pos.0,
                                pos.1,
                                rotation as f64,
                            )?;
                        }
                        (Some(_), None) => anyhow::bail!("node {id}: `atlas` requires a `frame`"),
                        (None, Some(_)) => anyhow::bail!("node {id}: `frame` requires an `atlas`"),
                        (None, None) => {
                            if has_texture {
                                let resolved = self.resolve_path(&texture);
                                r.draw_sprite(
                                    &resolved.to_string_lossy(),
                                    pos.0,
                                    pos.1,
                                    rotation as f64,
                                )?;
                            }
                        }
                    }
                }
                NodeDraw::AnimatedSprite {
                    id,
                    atlas,
                    animation,
                    frame,
                    pos,
                    rotation,
                } => {
                    if atlas.is_empty() {
                        continue;
                    }
                    let sidecar = self.resolve_path(&atlas);
                    let atlas_obj = match self.atlas_cache.entry(sidecar.clone()) {
                        std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                        std::collections::hash_map::Entry::Vacant(e) => {
                            match pite_render::atlas::load_atlas(&sidecar) {
                                Ok(a) => e.insert(a),
                                Err(e) => {
                                    tracing::error!(node = %id, "cannot load atlas {}: {e:#}", sidecar.display());
                                    continue;
                                }
                            }
                        }
                    };
                    let anim_name = if !animation.is_empty() {
                        Some(animation.as_str())
                    } else {
                        atlas_obj.animation_names().into_iter().next()
                    };
                    if let Some(anim_name) = anim_name {
                        if let Ok(anim) = atlas_obj.animation(anim_name) {
                            if !anim.frames.is_empty() {
                                let idx = (frame.max(0) as usize) % anim.frames.len();
                                let frame_name = &anim.frames[idx];
                                let sheet = pite_render::atlas::sheet_for_sidecar(&sidecar);
                                r.draw_sprite_frame(
                                    &sheet.to_string_lossy(),
                                    Some(frame_name),
                                    pos.0,
                                    pos.1,
                                    rotation as f64,
                                )?;
                            }
                        }
                    }
                }
                NodeDraw::Text {
                    text,
                    size,
                    color,
                    pos,
                } => {
                    r.draw_text(&text, pos.0, pos.1, size, color)?;
                }
                NodeDraw::Button {
                    text,
                    size,
                    color,
                    pos,
                    extent: (w, h),
                } => {
                    r.draw_rect(pos.0, pos.1, w, h, color)?;
                    if !text.is_empty() {
                        r.draw_text(&text, pos.0, pos.1, size, [255, 255, 255, 255])?;
                    }
                }
                NodeDraw::TileMap { id, pos } => {
                    if let Some(doc) = self.host.get_tilemap_doc(&id) {
                        let sidecar = self.resolve_path(&doc.atlas);
                        let sheet = pite_render::atlas::sheet_for_sidecar(&sidecar);
                        let sheet_str = sheet.to_string_lossy();
                        let tile_w = doc.tile_size[0] as f64;
                        let tile_h = doc.tile_size[1] as f64;
                        let zoom = self.effective_zoom();
                        let half_w = (self.viewport.0 as f64 / 2.0) / zoom;
                        let half_h = (self.viewport.1 as f64 / 2.0) / zoom;
                        let min_x = cam.0 - half_w;
                        let max_x = cam.0 + half_w;
                        let min_y = cam.1 - half_h;
                        let max_y = cam.1 + half_h;

                        let min_col = ((min_x - pos.0) / tile_w).floor() as i32 - 1;
                        let max_col = ((max_x - pos.0) / tile_w).ceil() as i32 + 1;
                        let min_row = ((min_y - pos.1) / tile_h).floor() as i32 - 1;
                        let max_row = ((max_y - pos.1) / tile_h).ceil() as i32 + 1;

                        for layer in &doc.layers {
                            if !layer.visible {
                                continue;
                            }
                            for tile in &layer.tiles {
                                if tile.0 >= min_col
                                    && tile.0 <= max_col
                                    && tile.1 >= min_row
                                    && tile.1 <= max_row
                                {
                                    let cx = pos.0 + (tile.0 as f64 + 0.5) * tile_w;
                                    let cy = pos.1 + (tile.1 as f64 + 0.5) * tile_h;
                                    r.draw_sprite_frame(&sheet_str, Some(&tile.2), cx, cy, 0.0)?;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
