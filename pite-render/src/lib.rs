//! `pite-render`: `Renderer2D` trait. wgpu is the only impl (M1);
//! the viewport talks to the trait so batching/3D later doesn't touch
//! tree or editor code.

use anyhow::{Context, Result};
use wgpu::util::DeviceExt;

pub mod atlas;
pub mod text;

pub use atlas::{Atlas, AtlasFrame};

const FULL_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

pub trait Renderer2D {
    fn begin_frame(&mut self) -> Result<()>;
    fn draw_sprite(&mut self, texture: &str, x: f64, y: f64) -> Result<()>;
    /// Draw `texture`, optionally from one named atlas frame. The default
    /// ignores the frame, so impls without atlas support stay correct.
    fn draw_sprite_frame(
        &mut self,
        texture: &str,
        frame: Option<&str>,
        x: f64,
        y: f64,
    ) -> Result<()> {
        let _ = frame;
        self.draw_sprite(texture, x, y)
    }
    fn draw_text(&mut self, text: &str, x: f64, y: f64, size: f32, color: [u8; 4]) -> Result<()>;
    fn draw_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: [u8; 4]) -> Result<()>;
    fn end_frame(&mut self) -> Result<()>;
    fn set_camera(&mut self, _x: f64, _y: f64, _zoom: f64) {}
}

#[derive(Debug, Default)]
pub struct NoopRenderer;

impl Renderer2D for NoopRenderer {
    fn begin_frame(&mut self) -> Result<()> {
        Ok(())
    }

    fn draw_sprite(&mut self, _texture: &str, _x: f64, _y: f64) -> Result<()> {
        Ok(())
    }

    fn draw_text(
        &mut self,
        _text: &str,
        _x: f64,
        _y: f64,
        _size: f32,
        _color: [u8; 4],
    ) -> Result<()> {
        Ok(())
    }

    fn draw_rect(
        &mut self,
        _x: f64,
        _y: f64,
        _w: f64,
        _h: f64,
        _color: [u8; 4],
    ) -> Result<()> {
        Ok(())
    }

    fn end_frame(&mut self) -> Result<()> {
        Ok(())
    }
}

pub fn world_to_screen(
    world: (f64, f64),
    cam: (f64, f64),
    zoom: f64,
    size: (u32, u32),
) -> (f32, f32) {
    let sx = (size.0 as f64 / 2.0 + (world.0 - cam.0) * zoom) as f32;
    let sy = (size.1 as f64 / 2.0 + (world.1 - cam.1) * zoom) as f32;
    (sx, sy)
}

pub fn screen_to_world(screen: (f32, f32), cam: (f64, f64), zoom: f64, size: (u32, u32)) -> (f64, f64) {
    let zoom = if zoom == 0.0 { 1.0 } else { zoom };
    (
        cam.0 + (screen.0 as f64 - size.0 as f64 / 2.0) / zoom,
        cam.1 + (screen.1 as f64 - size.1 as f64 / 2.0) / zoom,
    )
}

