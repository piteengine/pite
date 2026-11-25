// SPDX-License-Identifier: MIT OR Apache-2.0
//! 2D hardware-accelerated rendering subsystem for the Pite game engine.
//!
//! Defines the `Renderer2D` trait and provides a pure-Rust `wgpu` implementation
//! supporting batched sprite quad rendering, texture atlas sampling, font
//! rasterization, and viewport projections.

use anyhow::Result;

pub mod atlas;
mod coords;
mod offscreen;
mod pipeline;
mod queue;
pub mod text;
mod wgpu_renderer;

pub use atlas::{Atlas, AtlasAnimation, AtlasFrame};
pub use coords::{fit_scale, screen_to_ndc, screen_to_world, world_to_screen};
pub use offscreen::OffscreenRenderer;
pub use wgpu_renderer::WgpuRenderer;

pub trait Renderer2D {
    fn begin_frame(&mut self) -> Result<()>;
    fn draw_sprite(&mut self, texture: &str, pos: (f64, f64), rotation: f64) -> Result<()>;
    /// Draw `texture`, optionally from one named atlas frame, rotated by
    /// `rotation` radians around the sprite center. The default impl
    /// forwards to `draw_sprite`, so impls without atlas support stay
    /// correct.
    fn draw_sprite_frame(
        &mut self,
        texture: &str,
        frame: Option<&str>,
        pos: (f64, f64),
        rotation: f64,
    ) -> Result<()> {
        let _ = frame;
        self.draw_sprite(texture, pos, rotation)
    }
    /// Draw `texture`, optionally from one named atlas frame, at explicit
    /// `z_index`. Higher z_index values draw on top. Ties fall back to
    /// submission order.
    fn draw_sprite_layered(
        &mut self,
        texture: &str,
        frame: Option<&str>,
        pos: (f64, f64),
        rotation: f64,
        z_index: i32,
    ) -> Result<()> {
        self.draw_sprite_tinted(texture, frame, pos, rotation, z_index, [255, 255, 255, 255])
    }
    /// Draw `texture`, optionally from one named atlas frame, at explicit
    /// `z_index` and modulated by `color` (RGBA). Higher z_index values draw
    /// on top. Ties fall back to submission order.
    fn draw_sprite_tinted(
        &mut self,
        texture: &str,
        frame: Option<&str>,
        pos: (f64, f64),
        rotation: f64,
        z_index: i32,
        color: [u8; 4],
    ) -> Result<()> {
        let _ = (z_index, color);
        self.draw_sprite_frame(texture, frame, pos, rotation)
    }
    /// Draw `texture` using pre-resolved normalized `uv` rect `[u_min, v_min, u_max, v_max]`
    /// and pixel dimensions `size_px: (f32, f32)`. Higher z_index values draw on top.
    fn draw_sprite_uv(
        &mut self,
        texture: &str,
        uv: [f32; 4],
        size_px: (f32, f32),
        pos: (f64, f64),
        rotation: f64,
        z_index: i32,
    ) -> Result<()> {
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
    /// Draw `texture` using pre-resolved normalized `uv` rect `[u_min, v_min, u_max, v_max]`
    /// and pixel dimensions `size_px: (f32, f32)` modulated by `color` (RGBA).
    // Intentional: Item is private and arguments are semantically separated.
    #[allow(clippy::too_many_arguments)]
    fn draw_sprite_uv_tinted(
        &mut self,
        texture: &str,
        uv: [f32; 4],
        size_px: (f32, f32),
        pos: (f64, f64),
        rotation: f64,
        z_index: i32,
        color: [u8; 4],
    ) -> Result<()> {
        let _ = (uv, size_px, color);
        self.draw_sprite_layered(texture, None, pos, rotation, z_index)
    }
    fn draw_text(&mut self, text: &str, pos: (f64, f64), size: f32, color: [u8; 4]) -> Result<()>;
    fn draw_text_layered(
        &mut self,
        text: &str,
        pos: (f64, f64),
        size: f32,
        color: [u8; 4],
        z_index: i32,
    ) -> Result<()> {
        let _ = z_index;
        self.draw_text(text, pos, size, color)
    }
    fn draw_rect(&mut self, pos: (f64, f64), size: (f64, f64), color: [u8; 4]) -> Result<()>;
    fn draw_rect_layered(
        &mut self,
        pos: (f64, f64),
        size: (f64, f64),
        color: [u8; 4],
        z_index: i32,
    ) -> Result<()> {
        let _ = z_index;
        self.draw_rect(pos, size, color)
    }
    fn end_frame(&mut self) -> Result<()>;
    fn set_camera(&mut self, _pos: (f64, f64), _zoom: f64) {}
    fn reload_asset(&mut self, _path: &str) {}
    fn clear_asset_cache(&mut self) {}
}

#[derive(Debug, Default)]
pub struct NoopRenderer;

impl Renderer2D for NoopRenderer {
    fn begin_frame(&mut self) -> Result<()> {
        Ok(())
    }

