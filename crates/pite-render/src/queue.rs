// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared sprite queue, z-index/texture batching, and GPU command encoding.

use anyhow::Result;
use wgpu::util::DeviceExt;

use crate::atlas::{self, Atlas};
use crate::coords::{screen_to_ndc, world_to_screen};
use crate::pipeline::{
    bind_group, upload, GpuTexture, SpriteInstance, FULL_UV, QUAD_INDICES, UNIT_QUAD_VERTS,
};
use crate::text;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpriteBatch {
    pub(crate) texture_id: u32,
    pub(crate) instance_range: std::ops::Range<u32>,
}

pub(crate) struct QueuedSprite {
    pub(crate) texture_id: u32,
    pub(crate) world: (f64, f64),
    pub(crate) size_world: Option<(f64, f64)>,
    /// Frame size in pixels; `None` means "the whole texture".
    pub(crate) size_px: Option<(u32, u32)>,
    pub(crate) uv: [f32; 4],
    /// Radians, positive = clockwise on screen (y-down NDC).
    pub(crate) rotation: f32,
    pub(crate) color: [u8; 4],
    pub(crate) z_index: i32,
    pub(crate) order: usize,
}

impl QueuedSprite {
    fn plain(texture_id: u32, world: (f64, f64), z_index: i32, order: usize) -> Self {
        Self {
            texture_id,
            world,
            size_world: None,
            size_px: None,
            uv: FULL_UV,
            rotation: 0.0,
            color: [255, 255, 255, 255],
            z_index,
            order,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SpriteSortKey {
    z_index: i32,
    texture_id: u32,
    order: u32,
}

/// Shared vertex-build half of `end_frame`: sorts sprites by z-index and submission order,
/// batching consecutive sprites that share a texture. Sizes are resolved by the caller (which
/// owns the texture cache) so both the surface and offscreen paths share it.
pub(crate) fn build_groups(
    sprites: Vec<QueuedSprite>,
    sizes: &[(u32, u32)],
    cam: (f64, f64),
    zoom: f64,
    size: (u32, u32),
) -> (Vec<SpriteBatch>, Vec<SpriteInstance>) {
    let mut sort_keys: Vec<(SpriteSortKey, u32)> = sprites
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            (
                SpriteSortKey {
                    z_index: s.z_index,
                    texture_id: s.texture_id,
                    order: (s.order & 0xffff_ffff) as u32,
                },
                idx as u32,
            )
        })
        .collect();
    sort_keys.sort_unstable_by_key(|&(key, _)| key);

    let inv_viewport = [2.0 / size.0 as f32, 2.0 / size.1 as f32];
    let mut batches: Vec<SpriteBatch> = Vec::new();
    let mut instances: Vec<SpriteInstance> = Vec::with_capacity(sprites.len());

    for &(_, idx) in &sort_keys {
        let sprite = &sprites[idx as usize];
        let (tw, th) = sizes
            .get(sprite.texture_id as usize)
            .copied()
            .unwrap_or((8, 8));
        // Every size scales with zoom: positions already do, so a sprite
        // that stayed pixel-fixed while its center moved would shear apart
        // from the scene on resize (and under Camera2D zoom).
        let (w, h) = match (sprite.size_world, sprite.size_px) {
            (Some((w, h)), _) => ((w * zoom) as f32, (h * zoom) as f32),
            (None, Some((w, h))) => ((w as f64 * zoom) as f32, (h as f64 * zoom) as f32),
            (None, None) => ((tw as f64 * zoom) as f32, (th as f64 * zoom) as f32),
        };
        let screen = world_to_screen(sprite.world, cam, zoom, size);
        let ndc = screen_to_ndc(screen, size);

        let instance = SpriteInstance {
            center_ndc: [ndc.0, ndc.1],
            size_px: [w, h],
            inv_viewport,
            uv_rect: sprite.uv,
            rotation: sprite.rotation,
            color: sprite.color,
        };

        if let Some(last) = batches.last_mut() {
            if last.texture_id == sprite.texture_id {
                instances.push(instance);
                last.instance_range.end += 1;
                continue;
            }
        }
        let start = instances.len() as u32;
        instances.push(instance);
        batches.push(SpriteBatch {
            texture_id: sprite.texture_id,
            instance_range: start..start + 1,
        });
    }
    (batches, instances)
}

/// Resolve a frame name to UV rect + pixel size, loading the sheet's sidecar
/// on first use. Missing sidecar or frame is an error, never a silent
/// full-texture fallback.
pub(crate) fn frame_uv(
    cache: &mut std::collections::HashMap<String, Atlas>,
    texture: &str,
    frame: &str,
) -> Result<([f32; 4], (u32, u32))> {
    if !cache.contains_key(texture) {
        let sidecar = atlas::sidecar_for(std::path::Path::new(texture));
        let loaded = atlas::load_atlas(&sidecar)?;
        cache.insert(texture.to_string(), loaded);
    }
    let atlas = &cache[texture];
    let found = atlas.frame(frame)?;
    Ok((found.uv(atlas.size), (found.w, found.h)))
}

/// Encode one render pass drawing pre-built batches into `view`.
pub(crate) fn encode_groups(
    shared: &SharedQueue,
    batches: &[SpriteBatch],
    view: &wgpu::TextureView,
) -> wgpu::CommandBuffer {
    let mut encoder = shared.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pite pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.07,
                        g: 0.07,
                        b: 0.10,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&shared.pipeline);
        pass.set_vertex_buffer(0, shared.quad_vbuf.slice(..));
        pass.set_index_buffer(shared.quad_ibuf.slice(..), wgpu::IndexFormat::Uint16);
        pass.set_vertex_buffer(1, shared.instance_buf.slice(..));
        for batch in batches {
            if batch.instance_range.is_empty() {
                continue;
            }
            let Some(Some(tex)) = shared.textures.get(batch.texture_id as usize) else {
                continue;
            };
            pass.set_bind_group(0, &tex.bind_group, &[]);
            pass.draw_indexed(0..6, 0, batch.instance_range.clone());
        }
    }
    encoder.finish()
}