pub fn screen_to_ndc(screen: (f32, f32), size: (u32, u32)) -> (f32, f32) {
    (
        screen.0 / size.0 as f32 * 2.0 - 1.0,
        1.0 - screen.1 / size.1 as f32 * 2.0,
    )
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SpriteVertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

fn quad_for(
    center_ndc: (f32, f32),
    w_px: f32,
    h_px: f32,
    size: (u32, u32),
    uv: [f32; 4],
) -> [SpriteVertex; 4] {
    let hw = w_px / size.0 as f32 / 2.0;
    let hh = h_px / size.1 as f32 / 2.0;
    let (cx, cy) = center_ndc;
    [
        SpriteVertex { pos: [cx - hw, cy + hh], uv: [uv[0], uv[1]] },
        SpriteVertex { pos: [cx + hw, cy + hh], uv: [uv[2], uv[1]] },
        SpriteVertex { pos: [cx + hw, cy - hh], uv: [uv[2], uv[3]] },
        SpriteVertex { pos: [cx - hw, cy - hh], uv: [uv[0], uv[3]] },
    ]
}

const SPRITE_SHADER: &str = r#"
struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var sprite_tex: texture_2d<f32>;
@group(0) @binding(1) var sprite_smp: sampler;

@vertex
fn vs(@location(0) p: vec2<f32>, @location(1) uv: vec2<f32>) -> VertexOut {
    var out: VertexOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(sprite_tex, sprite_smp, in.uv);
}
"#;

struct GpuTexture {
    #[allow(dead_code)]
    texture: wgpu::Texture,
    #[allow(dead_code)]
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    w: u32,
    h: u32,
}

struct QueuedSprite {
    tex_key: String,
    world: (f64, f64),
    size_world: Option<(f64, f64)>,
    /// Frame size in pixels; `None` means "the whole texture".
    size_px: Option<(u32, u32)>,
    uv: [f32; 4],
}

impl QueuedSprite {
    fn plain(tex_key: String, world: (f64, f64)) -> Self {
        Self {
            tex_key,
            world,
            size_world: None,
            size_px: None,
            uv: FULL_UV,
        }
    }
}

fn create_sprite_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline, wgpu::Sampler) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("pite sprite"),
        source: wgpu::ShaderSource::Wgsl(SPRITE_SHADER.into()),
    });
    let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("pite tex layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pite pipe layout"),
        bind_group_layouts: &[&tex_layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pite sprite pipe"),
        layout: Some(&pipe_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<SpriteVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
            }],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("pite sampler"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    (tex_layout, pipeline, sampler)
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pite tex bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &[u8],
    w: u32,
    h: u32,
) -> wgpu::Texture {
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pite tex"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * w),
            rows_per_image: Some(h),
        },
        size,
    );
    tex
}

/// Shared vertex-build half of `end_frame`: group queued sprites by texture
/// key, preserving first-seen order. Sizes are resolved by the caller (which
/// owns the texture cache) so both the surface and offscreen paths share it.
fn build_groups(
    sprites: Vec<QueuedSprite>,
    sizes: &std::collections::HashMap<String, (u32, u32)>,
    cam: (f64, f64),
    zoom: f64,
    size: (u32, u32),
) -> (
    std::collections::HashMap<String, Vec<SpriteVertex>>,
    Vec<String>,
) {
    let mut groups: std::collections::HashMap<String, Vec<SpriteVertex>> =
        std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for sprite in sprites {
        let (tw, th) = sizes.get(&sprite.tex_key).copied().unwrap_or((8, 8));
        let (w, h) = match (sprite.size_world, sprite.size_px) {
            (Some((w, h)), _) => ((w * zoom) as f32, (h * zoom) as f32),
            (None, Some((w, h))) => (w as f32, h as f32),
            (None, None) => (tw as f32, th as f32),
        };
        let screen = world_to_screen(sprite.world, cam, zoom, size);
        let ndc = screen_to_ndc(screen, size);
        let quad = quad_for(ndc, w, h, size, sprite.uv);
        if !groups.contains_key(&sprite.tex_key) {
            order.push(sprite.tex_key.clone());
        }
        groups
            .entry(sprite.tex_key)
            .or_default()
            .extend_from_slice(&quad);
    }
    (groups, order)
}

/// Resolve a frame name to UV rect + pixel size, loading the sheet's sidecar
/// on first use. Missing sidecar or frame is an error, never a silent
/// full-texture fallback.
fn frame_uv(
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

/// Encode one render pass drawing pre-built groups into `view`.
fn encode_groups(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    textures: &std::collections::HashMap<String, GpuTexture>,
    groups: &std::collections::HashMap<String, Vec<SpriteVertex>>,
    order: &[String],
    view: &wgpu::TextureView,
) -> wgpu::CommandBuffer {
    let mut encoder = device.create_command_encoder(&Default::default());
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
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline);
        for key in order {
            let Some(verts) = groups.get(key) else {
                continue;
            };
            let Some(tex) = textures.get(key) else {
                continue;
            };
            let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("pite verts"),
                contents: bytemuck::cast_slice(verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
            pass.set_bind_group(0, &tex.bind_group, &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..verts.len() as u32, 0..1);
        }
    }
    encoder.finish()
}

