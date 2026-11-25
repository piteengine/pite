// SPDX-License-Identifier: MIT OR Apache-2.0
//! WGPU sprite pipeline, vertex/instance layouts, WGSL shader, and GPU texture upload helpers.

use anyhow::{Context, Result};

pub(crate) const FULL_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct UnitQuadVertex {
    pub(crate) pos: [f32; 2],
    pub(crate) uv: [f32; 2],
}

pub(crate) const UNIT_QUAD_VERTS: [UnitQuadVertex; 4] = [
    UnitQuadVertex {
        pos: [-0.5, -0.5],
        uv: [0.0, 0.0],
    },
    UnitQuadVertex {
        pos: [0.5, -0.5],
        uv: [1.0, 0.0],
    },
    UnitQuadVertex {
        pos: [0.5, 0.5],
        uv: [1.0, 1.0],
    },
    UnitQuadVertex {
        pos: [-0.5, 0.5],
        uv: [0.0, 1.0],
    },
];

pub(crate) const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SpriteInstance {
    pub(crate) center_ndc: [f32; 2],
    pub(crate) size_px: [f32; 2],
    pub(crate) inv_viewport: [f32; 2],
    pub(crate) uv_rect: [f32; 4],
    pub(crate) rotation: f32,
    pub(crate) color: [u8; 4],
}

pub(crate) struct GpuTexture {
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) w: u32,
    pub(crate) h: u32,
}

const SPRITE_SHADER: &str = r#"
struct VertexInput {
    @location(0) quad_pos: vec2<f32>,
    @location(1) unit_uv: vec2<f32>,
};

struct InstanceInput {
    @location(2) center_ndc: vec2<f32>,
    @location(3) size_px: vec2<f32>,
    @location(4) inv_viewport: vec2<f32>,
    @location(5) uv_rect: vec4<f32>,
    @location(6) rotation: f32,
    @location(7) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@group(0) @binding(0) var sprite_tex: texture_2d<f32>;
@group(0) @binding(1) var sprite_smp: sampler;

@vertex
fn vs(vert: VertexInput, inst: InstanceInput) -> VertexOut {
    var out: VertexOut;
    let d_px = vert.quad_pos * inst.size_px;
    let cos_r = cos(inst.rotation);
    let sin_r = sin(inst.rotation);
    let rot_x = d_px.x * cos_r - d_px.y * sin_r;
    let rot_y = d_px.x * sin_r + d_px.y * cos_r;
    let ndc_x = inst.center_ndc.x + rot_x * inst.inv_viewport.x;
    let ndc_y = inst.center_ndc.y - rot_y * inst.inv_viewport.y;
    out.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = inst.uv_rect.xy + vert.unit_uv * (inst.uv_rect.zw - inst.uv_rect.xy);
    out.color = inst.color;
    return out;
}

@fragment
fn fs(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(sprite_tex, sprite_smp, in.uv) * in.color;
}
"#;

pub(crate) fn request_device_for(
    instance: &wgpu::Instance,
    compatible_surface: Option<&wgpu::Surface<'_>>,
    force_fallback_adapter: bool,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: if force_fallback_adapter {
            wgpu::PowerPreference::LowPower
        } else {
            wgpu::PowerPreference::HighPerformance
        },
        compatible_surface,
        force_fallback_adapter,
        apply_limit_buckets: false,
    }))
    .context("no suitable GPU adapter")?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .context("cannot request GPU device")?;
    Ok((adapter, device, queue))
}

pub(crate) fn create_sprite_pipeline(
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
        bind_group_layouts: &[Some(&tex_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pite sprite pipe"),
        layout: Some(&pipe_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UnitQuadVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SpriteInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 3,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 16,
                            shader_location: 4,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 24,
                            shader_location: 5,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 40,
                            shader_location: 6,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Unorm8x4,
                            offset: 44,
                            shader_location: 7,
                        },
                    ],
                }),
            ],
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
        multiview_mask: None,
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

pub(crate) fn bind_group(
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

pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &[u8],
    w: u32,
    h: u32,
) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
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
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * w),
            rows_per_image: Some(h),
        },
        size,
    );
    tex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq)]
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
        rotation: f32,
    ) -> [SpriteVertex; 6] {
        // Rotation must be uniform in pixel space — rotating in NDC would
        // shear for non-square viewports. Compute corners in pixels, rotate,
        // then convert to NDC.
        let cx_px = (center_ndc.0 + 1.0) * size.0 as f32 * 0.5;
        let cy_px = (1.0 - center_ndc.1) * size.1 as f32 * 0.5;
        let hw = w_px * 0.5;
        let hh = h_px * 0.5;
        let (sin_r, cos_r) = rotation.sin_cos();

        let to_ndc = |px: f32, py: f32| -> [f32; 2] {
            [
                px / size.0 as f32 * 2.0 - 1.0,
                1.0 - py / size.1 as f32 * 2.0,
            ]
        };
        let corner = |dx: f32, dy: f32| -> [f32; 2] {
            let rx = cx_px + dx * cos_r - dy * sin_r;
            let ry = cy_px + dx * sin_r + dy * cos_r;
            to_ndc(rx, ry)
        };

        let tl = SpriteVertex {
            pos: corner(-hw, -hh),
            uv: [uv[0], uv[1]],
        };
        let tr = SpriteVertex {
            pos: corner(hw, -hh),
            uv: [uv[2], uv[1]],
        };
        let br = SpriteVertex {
            pos: corner(hw, hh),
            uv: [uv[2], uv[3]],
        };
        let bl = SpriteVertex {
            pos: corner(-hw, hh),
            uv: [uv[0], uv[3]],
        };
        [tl, tr, br, tl, br, bl]
    }

    #[test]
    fn quad_is_centered_and_textured() {
        let quad = quad_for((0.0, 0.0), 32.0, 32.0, (800, 600), FULL_UV, 0.0);
        assert_eq!(quad.len(), 6);
        let to_px = |ndc: f32, size: u32| (ndc + 1.0) / 2.0 * size as f32;
        assert!((to_px(quad[0].pos[0], 800) - 384.0).abs() < 0.01);
        assert!((to_px(quad[1].pos[0], 800) - 416.0).abs() < 0.01);
        assert!((to_px(quad[0].pos[1], 600) - 316.0).abs() < 0.01);
        assert!((to_px(quad[2].pos[1], 600) - 284.0).abs() < 0.01);
        assert_eq!(quad[0].uv, [0.0, 0.0]);
        assert_eq!(quad[2].uv, [1.0, 1.0]);
    }

    #[test]
    fn rotated_quad_has_rotated_corners() {
        // 100x50 sprite centered on a 200x200 canvas. 90° moves the
        // top-left corner from pixel (-50, -25) to (25, -50), i.e. NDC
        // (-0.5, 0.25) → (0.25, 0.5).
        let quad = quad_for(
            (0.0, 0.0),
            100.0,
            50.0,
            (200, 200),
            FULL_UV,
            std::f32::consts::FRAC_PI_2,
        );
        assert!((quad[0].pos[0] - 0.25).abs() < 0.01, "{}", quad[0].pos[0]);
        assert!((quad[0].pos[1] - 0.5).abs() < 0.01, "{}", quad[0].pos[1]);
    }
}