/// The GPU objects, caches, sprite queue and camera that the surface and
/// offscreen paths share. Only submission (and readback) differs between them,
/// so both renderers hold one of these instead of copying the state.
pub(crate) struct SharedQueue {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    tex_layout: wgpu::BindGroupLayout,
    textures: Vec<Option<GpuTexture>>,
    texture_sizes: Vec<(u32, u32)>,
    texture_lookup: std::collections::HashMap<String, u32>,
    free_slots: Vec<u32>,
    baked: std::collections::HashMap<String, text::BakedText>,
    baked_order: std::collections::VecDeque<String>,
    atlas: text::TextAtlas,
    queue_list: Vec<QueuedSprite>,
    order_counter: usize,
    sprite_atlases: std::collections::HashMap<String, Atlas>,
    cam: (f64, f64),
    zoom: f64,
    quad_vbuf: wgpu::Buffer,
    quad_ibuf: wgpu::Buffer,
    instance_buf: wgpu::Buffer,
    instance_buf_capacity: usize,
}

impl SharedQueue {
    pub(crate) fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        pipeline: wgpu::RenderPipeline,
        sampler: wgpu::Sampler,
        tex_layout: wgpu::BindGroupLayout,
    ) -> Result<Self> {
        let quad_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite unit quad vbuf"),
            contents: bytemuck::cast_slice(&UNIT_QUAD_VERTS),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite quad ibuf"),
            contents: bytemuck::cast_slice(&QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        const DEFAULT_INSTANCE_CAPACITY: usize = 1024 * 1024; // 1 MB
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pite instance buffer"),
            size: DEFAULT_INSTANCE_CAPACITY as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            sampler,
            tex_layout,
            textures: Vec::new(),
            texture_sizes: Vec::new(),
            texture_lookup: std::collections::HashMap::new(),
            free_slots: Vec::new(),
            baked: std::collections::HashMap::new(),
            baked_order: std::collections::VecDeque::new(),
            atlas: text::TextAtlas::new()?,
            queue_list: Vec::new(),
            order_counter: 0,
            sprite_atlases: std::collections::HashMap::new(),
            cam: (0.0, 0.0),
            zoom: 1.0,
            quad_vbuf,
            quad_ibuf,
            instance_buf,
            instance_buf_capacity: DEFAULT_INSTANCE_CAPACITY,
        })
    }

    pub(crate) fn begin_frame(&mut self) {
        self.queue_list.clear();
        self.order_counter = 0;
    }

    fn upload_instances(&mut self, instances: &[SpriteInstance]) {
        if instances.is_empty() {
            return;
        }
        let required_bytes = std::mem::size_of_val(instances);
        if required_bytes > self.instance_buf_capacity {
            let new_cap = (required_bytes * 2).max(self.instance_buf_capacity * 2);
            self.instance_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pite instance buffer"),
                size: new_cap as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_buf_capacity = new_cap;
        }
        self.queue
            .write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(instances));
    }

    pub(crate) fn set_camera(&mut self, pos: (f64, f64), zoom: f64) {
        self.cam = pos;
        self.zoom = if zoom > 0.0 { zoom } else { 1.0 };
    }

    fn next_order(&mut self) -> usize {
        let o = self.order_counter;
        self.order_counter += 1;
        o
    }

    fn load_texture(&self, key: &str) -> GpuTexture {
        match image::open(key).map(|img| img.to_rgba8()).ok() {
            Some(rgba) => {
                let (w, h) = (rgba.width(), rgba.height());
                let texture = upload(&self.device, &self.queue, &rgba, w, h);
                let view = texture.create_view(&Default::default());
                let bind_group = bind_group(&self.device, &self.tex_layout, &self.sampler, &view);
                GpuTexture { bind_group, w, h }
            }
            None => {
                tracing::warn!(texture = key, "cannot load texture, using fallback");
                let texture = upload(&self.device, &self.queue, &[255, 0, 255, 255], 1, 1);
                let view = texture.create_view(&Default::default());
                let bind_group = bind_group(&self.device, &self.tex_layout, &self.sampler, &view);
                GpuTexture {
                    bind_group,
                    w: 8,
                    h: 8,
                }
            }
        }
    }

    fn texture_id_for(&mut self, key: &str) -> u32 {
        if let Some(&id) = self.texture_lookup.get(key) {
            return id;
        }
        let baked = self.baked.get(key).map(|b| (b.rgba.clone(), b.w, b.h));
        let entry = match baked {
            Some((rgba, w, h)) => {
                let texture = upload(&self.device, &self.queue, &rgba, w, h);
                let view = texture.create_view(&Default::default());
                let bind_group = bind_group(&self.device, &self.tex_layout, &self.sampler, &view);
                GpuTexture { bind_group, w, h }
            }
            None => self.load_texture(key),
        };
        let size = (entry.w, entry.h);
        if let Some(slot) = self.free_slots.pop() {
            let idx = slot as usize;
            self.textures[idx] = Some(entry);
            self.texture_sizes[idx] = size;
            self.texture_lookup.insert(key.to_string(), slot);
            slot
        } else {
            let slot = self.textures.len() as u32;
            self.textures.push(Some(entry));
            self.texture_sizes.push(size);
            self.texture_lookup.insert(key.to_string(), slot);
            slot
        }
    }

    pub(crate) fn queue_sprite(
        &mut self,
        texture: &str,
        world: (f64, f64),
        rotation: f32,
    ) -> Result<()> {
        self.queue_sprite_tinted(texture, None, world, rotation, 0, [255, 255, 255, 255])
    }

    pub(crate) fn queue_sprite_frame(
        &mut self,
        texture: &str,
        frame: Option<&str>,
        world: (f64, f64),
        rotation: f32,
    ) -> Result<()> {
        self.queue_sprite_tinted(texture, frame, world, rotation, 0, [255, 255, 255, 255])
    }

    pub(crate) fn queue_sprite_tinted(
        &mut self,
        texture: &str,
        frame: Option<&str>,
        world: (f64, f64),
        rotation: f32,
        z_index: i32,
        color: [u8; 4],
    ) -> Result<()> {
        let order = self.next_order();
        let (uv, size_px) = match frame {
            Some(frame_name) => {
                let (uv, size_px) = frame_uv(&mut self.sprite_atlases, texture, frame_name)?;
                (uv, Some(size_px))
            }
            None => (FULL_UV, None),
        };
        let texture_id = self.texture_id_for(texture);
        self.queue_list.push(QueuedSprite {
            texture_id,
            world,
            size_world: None,
            size_px,
            uv,
            rotation,
            color,
            z_index,
            order,
        });
        Ok(())
    }

    // Intentional: Item is private and arguments are semantically separated.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn queue_sprite_uv(
        &mut self,
        texture: &str,
        uv: [f32; 4],
        size_px: (f32, f32),
        world: (f64, f64),
        rotation: f32,
        z_index: i32,
        color: [u8; 4],
    ) -> Result<()> {
        let order = self.next_order();
        let texture_id = self.texture_id_for(texture);
        self.queue_list.push(QueuedSprite {
            texture_id,
            world,
            size_world: None,
            size_px: Some((size_px.0.round() as u32, size_px.1.round() as u32)),
            uv,
            rotation,
            color,
            z_index,
            order,
        });
        Ok(())
    }

    pub(crate) fn queue_rect(&mut self, world: (f64, f64), size: (f64, f64), color: [u8; 4]) {
        self.queue_rect_layered(world, size, color, 0);
    }

    fn ensure_white_1x1(&mut self) -> u32 {
        let key = "__pite_white_1x1";
        if !self.baked.contains_key(key) {
            self.baked.insert(
                key.to_string(),
                text::BakedText {
                    rgba: vec![255, 255, 255, 255],
                    w: 1,
                    h: 1,
                },
            );
        }
        self.texture_id_for(key)
    }

    pub(crate) fn queue_rect_layered(
        &mut self,
        world: (f64, f64),
        size: (f64, f64),
        color: [u8; 4],
        z_index: i32,
    ) {
        let texture_id = self.ensure_white_1x1();
        let order = self.next_order();
        self.queue_list.push(QueuedSprite {
            size_world: Some((size.0.max(1.0), size.1.max(1.0))),
            color,
            ..QueuedSprite::plain(texture_id, world, z_index, order)
        });
    }

    pub(crate) fn queue_line_layered(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        color: [u8; 4],
        thickness: f64,
        z_index: i32,
    ) {
        let dx = to.0 - from.0;
        let dy = to.1 - from.1;
        let len = (dx * dx + dy * dy).sqrt();
        let mid = ((from.0 + to.0) * 0.5, (from.1 + to.1) * 0.5);
        let texture_id = self.ensure_white_1x1();
        let order = self.next_order();
        self.queue_list.push(QueuedSprite {
            size_world: Some((len.max(1.0), thickness.max(1.0))),
            rotation: dy.atan2(dx) as f32,
            color,
            ..QueuedSprite::plain(texture_id, mid, z_index, order)
        });
    }

    pub(crate) fn draw_text_queued(
        &mut self,
        text: &str,
        world: (f64, f64),
        size: f32,
        color: [u8; 4],
    ) {
        self.draw_text_queued_layered(text, world, size, color, 0);
    }

    pub(crate) fn draw_text_queued_layered(
        &mut self,
        text: &str,
        world: (f64, f64),
        size: f32,
        color: [u8; 4],
        z_index: i32,
    ) {
        if text.is_empty() {
            return;
        }
        let px = ((size as f64 * self.zoom).round().max(1.0)) as u32;
        let key = text::glyph_cache_key(text, px, color);
        if !self.baked.contains_key(&key) {
            self.baked
                .insert(key.clone(), self.atlas.bake(text, px as f32, color));
            text::lru_touch(&mut self.baked_order, &key, 64);
            while self.baked_order.len() > 64 {
                if let Some(old) = self.baked_order.pop_front() {
                    self.baked.remove(&old);
                    if let Some(slot) = self.texture_lookup.remove(&old) {
                        self.textures[slot as usize] = None;
                        self.free_slots.push(slot);
                    }
                }
            }
        }
        let texture_id = self.texture_id_for(&key);
        let order = self.next_order();
        self.queue_list
            .push(QueuedSprite::plain(texture_id, world, z_index, order));
    }

    /// Drain the queue, resolve every queued texture once, and build the
    /// batches ordered by (z_index, order) for a `size`-sized target.
    pub(crate) fn build(&mut self, size: (u32, u32)) -> Vec<SpriteBatch> {
        self.order_counter = 0;
        let sprites = std::mem::take(&mut self.queue_list);
        let (batches, instances) =
            build_groups(sprites, &self.texture_sizes, self.cam, self.zoom, size);
        self.upload_instances(&instances);
        batches
    }

    pub(crate) fn reload_asset(&mut self, path: &str) {
        if let Some(&id) = self.texture_lookup.get(path) {
            let tex = self.load_texture(path);
            let size = (tex.w, tex.h);
            self.textures[id as usize] = Some(tex);
            self.texture_sizes[id as usize] = size;
        }
        self.sprite_atlases.remove(path);
        let path_obj = std::path::Path::new(path);
        if let Some(stem) = path_obj.file_stem().and_then(|s| s.to_str()) {
            let pure_stem = stem.strip_suffix(".atlas").unwrap_or(stem);
            self.sprite_atlases.retain(|k, _| !k.contains(pure_stem));
            for (key, &id) in &self.texture_lookup {
                if key.contains(pure_stem) {
                    let tex = self.load_texture(key);
                    let size = (tex.w, tex.h);
                    self.textures[id as usize] = Some(tex);
                    self.texture_sizes[id as usize] = size;
                }
            }
        }
    }

    pub(crate) fn clear_asset_cache(&mut self) {
        self.textures.clear();
        self.texture_sizes.clear();
        self.texture_lookup.clear();
        self.free_slots.clear();
        self.sprite_atlases.clear();
    }
}

