// SPDX-License-Identifier: MIT OR Apache-2.0
//! Headless offscreen `wgpu` renderer implementation with RGBA readback.

use anyhow::{Context, Result};

use crate::pipeline::{create_sprite_pipeline, request_device_for};
use crate::queue::{delegate_renderer2d_to_shared, encode_groups, SharedQueue};
use crate::Renderer2D;

pub struct OffscreenRenderer {
    target: wgpu::Texture,
    width: u32,
    height: u32,
    shared: SharedQueue,
}

impl OffscreenRenderer {
    pub fn new_offscreen(width: u32, height: u32) -> Result<Self> {
        let width = width.max(1);
        let height = height.max(1);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let (_adapter, device, queue) = request_device_for(&instance, None, false)?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let (tex_layout, pipeline, sampler) = create_sprite_pipeline(&device, format);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pite offscreen target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let shared = SharedQueue::new(device, queue, pipeline, sampler, tex_layout)?;
        Ok(Self {
            target,
            width,
            height,
            shared,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn render_queued_to_target(&mut self) -> wgpu::CommandBuffer {
        let size = (self.width, self.height);
        let batches = self.shared.build(size);
        let view = self.target.create_view(&Default::default());
        encode_groups(&self.shared, &batches, &view)
    }

    pub fn render_to_rgba(&mut self) -> Result<Vec<u8>> {
        let (w, h) = (self.width, self.height);
        let padded_bytes_per_row: u32 = (w * 4).div_ceil(256) * 256;
        let buffer_size: u64 = padded_bytes_per_row as u64 * h as u64;
        let draw_cmd = self.render_queued_to_target();
        let mut encoder = self
            .shared
            .device
            .create_command_encoder(&Default::default());
        let readback = self.shared.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pite offscreen readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.shared.queue.submit([draw_cmd, encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.shared
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("offscreen readback poll failed")?;
        rx.recv()
            .map_err(|_| anyhow::anyhow!("offscreen readback channel closed"))??;
        let out = {
            let view = readback
                .slice(..)
                .get_mapped_range()
                .context("offscreen readback buffer failed to map")?;
            let row_len = w as usize * 4;
            let pitch = padded_bytes_per_row as usize;
            let mut out = vec![0u8; row_len * h as usize];
            for y in 0..h as usize {
                out[y * row_len..(y + 1) * row_len]
                    .copy_from_slice(&view[y * pitch..y * pitch + row_len]);
            }
            out
        };
        readback.unmap();
        Ok(out)
    }
}

impl Renderer2D for OffscreenRenderer {
    delegate_renderer2d_to_shared!();

    fn end_frame(&mut self) -> Result<()> {
        let cmd = self.render_queued_to_target();
        self.shared.queue.submit(std::iter::once(cmd));
        Ok(())
    }
}