pub struct WgpuRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    tex_layout: wgpu::BindGroupLayout,
    textures: std::collections::HashMap<String, GpuTexture>,
    baked: std::collections::HashMap<String, text::BakedText>,
    baked_order: std::collections::VecDeque<String>,
    atlas: text::TextAtlas,
    queue_list: Vec<QueuedSprite>,
    sprite_atlases: std::collections::HashMap<String, Atlas>,
    cam: (f64, f64),
    zoom: f64,
}

impl WgpuRenderer {
    pub fn new(window: std::sync::Arc<winit::window::Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window)?;
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            }))
            .context("no suitable GPU adapter")?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor::default(),
            None,
        ))
        .context("cannot request GPU device")?;
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
        };
        surface.configure(&device, &config);

        let (tex_layout, pipeline, sampler) = create_sprite_pipeline(&device, format);
        let renderer = Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            sampler,
            tex_layout,
            textures: std::collections::HashMap::new(),
            baked: std::collections::HashMap::new(),
            baked_order: std::collections::VecDeque::new(),
            atlas: text::TextAtlas::new()?,
            queue_list: Vec::new(),
            sprite_atlases: std::collections::HashMap::new(),
            cam: (0.0, 0.0),
            zoom: 1.0,
        };
        Ok(renderer)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn set_camera(&mut self, x: f64, y: f64, zoom: f64) {
        self.cam = (x, y);
        self.zoom = if zoom > 0.0 { zoom } else { 1.0 };
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn load_texture(&self, key: &str) -> GpuTexture {
        match image::open(key).map(|img| img.to_rgba8()).ok() {
            Some(rgba) => {
                let (w, h) = (rgba.width(), rgba.height());
                let texture = upload(&self.device, &self.queue, &rgba, w, h);
                let view = texture.create_view(&Default::default());
                let bind_group = bind_group(&self.device, &self.tex_layout, &self.sampler, &view);
                GpuTexture { texture, view, bind_group, w, h }
            }
            None => {
                tracing::warn!(texture = key, "cannot load texture, using fallback");
                let texture = upload(&self.device, &self.queue, &[255, 0, 255, 255], 1, 1);
                let view = texture.create_view(&Default::default());
                let bind_group =
                    bind_group(&self.device, &self.tex_layout, &self.sampler, &view);
                GpuTexture { texture, view, bind_group, w: 8, h: 8 }
            }
        }
    }

    fn texture_for(&mut self, key: &str) -> &GpuTexture {
        if !self.textures.contains_key(key) {
            let baked = self
                .baked
                .get(key)
                .map(|b| (b.rgba.clone(), b.w, b.h));
            let entry = match baked {
                Some((rgba, w, h)) => {
                    let texture = upload(&self.device, &self.queue, &rgba, w, h);
                    let view = texture.create_view(&Default::default());
                    let bind_group =
                        bind_group(&self.device, &self.tex_layout, &self.sampler, &view);
                    GpuTexture { texture, view, bind_group, w, h }
                }
                None => self.load_texture(key),
            };
            self.textures.insert(key.to_string(), entry);
        }
        &self.textures[key]
    }

    fn draw_text_queued(&mut self, text: &str, world: (f64, f64), size: f32, color: [u8; 4]) {
        if text.is_empty() {
            return;
        }
        let px = ((size as f64 * self.zoom).round().max(1.0)) as u32;
        let key = text::glyph_cache_key(text, px, color);
        if !self.baked.contains_key(&key) {
            self.baked.insert(key.clone(), self.atlas.bake(text, px as f32, color));
            text::lru_touch(&mut self.baked_order, &key, 64);
            while self.baked_order.len() > 64 {
                if let Some(old) = self.baked_order.pop_front() {
                    self.baked.remove(&old);
                    self.textures.remove(&old);
                }
            }
        }
        self.queue_list.push(QueuedSprite::plain(key, world));
    }
}