/// Delegates all [`Renderer2D`] queue/camera/cache methods to `self.shared`,
/// leaving only `end_frame` to the concrete renderer target.
macro_rules! delegate_renderer2d_to_shared {
    () => {
        fn begin_frame(&mut self) -> anyhow::Result<()> {
            self.shared.begin_frame();
            Ok(())
        }

        fn draw_sprite(
            &mut self,
            texture: &str,
            pos: (f64, f64),
            rotation: f64,
        ) -> anyhow::Result<()> {
            self.shared.queue_sprite(texture, pos, rotation as f32)
        }

        fn draw_sprite_frame(
            &mut self,
            texture: &str,
            frame: Option<&str>,
            pos: (f64, f64),
            rotation: f64,
        ) -> anyhow::Result<()> {
            self.shared
                .queue_sprite_frame(texture, frame, pos, rotation as f32)
        }

        fn draw_sprite_layered(
            &mut self,
            texture: &str,
            frame: Option<&str>,
            pos: (f64, f64),
            rotation: f64,
            z_index: i32,
        ) -> anyhow::Result<()> {
            self.draw_sprite_tinted(texture, frame, pos, rotation, z_index, [255, 255, 255, 255])
        }

        fn draw_sprite_tinted(
            &mut self,
            texture: &str,
            frame: Option<&str>,
            pos: (f64, f64),
            rotation: f64,
            z_index: i32,
            color: [u8; 4],
        ) -> anyhow::Result<()> {
            self.shared
                .queue_sprite_tinted(texture, frame, pos, rotation as f32, z_index, color)
        }

        fn draw_sprite_uv(
            &mut self,
            texture: &str,
            uv: [f32; 4],
            size_px: (f32, f32),
            pos: (f64, f64),
            rotation: f64,
            z_index: i32,
        ) -> anyhow::Result<()> {
            self.draw_sprite_uv_tinted(
                texture,
                uv,
                size_px,
                pos,
                rotation,
                z_index,
                [255, 255, 255, 255],
            )
        }

        fn draw_sprite_uv_tinted(
            &mut self,
            texture: &str,
            uv: [f32; 4],
            size_px: (f32, f32),
            pos: (f64, f64),
            rotation: f64,
            z_index: i32,
            color: [u8; 4],
        ) -> anyhow::Result<()> {
            self.shared
                .queue_sprite_uv(texture, uv, size_px, pos, rotation as f32, z_index, color)
        }

        fn draw_rect(
            &mut self,
            pos: (f64, f64),
            size: (f64, f64),
            color: [u8; 4],
        ) -> anyhow::Result<()> {
            self.shared.queue_rect(pos, size, color);
            Ok(())
        }

        fn draw_rect_layered(
            &mut self,
            pos: (f64, f64),
            size: (f64, f64),
            color: [u8; 4],
            z_index: i32,
        ) -> anyhow::Result<()> {
            self.shared.queue_rect_layered(pos, size, color, z_index);
            Ok(())
        }

        fn draw_line_layered(
            &mut self,
            from: (f64, f64),
            to: (f64, f64),
            color: [u8; 4],
            thickness: f64,
            z_index: i32,
        ) -> anyhow::Result<()> {
            self.shared
                .queue_line_layered(from, to, color, thickness, z_index);
            Ok(())
        }

        fn draw_text(
            &mut self,
            text: &str,
            pos: (f64, f64),
            size: f32,
            color: [u8; 4],
        ) -> anyhow::Result<()> {
            self.shared.draw_text_queued(text, pos, size, color);
            Ok(())
        }

        fn draw_text_layered(
            &mut self,
            text: &str,
            pos: (f64, f64),
            size: f32,
            color: [u8; 4],
            z_index: i32,
        ) -> anyhow::Result<()> {
            self.shared
                .draw_text_queued_layered(text, pos, size, color, z_index);
            Ok(())
        }

        fn set_camera(&mut self, pos: (f64, f64), zoom: f64) {
            self.shared.set_camera(pos, zoom);
        }

        fn reload_asset(&mut self, path: &str) {
            self.shared.reload_asset(path);
        }

        fn clear_asset_cache(&mut self) {
            self.shared.clear_asset_cache();
        }
    };
}

