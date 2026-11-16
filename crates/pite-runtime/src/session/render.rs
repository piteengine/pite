// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::Result;
use pite_core::{global_position, Node, NodeTree, PropValue};
use pite_render::Renderer2D;

use super::physics::prop_num;
use super::tilemap_cache::{BakedTileMap, CHUNK_SIZE};
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
        modulate: [u8; 4],
        z_index: i32,
    },
    AnimatedSprite {
        id: String,
        atlas: String,
        animation: String,
        frame: i64,
        pos: (f64, f64),
        rotation: f32,
        modulate: [u8; 4],
        z_index: i32,
    },
    Text {
        text: String,
        size: f32,
        color: [u8; 4],
        pos: (f64, f64),
        z_index: i32,
    },
    Button {
        text: String,
        size: f32,
        color: [u8; 4],
        pos: (f64, f64),
        extent: (f64, f64),
        z_index: i32,
    },
    TileMap {
        id: String,
        pos: (f64, f64),
        z_index: i32,
    },
}

pub(crate) fn extract_draw(node: &Node, tree: &NodeTree) -> Option<NodeDraw> {
    let role = render_role(&node.type_name)?;
    let pos = global_position(tree, &node.id);
    let z_index = match node.props.get("z_index") {
        Some(PropValue::Int(i)) => *i as i32,
        Some(PropValue::Num(n)) => *n as i32,
        _ => 0,
    };
    match role {
        RenderRole::Sprite => {
            let prop = |key: &str| match node.props.get(key) {
                Some(PropValue::Str(s)) if !s.is_empty() => Some(s.clone()),
                _ => None,
            };
            let modulate = match node.props.get("modulate") {
                Some(PropValue::Color(r, g, b, a)) => [*r, *g, *b, *a],
                _ => [255, 255, 255, 255],
            };
            Some(NodeDraw::Sprite {
                id: node.id.to_string(),
                texture: prop("texture").unwrap_or_default(),
                atlas: prop("atlas"),
                frame: prop("frame"),
                pos,
                rotation: prop_num(node, "rotation"),
                modulate,
                z_index,
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
            let modulate = match node.props.get("modulate") {
                Some(PropValue::Color(r, g, b, a)) => [*r, *g, *b, *a],
                _ => [255, 255, 255, 255],
            };
            Some(NodeDraw::AnimatedSprite {
                id: node.id.to_string(),
                atlas: prop_str("atlas"),
                animation: prop_str("animation"),
                frame,
                pos,
                rotation: prop_num(node, "rotation"),
                modulate,
                z_index,
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
                z_index,
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
                z_index,
            })
        }
        RenderRole::TileMap => Some(NodeDraw::TileMap {
            id: node.id.to_string(),
            pos,
            z_index,
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
            self.tilemap_cache.clear();
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
                    modulate,
                    z_index,
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
                            r.draw_sprite_tinted(
                                &sheet.to_string_lossy(),
                                Some(&frame),
                                pos.0,
                                pos.1,
                                rotation as f64,
                                z_index,
                                modulate,
                            )?;
                        }
                        (Some(_), None) => anyhow::bail!("node {id}: `atlas` requires a `frame`"),
                        (None, Some(_)) => anyhow::bail!("node {id}: `frame` requires an `atlas`"),
                        (None, None) => {
                            if has_texture {
                                let resolved = self.resolve_path(&texture);
                                r.draw_sprite_tinted(
                                    &resolved.to_string_lossy(),
                                    None,
                                    pos.0,
                                    pos.1,
                                    rotation as f64,
                                    z_index,
                                    modulate,
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
                    modulate,
                    z_index,
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
                                r.draw_sprite_tinted(
                                    &sheet.to_string_lossy(),
                                    Some(frame_name),
                                    pos.0,
                                    pos.1,
                                    rotation as f64,
                                    z_index,
                                    modulate,
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
                    z_index,
                } => {
                    r.draw_text_layered(&text, pos.0, pos.1, size, color, z_index)?;
                }
                NodeDraw::Button {
                    text,
                    size,
                    color,
                    pos,
                    extent: (w, h),
                    z_index,
                } => {
                    r.draw_rect_layered(pos.0, pos.1, w, h, color, z_index)?;
                    if !text.is_empty() {
                        r.draw_text_layered(
                            &text,
                            pos.0,
                            pos.1,
                            size,
                            [255, 255, 255, 255],
                            z_index,
                        )?;
                    }
                }
                NodeDraw::TileMap { id, pos, z_index } => {
                    let current_version = self.host.get_tilemap_version(&id);
                    let needs_bake = match self.tilemap_cache.get(&id) {
                        Some(b) => b.version != current_version,
                        None => true,
                    };

                    if needs_bake {
                        if let Some(doc) = self.host.get_tilemap_doc(&id) {
                            let sidecar = self.resolve_path(&doc.atlas);
                            let sheet = pite_render::atlas::sheet_for_sidecar(&sidecar);
                            let sheet_str = sheet.to_string_lossy().into_owned();

                            let atlas_obj = match self.atlas_cache.entry(sidecar.clone()) {
                                std::collections::hash_map::Entry::Occupied(e) => {
                                    Some(&*e.into_mut())
                                }
                                std::collections::hash_map::Entry::Vacant(e) => {
                                    match pite_render::atlas::load_atlas(&sidecar) {
                                        Ok(a) => Some(&*e.insert(a)),
                                        Err(err) => {
                                            tracing::error!(
                                                node = %id,
                                                "cannot load atlas {}: {err:#}",
                                                sidecar.display()
                                            );
                                            None
                                        }
                                    }
                                }
                            };

                            let dirty_chunks = self.host.take_dirty_tile_chunks(&id);
                            if let (Some(b), Some(dirty)) =
                                (self.tilemap_cache.get_mut(&id), dirty_chunks)
                            {
                                if b.atlas_path == doc.atlas
                                    && b.tile_size == doc.tile_size
                                    && b.layers.len() == doc.layers.len()
                                {
                                    for (layer_idx, chunk_col, chunk_row) in dirty {
                                        b.rebake_chunk(
                                            &doc, layer_idx, chunk_col, chunk_row, atlas_obj,
                                        );
                                    }
                                    b.version = current_version;
                                } else {
                                    let baked = BakedTileMap::bake(
                                        &doc,
                                        sheet_str,
                                        current_version,
                                        atlas_obj,
                                    );
                                    self.tilemap_cache.insert(id.clone(), baked);
                                }
                            } else {
                                let baked =
                                    BakedTileMap::bake(&doc, sheet_str, current_version, atlas_obj);
                                self.tilemap_cache.insert(id.clone(), baked);
                            }
                        } else {
                            self.tilemap_cache.remove(&id);
                        }
                    }

                    if let Some(baked) = self.tilemap_cache.get(&id) {
                        if baked.tile_size[0] == 0 || baked.tile_size[1] == 0 {
                            continue;
                        }
                        let tile_w = baked.tile_size[0] as f64;
                        let tile_h = baked.tile_size[1] as f64;
                        let chunk_w = tile_w * CHUNK_SIZE as f64;
                        let chunk_h = tile_h * CHUNK_SIZE as f64;
                        let zoom = self.effective_zoom();
                        let half_w = (self.viewport.0 as f64 / 2.0) / zoom;
                        let half_h = (self.viewport.1 as f64 / 2.0) / zoom;
                        let min_x = cam.0 - half_w;
                        let max_x = cam.0 + half_w;
                        let min_y = cam.1 - half_h;
                        let max_y = cam.1 + half_h;

                        let min_chunk_col = ((min_x - pos.0) / chunk_w).floor() as i32 - 1;
                        let max_chunk_col = ((max_x - pos.0) / chunk_w).ceil() as i32 + 1;
                        let min_chunk_row = ((min_y - pos.1) / chunk_h).floor() as i32 - 1;
                        let max_chunk_row = ((max_y - pos.1) / chunk_h).ceil() as i32 + 1;

                        let view_chunk_count = (max_chunk_col - min_chunk_col + 1).max(0) as usize
                            * (max_chunk_row - min_chunk_row + 1).max(0) as usize;

                        for (layer_idx, layer) in baked.layers.iter().enumerate() {
                            if !layer.visible {
                                continue;
                            }
                            let layer_z = z_index + layer_idx as i32;

                            if view_chunk_count < layer.chunks.len() {
                                for chunk_row in min_chunk_row..=max_chunk_row {
                                    for chunk_col in min_chunk_col..=max_chunk_col {
                                        if let Some(chunk) =
                                            layer.chunks.get(&(chunk_col, chunk_row))
                                        {
                                            for tile in &chunk.tiles {
                                                r.draw_sprite_uv(
                                                    &baked.sheet_str,
                                                    tile.uv,
                                                    (tile_w as f32, tile_h as f32),
                                                    pos.0 + tile.local_x,
                                                    pos.1 + tile.local_y,
                                                    0.0,
                                                    layer_z,
                                                )?;
                                            }
                                        }
                                    }
                                }
                            } else {
                                for (&(chunk_col, chunk_row), chunk) in &layer.chunks {
                                    if chunk_col >= min_chunk_col
                                        && chunk_col <= max_chunk_col
                                        && chunk_row >= min_chunk_row
                                        && chunk_row <= max_chunk_row
                                    {
                                        for tile in &chunk.tiles {
                                            r.draw_sprite_uv(
                                                &baked.sheet_str,
                                                tile.uv,
                                                (tile_w as f32, tile_h as f32),
                                                pos.0 + tile.local_x,
                                                pos.1 + tile.local_y,
                                                0.0,
                                                layer_z,
                                            )?;
                                        }
                                    }
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