impl Renderer2D for WgpuRenderer {
    fn begin_frame(&mut self) -> Result<()> {
        self.queue_list.clear();
        Ok(())
    }

    fn draw_sprite(&mut self, texture: &str, x: f64, y: f64) -> Result<()> {
        self.queue_list
            .push(QueuedSprite::plain(texture.to_string(), (x, y)));
        Ok(())
    }

    fn draw_sprite_frame(&mut self, texture: &str, frame: Option<&str>, x: f64, y: f64) -> Result<()> {
        let Some(frame) = frame else {
            self.queue_list
                .push(QueuedSprite::plain(texture.to_string(), (x, y)));
            return Ok(());
        };
        let (uv, size_px) = frame_uv(&mut self.sprite_atlases, texture, frame)?;
        self.queue_list.push(QueuedSprite {
            tex_key: texture.to_string(),
            world: (x, y),
            size_world: None,
            size_px: Some(size_px),
            uv,
        });
        Ok(())
    }

    fn draw_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: [u8; 4]) -> Result<()> {
        let key = format!(
            "rect\0{}\0{}\0{}\0{}",
            color[0], color[1], color[2], color[3]
        );
        if !self.baked.contains_key(&key) {
            self.baked.insert(
                key.clone(),
                text::BakedText {
                    rgba: vec![color[0], color[1], color[2], color[3]],
                    w: 1,
                    h: 1,
                },
            );
            text::lru_touch(&mut self.baked_order, &key, 64);
        }
        self.queue_list.push(QueuedSprite {
            size_world: Some((w.max(1.0), h.max(1.0))),
            ..QueuedSprite::plain(key, (x, y))
        });
        Ok(())
    }

    fn draw_text(&mut self, text: &str, x: f64, y: f64, size: f32, color: [u8; 4]) -> Result<()> {
        self.draw_text_queued(text, (x, y), size, color);
        Ok(())
    }

    fn end_frame(&mut self) -> Result<()> {
        let size = (self.config.width, self.config.height);
        let sprites = std::mem::take(&mut self.queue_list);
        for sprite in &sprites {
            self.texture_for(&sprite.tex_key);
        }
        let sizes: std::collections::HashMap<String, (u32, u32)> = self
            .textures
            .iter()
            .map(|(k, t)| (k.clone(), (t.w, t.h)))
            .collect();
        let (groups, order) = build_groups(sprites, &sizes, self.cam, self.zoom, size);
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(e) => anyhow::bail!("surface failed: {e}"),
        };
        let view = frame.texture.create_view(&Default::default());
        let cmd = encode_groups(&self.device, &self.pipeline, &self.textures, &groups, &order, &view);
        self.queue.submit(std::iter::once(cmd));
        frame.present();
        Ok(())
    }

    fn set_camera(&mut self, x: f64, y: f64, zoom: f64) {
        self.cam = (x, y);
        self.zoom = if zoom > 0.0 { zoom } else { 1.0 };
    }
}

pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: wgpu::Texture,
    width: u32,
    height: u32,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    tex_layout: wgpu::BindGroupLayout,
    textures: std::collections::HashMap<String, GpuTexture>,
    baked: std::collections::HashMap<String, text::BakedText>,
    baked_order: std::collections::VecDeque<String>,
    atlas: text::TextAtlas,
    queue_list: Vec<QueuedSprite>,
    sprite_atlases: std::collections::HashMap<String, Atlas>,
    cam: (f64, f64),
    zoom: f64,
}