    fn draw_sprite(&mut self, _texture: &str, _pos: (f64, f64), _rotation: f64) -> Result<()> {
        Ok(())
    }

    fn draw_sprite_tinted(
        &mut self,
        _texture: &str,
        _frame: Option<&str>,
        _pos: (f64, f64),
        _rotation: f64,
        _z_index: i32,
        _color: [u8; 4],
    ) -> Result<()> {
        Ok(())
    }

    fn draw_text(
        &mut self,
        _text: &str,
        _pos: (f64, f64),
        _size: f32,
        _color: [u8; 4],
    ) -> Result<()> {
        Ok(())
    }

    fn draw_rect(&mut self, _pos: (f64, f64), _size: (f64, f64), _color: [u8; 4]) -> Result<()> {
        Ok(())
    }

    fn end_frame(&mut self) -> Result<()> {
        Ok(())
    }

    fn reload_asset(&mut self, _path: &str) {}
    fn clear_asset_cache(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{
        bind_group, create_sprite_pipeline, request_device_for, upload, SpriteInstance, FULL_UV,
        QUAD_INDICES, UNIT_QUAD_VERTS,
    };
    use wgpu::util::DeviceExt;

    #[test]
    fn demo_textures_decode() {
        let sheet = image::open("../../examples/minimal-2d/assets/sheet.png").unwrap();
        assert_eq!((sheet.width(), sheet.height()), (128, 64));
    }

    #[test]
    fn sprite_renders_green_pixel_offscreen() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok((_adapter, device, queue)) = request_device_for(&instance, None, true) else {
            eprintln!("SKIP: no fallback GPU adapter on this machine");
            return;
        };
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let (tex_layout, pipeline, sampler) = create_sprite_pipeline(&device, format);

        let sheet = image::open("../../examples/minimal-2d/assets/sheet.png")
            .unwrap()
            .to_rgba8();
        // The `player` frame's rect from sheet.ptatlas: (0, 0, 32, 32).
        let rgba = image::imageops::crop_imm(&sheet, 0, 0, 32, 32).to_image();
        let sprite = upload(&device, &queue, &rgba, 32, 32);
        let sprite_view = sprite.create_view(&Default::default());
        let sprite_bg = bind_group(&device, &tex_layout, &sampler, &sprite_view);

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pite test target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&Default::default());
        let quad_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite test quad vbuf"),
            contents: bytemuck::cast_slice(&UNIT_QUAD_VERTS),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite test quad ibuf"),
            contents: bytemuck::cast_slice(&QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance = SpriteInstance {
            center_ndc: [0.0, 0.0],
            size_px: [64.0, 64.0],
            inv_viewport: [2.0 / 64.0, 2.0 / 64.0],
            uv_rect: FULL_UV,
            rotation: 0.0,
            color: [255, 255, 255, 255],
        };
        let inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite test inst buf"),
            contents: bytemuck::cast_slice(&[instance]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pite test pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
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
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &sprite_bg, &[]);
            pass.set_vertex_buffer(0, quad_vbuf.slice(..));
            pass.set_vertex_buffer(1, inst_buf.slice(..));
            pass.set_index_buffer(quad_ibuf.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pite test readback"),
            size: 64 * 64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(64 * 4),
                    rows_per_image: Some(64),
                },
            },
            wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let center: [u8; 4];
        {
            let view = readback
                .slice(..)
                .get_mapped_range()
                .expect("offscreen readback maps");
            let i = (32 * 64 + 32) * 4;
            center = [view[i], view[i + 1], view[i + 2], view[i + 3]];
        }
        readback.unmap();
        assert!(
            center[1] as i32 - center[0] as i32 > 60,
            "center pixel should be sprite-green, got {center:?}"
        );
    }

    #[test]
    fn offscreen_clear_is_srgb_encoded_to_match_the_window_surface() {
        let Ok(mut r) = OffscreenRenderer::new_offscreen(8, 8) else {
            eprintln!("SKIP: no GPU adapter on this machine");
            return;
        };
        let rgba = r.render_to_rgba().expect("offscreen readback");
        assert_eq!(
            &rgba[0..3],
            &[75, 75, 89],
            "editor viewport must show the same background as the game window"
        );
    }

    #[test]
    fn offscreen_sprite_keeps_authored_srgb_colors() {
        let Ok(mut r) = OffscreenRenderer::new_offscreen(64, 64) else {
            eprintln!("SKIP: no GPU adapter on this machine");
            return;
        };
        r.draw_sprite_frame(
            "../../examples/minimal-2d/assets/sheet.png",
            Some("player"),
            (0.0, 0.0),
            0.0,
        )
        .expect("queue sprite");
        let rgba = r.render_to_rgba().expect("offscreen readback");
        let seen: std::collections::HashSet<[u8; 4]> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
        assert!(
            seen.contains(&[74, 222, 128, 255]),
            "authored green must survive the round-trip, saw {:?}",
            seen
        );
    }

