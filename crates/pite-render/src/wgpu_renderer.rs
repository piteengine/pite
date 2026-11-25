// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windowed surface `wgpu` renderer implementation.

use anyhow::Result;

use crate::pipeline::{create_sprite_pipeline, request_device_for};
use crate::queue::{delegate_renderer2d_to_shared, encode_groups, SharedQueue};
use crate::Renderer2D;

pub struct WgpuRenderer {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    shared: SharedQueue,
}

impl WgpuRenderer {
    pub fn new(window: std::sync::Arc<winit::window::Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window)?;
        let (adapter, device, queue) = request_device_for(&instance, Some(&surface), false)?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(wgpu::TextureFormat::Bgra8Unorm);
        let alpha = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: alpha,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let (tex_layout, pipeline, sampler) = create_sprite_pipeline(&device, format);
        let shared = SharedQueue::new(device, queue, pipeline, sampler, tex_layout)?;
        Ok(Self {
            surface,
            config,
            shared,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.shared.device, &self.config);
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
}

impl Renderer2D for WgpuRenderer {
    delegate_renderer2d_to_shared!();

    fn end_frame(&mut self) -> Result<()> {
        let size = (self.config.width, self.config.height);
        let batches = self.shared.build(size);
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.shared.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(())
            }
            other => anyhow::bail!("surface failed: {other:?}"),
        };
        let view = frame.texture.create_view(&Default::default());
        let cmd = encode_groups(&self.shared, &batches, &view);
        self.shared.queue.submit(std::iter::once(cmd));
        self.shared.queue.present(frame);
        Ok(())
    }
}