impl OffscreenRenderer {
    pub fn new_offscreen(width: u32, height: u32) -> Result<Self> {
        let width = width.max(1);
        let height = height.max(1);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            },
        ))
        .context("no suitable GPU adapter")?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor::default(),
            None,
        ))
        .context("cannot request GPU device")?;
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let (tex_layout, pipeline, sampler) = create_sprite_pipeline(&device, format);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pite offscreen target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Ok(Self {
            device,
            queue,
            target,
            width,
            height,
            pipeline,
            sampler,
            tex_layout,
            textures: std::collections::HashMap::new(),
            baked: std::collections::HashMap::new(),
            baked_order: std::collections::VecDeque::new(),
            atlas: text::TextAtlas::new()?,
            queue_list: Vec::new(),
            sprite_atlases: std::collections::HashMap::new(),
            cam: (0.0, 0.0),
            zoom: 1.0,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn set_camera_size(&mut self, x: f64, y: f64, zoom: f64) {
        self.cam = (x, y);
        self.zoom = if zoom > 0.0 { zoom } else { 1.0 };
    }

    fn load_texture(&self, key: &str) -> GpuTexture {
        match image::open(key).map(|img| img.to_rgba8()).ok() {
            Some(rgba) => {
                let (w, h) = (rgba.width(), rgba.height());
                let texture = upload(&self.device, &self.queue, &rgba, w, h);
                let view = texture.create_view(&Default::default());
                let bind_group = bind_group(&self.device, &self.tex_layout, &self.sampler, &view);
                GpuTexture { texture, view, bind_group, w, h }
            }
            None => {
                tracing::warn!(texture = key, "cannot load texture, using fallback");
                let texture = upload(&self.device, &self.queue, &[255, 0, 255, 255], 1, 1);
                let view = texture.create_view(&Default::default());
                let bind_group =
                    bind_group(&self.device, &self.tex_layout, &self.sampler, &view);
                GpuTexture { texture, view, bind_group, w: 8, h: 8 }
            }
        }
    }

    fn texture_for(&mut self, key: &str) -> &GpuTexture {
        if !self.textures.contains_key(key) {
            let baked = self
                .baked
                .get(key)
                .map(|b| (b.rgba.clone(), b.w, b.h));
            let entry = match baked {
                Some((rgba, w, h)) => {
                    let texture = upload(&self.device, &self.queue, &rgba, w, h);
                    let view = texture.create_view(&Default::default());
                    let bind_group =
                        bind_group(&self.device, &self.tex_layout, &self.sampler, &view);
                    GpuTexture { texture, view, bind_group, w, h }
                }
                None => self.load_texture(key),
            };
            self.textures.insert(key.to_string(), entry);
        }
        &self.textures[key]
    }

    fn draw_text_queued(&mut self, text: &str, world: (f64, f64), size: f32, color: [u8; 4]) {
        if text.is_empty() {
            return;
        }
        let px = ((size as f64 * self.zoom).round().max(1.0)) as u32;
        let key = text::glyph_cache_key(text, px, color);
        if !self.baked.contains_key(&key) {
            self.baked.insert(key.clone(), self.atlas.bake(text, px as f32, color));
            text::lru_touch(&mut self.baked_order, &key, 64);
            while self.baked_order.len() > 64 {
                if let Some(old) = self.baked_order.pop_front() {
                    self.baked.remove(&old);
                    self.textures.remove(&old);
                }
            }
        }
        self.queue_list.push(QueuedSprite::plain(key, world));
    }

    fn render_queued_to_target(&mut self) -> wgpu::CommandBuffer {
        let size = (self.width, self.height);
        let sprites = std::mem::take(&mut self.queue_list);
        for sprite in &sprites {
            self.texture_for(&sprite.tex_key);
        }
        let sizes: std::collections::HashMap<String, (u32, u32)> = self
            .textures
            .iter()
            .map(|(k, t)| (k.clone(), (t.w, t.h)))
            .collect();
        let (groups, order) = build_groups(sprites, &sizes, self.cam, self.zoom, size);
        let view = self.target.create_view(&Default::default());
        encode_groups(&self.device, &self.pipeline, &self.textures, &groups, &order, &view)
    }

    pub fn render_to_rgba(&mut self) -> Result<Vec<u8>> {
        let (w, h) = (self.width, self.height);
        let padded_bytes_per_row: u32 = ((w * 4 + 255) / 256) * 256;
        let buffer_size: u64 = padded_bytes_per_row as u64 * h as u64;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let size = (w, h);
        let sprites = std::mem::take(&mut self.queue_list);
        for sprite in &sprites {
            self.texture_for(&sprite.tex_key);
        }
        let sizes: std::collections::HashMap<String, (u32, u32)> = self
            .textures
            .iter()
            .map(|(k, t)| (k.clone(), (t.w, t.h)))
            .collect();
        let (groups, order) = build_groups(sprites, &sizes, self.cam, self.zoom, size);
        let view = self.target.create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pite offscreen pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            for key in &order {
                let Some(verts) = groups.get(key) else {
                    continue;
                };
                let Some(tex) = self.textures.get(key) else {
                    continue;
                };
                let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("pite offscreen verts"),
                    contents: bytemuck::cast_slice(verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                pass.set_bind_group(0, &tex.bind_group, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..verts.len() as u32, 0..1);
            }
        }
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pite offscreen readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &readback,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(std::iter::once(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|_| anyhow::anyhow!("offscreen readback channel closed"))??;
        let out = {
            let view = readback.slice(..).get_mapped_range();
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
    fn begin_frame(&mut self) -> Result<()> {
        self.queue_list.clear();
        Ok(())
    }

    fn draw_sprite(&mut self, texture: &str, x: f64, y: f64) -> Result<()> {
        self.queue_list
            .push(QueuedSprite::plain(texture.to_string(), (x, y)));
        Ok(())
    }

    fn draw_sprite_frame(&mut self, texture: &str, frame: Option<&str>, x: f64, y: f64) -> Result<()> {
        let Some(frame) = frame else {
            self.queue_list
                .push(QueuedSprite::plain(texture.to_string(), (x, y)));
            return Ok(());
        };
        let (uv, size_px) = frame_uv(&mut self.sprite_atlases, texture, frame)?;
        self.queue_list.push(QueuedSprite {
            tex_key: texture.to_string(),
            world: (x, y),
            size_world: None,
            size_px: Some(size_px),
            uv,
        });
        Ok(())
    }

    fn draw_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: [u8; 4]) -> Result<()> {
        let key = format!(
            "rect\0{}\0{}\0{}\0{}",
            color[0], color[1], color[2], color[3]
        );
        if !self.baked.contains_key(&key) {
            self.baked.insert(
                key.clone(),
                text::BakedText {
                    rgba: vec![color[0], color[1], color[2], color[3]],
                    w: 1,
                    h: 1,
                },
            );
            text::lru_touch(&mut self.baked_order, &key, 64);
        }
        self.queue_list.push(QueuedSprite {
            size_world: Some((w.max(1.0), h.max(1.0))),
            ..QueuedSprite::plain(key, (x, y))
        });
        Ok(())
    }

    fn draw_text(&mut self, text: &str, x: f64, y: f64, size: f32, color: [u8; 4]) -> Result<()> {
        self.draw_text_queued(text, (x, y), size, color);
        Ok(())
    }

    fn end_frame(&mut self) -> Result<()> {
        let cmd = self.render_queued_to_target();
        self.queue.submit(std::iter::once(cmd));
        Ok(())
    }

    fn set_camera(&mut self, x: f64, y: f64, zoom: f64) {
        self.set_camera_size(x, y, zoom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_frames_world_into_ndc() {
        let size = (800, 600);
        let (sx, sy) = world_to_screen((100.0, 200.0), (100.0, 200.0), 1.0, size);
        assert_eq!((sx, sy), (400.0, 300.0));
        let (nx, ny) = screen_to_ndc((sx, sy), size);
        assert!((nx.abs() < 1e-6) && (ny.abs() < 1e-6));
        let (sx2, _) = world_to_screen((110.0, 200.0), (100.0, 200.0), 2.0, size);
        assert!((sx2 - 420.0).abs() < 1e-6);
    }

    #[test]
    fn quad_is_centered_and_textured() {
        let quad = quad_for((0.0, 0.0), 32.0, 32.0, (800, 600), FULL_UV);
        assert_eq!(quad.len(), 4);
        let xs: Vec<f32> = quad.iter().map(|v| v.pos[0]).collect();
        assert!((xs[0] + 0.02).abs() < 1e-6 && (xs[1] - 0.02).abs() < 1e-6);
        assert_eq!(quad[0].uv, [0.0, 0.0]);
        assert_eq!(quad[2].uv, [1.0, 1.0]);
    }

    #[test]
    fn screen_to_world_inverts_projection() {
        let size = (800, 600);
        let world = (150.0, -40.0);
        let screen = world_to_screen(world, (100.0, 200.0), 2.0, size);
        let back = screen_to_world(screen, (100.0, 200.0), 2.0, size);
        assert!((back.0 - world.0).abs() < 1e-3 && (back.1 - world.1).abs() < 1e-3);
        let center = screen_to_world((400.0, 300.0), (100.0, 200.0), 1.0, size);
        assert!((center.0 - 100.0).abs() < 1e-9 && (center.1 - 200.0).abs() < 1e-9);
    }

    #[test]
    fn demo_textures_decode() {
        let player = image::open("../examples/minimal-2d/assets/player.png").unwrap();
        assert_eq!((player.width(), player.height()), (32, 32));
    }

    #[test]
    fn sprite_renders_green_pixel_offscreen() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let Some(adapter) = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: true,
            },
        )) else {
            eprintln!("SKIP: no fallback GPU adapter on this machine");
            return;
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
                .unwrap();
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let (tex_layout, pipeline, sampler) = create_sprite_pipeline(&device, format);

        let rgba = image::open("../examples/minimal-2d/assets/player.png")
            .unwrap()
            .to_rgba8();
        let sprite = upload(&device, &queue, &rgba, 32, 32);
        let sprite_view = sprite.create_view(&Default::default());
        let sprite_bg = bind_group(&device, &tex_layout, &sampler, &sprite_view);

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pite test target"),
            size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&Default::default());
        let quad = [
            SpriteVertex { pos: [-1.0, 1.0], uv: [0.0, 0.0] },
            SpriteVertex { pos: [1.0, 1.0], uv: [1.0, 0.0] },
            SpriteVertex { pos: [1.0, -1.0], uv: [1.0, 1.0] },
            SpriteVertex { pos: [-1.0, -1.0], uv: [0.0, 1.0] },
        ];
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite test verts"),
            contents: bytemuck::cast_slice(&quad),
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
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &sprite_bg, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.draw(0..4, 0..1);
        }
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pite test readback"),
            size: 64 * 64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &readback,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(64 * 4),
                    rows_per_image: Some(64),
                },
            },
            wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();
        let center: [u8; 4];
        {
            let view = readback.slice(..).get_mapped_range();
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
    fn baked_text_renders_bright_pixels_offscreen() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let Some(adapter) = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: true,
            },
        )) else {
            eprintln!("SKIP: no fallback GPU adapter on this machine");
            return;
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
                .unwrap();
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let (tex_layout, pipeline, sampler) = create_sprite_pipeline(&device, format);

        let atlas = text::TextAtlas::new().unwrap();
        let baked = atlas.bake("HP", 32.0, [255, 255, 255, 255]);
        let glyph = upload(&device, &queue, &baked.rgba, baked.w, baked.h);
        let glyph_view = glyph.create_view(&Default::default());
        let glyph_bg = bind_group(&device, &tex_layout, &sampler, &glyph_view);

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pite text test target"),
            size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&Default::default());
        let quad = [
            SpriteVertex { pos: [-1.0, 1.0], uv: [0.0, 0.0] },
            SpriteVertex { pos: [1.0, 1.0], uv: [1.0, 0.0] },
            SpriteVertex { pos: [1.0, -1.0], uv: [1.0, 1.0] },
            SpriteVertex { pos: [-1.0, -1.0], uv: [0.0, 1.0] },
        ];
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pite text test verts"),
            contents: bytemuck::cast_slice(&quad),
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
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.2, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &glyph_bg, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.draw(0..4, 0..1);
        }
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pite text test readback"),
            size: 64 * 64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &readback,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(64 * 4),
                    rows_per_image: Some(64),
                },
            },
            wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();
        let bright = {
            let view = readback.slice(..).get_mapped_range();
            view.chunks(4)
                .filter(|px| px[0] > 200 && px[1] > 200 && px[2] > 200)
                .count()
        };
        readback.unmap();
        assert!(bright > 10, "expected bright glyph pixels, got {bright}");
    }
}