    #[test]
    fn offscreen_sprite_modulate_tints_color() {
        let Ok(mut r) = OffscreenRenderer::new_offscreen(64, 64) else {
            eprintln!("SKIP: no GPU adapter on this machine");
            return;
        };
        // Draw with pure red tint [255, 0, 0, 255]. Green sprite should have green eliminated!
        r.draw_sprite_tinted(
            "../../examples/minimal-2d/assets/sheet.png",
            Some("player"),
            (0.0, 0.0),
            0.0,
            0,
            [255, 0, 0, 255],
        )
        .expect("queue sprite tinted");
        let rgba = r.render_to_rgba().expect("offscreen readback");
        let seen: std::collections::HashSet<[u8; 4]> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
        // Authored green was [74, 222, 128, 255]. Modulated by [255, 0, 0, 255] -> green & blue become 0!
        assert!(
            seen.contains(&[74, 0, 0, 255]),
            "red-modulated sprite should produce [74, 0, 0, 255], saw {:?}",
            seen
        );
    }

    #[test]
    fn offscreen_sprite_covers_its_whole_texture_rect() {
        let Ok(mut r) = OffscreenRenderer::new_offscreen(200, 200) else {
            eprintln!("SKIP: no GPU adapter on this machine");
            return;
        };
        r.draw_sprite_frame(
            "../../examples/minimal-2d/assets/sheet.png",
            Some("player"),
            (0.0, 0.0),
            0.0,
        )
        .expect("queue sprite");
        let rgba = r.render_to_rgba().expect("offscreen readback");
        let clear = (75u8, 75u8, 89u8);
        let mut min = (200usize, 200usize);
        let mut max = (0usize, 0usize);
        for y in 0..200usize {
            for x in 0..200usize {
                let i = (y * 200 + x) * 4;
                if (rgba[i], rgba[i + 1], rgba[i + 2]) != clear {
                    min.0 = min.0.min(x);
                    min.1 = min.1.min(y);
                    max.0 = max.0.max(x);
                    max.1 = max.1.max(y);
                }
            }
        }
        assert_eq!(
            (min.0, min.1, max.0 - min.0 + 1, max.1 - min.1 + 1),
            (68, 68, 64, 64),
            "a 64x64 frame must fill 64x64 pixels, centered in a 200x200 target"
        );
    }

    #[test]
    fn baked_text_renders_bright_pixels_offscreen() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok((_adapter, device, queue)) = request_device_for(&instance, None, true) else {
            eprintln!("SKIP: no fallback GPU adapter on this machine");
            return;
        };
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let (tex_layout, pipeline, sampler) = create_sprite_pipeline(&device, format);

        let atlas = text::TextAtlas::new().unwrap();
        let baked = atlas.bake("HP", 32.0, [255, 255, 255, 255]);
        let glyph = upload(&device, &queue, &baked.rgba, baked.w, baked.h);
        let glyph_view = glyph.create_view(&Default::default());
        let glyph_bg = bind_group(&device, &tex_layout, &sampler, &glyph_view);

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pite text test target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&Default::default());
        let quad_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite test text quad vbuf"),
            contents: bytemuck::cast_slice(&UNIT_QUAD_VERTS),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite test text quad ibuf"),
            contents: bytemuck::cast_slice(&QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance = SpriteInstance {
            center_ndc: [0.0, 0.0],
            size_px: [64.0, 64.0],
            inv_viewport: [2.0 / 64.0, 2.0 / 64.0],
            uv_rect: FULL_UV,
            rotation: 0.0,
            color: [255, 255, 255, 255],
        };
        let inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite test text inst buf"),
            contents: bytemuck::cast_slice(&[instance]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pite text test pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.2,
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
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &glyph_bg, &[]);
            pass.set_vertex_buffer(0, quad_vbuf.slice(..));
            pass.set_vertex_buffer(1, inst_buf.slice(..));
            pass.set_index_buffer(quad_ibuf.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pite text test readback"),
            size: 64 * 64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(64 * 4),
                    rows_per_image: Some(64),
                },
            },
            wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let bright = {
            let view = readback
                .slice(..)
                .get_mapped_range()
                .expect("offscreen readback maps");
            view.chunks(4)
                .filter(|px| px[0] > 200 && px[1] > 200 && px[2] > 200)
                .count()
        };
        readback.unmap();
        assert!(bright > 10, "expected bright glyph pixels, got {bright}");
    }
}