pub(crate) use delegate_renderer2d_to_shared;

#[cfg(test)]
mod atlas_draw_tests {
    use super::*;
    use std::collections::HashMap;

    use crate::atlas::AtlasFrame;

    fn sheet_frame(name: &str) -> (Atlas, AtlasFrame) {
        let atlas = atlas::parse_atlas(
            r#"format_version = 1
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
"#,
        )
        .unwrap();
        let frame = atlas.frame(name).unwrap();
        (atlas, frame)
    }

    fn queued(atlas: &Atlas, frame: AtlasFrame, world: (f64, f64)) -> QueuedSprite {
        QueuedSprite {
            texture_id: 0,
            world,
            size_world: None,
            size_px: Some((frame.w, frame.h)),
            uv: frame.uv(atlas.size),
            rotation: 0.0,
            color: [255, 255, 255, 255],
            z_index: 0,
            order: 0,
        }
    }

    #[test]
    fn atlas_frames_from_one_sheet_batch_into_one_group() {
        let (atlas, player) = sheet_frame("player");
        let (_, enemy) = sheet_frame("enemy");
        let sprites = vec![
            queued(&atlas, player, (0.0, 0.0)),
            queued(&atlas, enemy, (40.0, 0.0)),
        ];
        let sizes = [atlas.size];
        let (batches, instances) = build_groups(sprites, &sizes, (0.0, 0.0), 1.0, (200, 100));

        assert_eq!(batches.len(), 1, "one sheet, one draw");
        assert_eq!(batches[0].texture_id, 0);
        assert_eq!(batches[0].instance_range, 0..2);
        assert_eq!(instances.len(), 2, "two instances share the group");

        assert_eq!(instances[0].uv_rect, [0.0, 0.0, 0.5, 1.0]);
        assert_eq!(instances[1].uv_rect, [0.5, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn frame_quad_uses_frame_size_not_sheet_size() {
        let (atlas, player) = sheet_frame("player");
        let sprites = vec![queued(&atlas, player, (0.0, 0.0))];
        let sizes = [atlas.size];
        let (batches, instances) = build_groups(sprites, &sizes, (0.0, 0.0), 1.0, (200, 100));
        assert_eq!(batches.len(), 1);
        assert_eq!(instances[0].size_px, [32.0, 32.0]);
    }

    #[test]
    fn z_index_and_consecutive_batching_preserves_draw_order() {
        let sprites = vec![
            QueuedSprite::plain(0, (0.0, 0.0), 0, 0),
            QueuedSprite::plain(1, (10.0, 0.0), 0, 1),
            QueuedSprite::plain(0, (20.0, 0.0), 0, 2),
            QueuedSprite::plain(1, (-10.0, 0.0), -1, 3),
        ];
        let sizes = [(16, 16), (16, 16)];

        let (batches, instances) = build_groups(sprites, &sizes, (0.0, 0.0), 1.0, (200, 100));

        // Grouping by texture within z_index reduces batches from 4 to 3:
        // z = -1: [tex_b (inst 0)]
        // z =  0: [tex_a (inst 1, 2)], [tex_b (inst 3)]
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].texture_id, 1);
        assert_eq!(batches[0].instance_range, 0..1);
        assert_eq!(batches[1].texture_id, 0);
        assert_eq!(batches[1].instance_range, 1..3);
        assert_eq!(batches[2].texture_id, 1);
        assert_eq!(batches[2].instance_range, 3..4);
        assert_eq!(instances.len(), 4);
    }

    #[test]
    fn missing_sidecar_and_frame_are_loud() {
        let mut cache: HashMap<String, Atlas> = HashMap::new();
        let err = frame_uv(&mut cache, "/nowhere/sheet.png", "player").unwrap_err();
        assert!(
            format!("{err:#}").contains("cannot read atlas"),
            "got {err:#}"
        );
    }
}