#[cfg(test)]
mod atlas_draw_tests {
    use super::*;
    use std::collections::HashMap;

    fn sheet_frame(name: &str) -> (Atlas, AtlasFrame) {
        let atlas = atlas::parse_atlas(
            r#"{"texture":"sheet.png","size":[64,32],
                "frames":{"player":{"x":0,"y":0,"w":32,"h":32},
                          "enemy":{"x":32,"y":0,"w":32,"h":32}}}"#,
        )
        .unwrap();
        let frame = atlas.frame(name).unwrap();
        (atlas, frame)
    }

    fn queued(atlas: &Atlas, frame: AtlasFrame, world: (f64, f64)) -> QueuedSprite {
        QueuedSprite {
            tex_key: "sheet.png".to_string(),
            world,
            size_world: None,
            size_px: Some((frame.w, frame.h)),
            uv: frame.uv(atlas.size),
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
        let sizes: HashMap<String, (u32, u32)> =
            HashMap::from([("sheet.png".to_string(), atlas.size)]);
        let (groups, order) = build_groups(sprites, &sizes, (0.0, 0.0), 1.0, (200, 100));

        assert_eq!(order, vec!["sheet.png".to_string()], "one sheet, one draw");
        let verts = groups.get("sheet.png").expect("group exists");
        assert_eq!(verts.len(), 8, "two quads share the group");

        let player_uvs: Vec<[f32; 2]> = verts[..4].iter().map(|v| v.uv).collect();
        assert_eq!(player_uvs[0], [0.0, 0.0]);
        assert_eq!(player_uvs[2], [0.5, 1.0]);
        let enemy_uvs: Vec<[f32; 2]> = verts[4..].iter().map(|v| v.uv).collect();
        assert_eq!(enemy_uvs[0], [0.5, 0.0]);
        assert_eq!(enemy_uvs[2], [1.0, 1.0]);
    }

    #[test]
    fn frame_quad_uses_frame_size_not_sheet_size() {
        let (atlas, player) = sheet_frame("player");
        let sprites = vec![queued(&atlas, player, (0.0, 0.0))];
        let sizes: HashMap<String, (u32, u32)> =
            HashMap::from([("sheet.png".to_string(), atlas.size)]);
        let (groups, _) = build_groups(sprites, &sizes, (0.0, 0.0), 1.0, (200, 100));
        let verts = &groups["sheet.png"];
        let width_ndc = verts[1].pos[0] - verts[0].pos[0];
        // 32px frame in a 200px-wide view, doubled because pos spans both edges.
        assert!((width_ndc - 32.0 / 200.0).abs() < 1e-6, "got {width_ndc}");
    }

    #[test]
    fn missing_sidecar_and_frame_are_loud() {
        let mut cache: HashMap<String, Atlas> = HashMap::new();
        let err = frame_uv(&mut cache, "/nowhere/sheet.png", "player").unwrap_err();
        assert!(format!("{err:#}").contains("cannot read atlas"), "got {err:#}");
    }
}
