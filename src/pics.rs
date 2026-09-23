use std::collections::HashMap;
use wgpu::{
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState, Buffer,
    BufferBindingType, BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, Device,
    Extent3d, FilterMode, FragmentState, MultisampleState, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPass, RenderPipeline,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, TexelCopyBufferLayout, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureViewDescriptor,
    VertexAttribute, VertexBufferLayout, VertexFormat, VertexState, VertexStepMode,
};

use crate::grid::Pic;
use crate::text::Geo;

const MAX: usize = 512;

const SHADER: &str = r#"
struct U { size: vec2<f32>, };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var t: texture_2d<f32>;
@group(1) @binding(1) var s: sampler;
struct V { @location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, };
struct O { @builtin(position) p: vec4<f32>, @location(0) c: vec2<f32>, };
@vertex fn vs(v: V) -> O {
    var o: O;
    o.p = vec4<f32>(v.pos.x / u.size.x * 2.0 - 1.0, 1.0 - v.pos.y / u.size.y * 2.0, 0.0, 1.0);
    o.c = v.uv;
    return o;
}
@fragment fn fs(o: O) -> @location(0) vec4<f32> { return textureSample(t, s, o.c); }
"#;

#[derive(Clone)]
pub struct Draw {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub id: u32,
}

pub fn rects(
    pics: &[Pic],
    dims: &dyn Fn(u32) -> Option<(u32, u32)>,
    geo: &Geo,
) -> Vec<Draw> {
    let mut out = Vec::new();
    for p in pics {
        let Some((iw, ih)) = dims(p.id) else {
            continue;
        };
        if iw == 0 || ih == 0 {
            continue;
        }
        let adv = geo.adv * geo.scale;
        let step = geo.step * geo.scale;
        let x = geo.pad + p.col as f32 * adv;
        let y = geo.pad + p.row as f32 * step;
        let w = p.cols as f32 * adv;
        let h = p.rows as f32 * step;
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        let s = (w / iw as f32).min(h / ih as f32);
        let (dw, dh) = (iw as f32 * s, ih as f32 * s);
        let (mut x0, mut y0) = (x + (w - dw) / 2.0, y + (h - dh) / 2.0);
        let (mut x1, mut y1) = (x0 + dw, y0 + dh);
        let (mut u0, mut v0, mut u1, mut v1) = (0.0, 0.0, 1.0, 1.0);
        if x0 < 0.0 {
            u0 = -x0 / dw;
            x0 = 0.0;
        }
        if y0 < 0.0 {
            v0 = -y0 / dh;
            y0 = 0.0;
        }
        if x1 > geo.wide {
            u1 = 1.0 - (x1 - geo.wide) / dw;
            x1 = geo.wide;
        }
        if y1 > geo.high {
            v1 = 1.0 - (y1 - geo.high) / dh;
            y1 = geo.high;
        }
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        out.push(Draw {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
            u0,
            v0,
            u1,
            v1,
            id: p.id,
        });
    }
    out
}

struct Tile {
    group: BindGroup,
}

pub struct Pics {
    pipe: RenderPipeline,
    verts: Buffer,
    uni: Buffer,
    ugroup: BindGroup,
    tlayout: BindGroupLayout,
    sampler: Sampler,
    tiles: HashMap<u32, Tile>,
    draws: Vec<Draw>,
    count: u32,
}

impl Pics {
    pub fn open(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(SHADER.into()),
        });
        let uni = device.create_buffer(&BufferDescriptor {
            label: None,
            size: 8,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ulayout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let ugroup = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &ulayout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uni.as_entire_binding(),
            }],
        });
        let tlayout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: None,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });
        let pipe = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(
                &device.create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&ulayout), Some(&tlayout)],
                    ..Default::default()
                }),
            ),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 16,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                })],
                compilation_options: PipelineCompilationOptions::default(),
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let verts = device.create_buffer(&BufferDescriptor {
            label: None,
            size: (MAX * 6 * 16) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipe,
            verts,
            uni,
            ugroup,
            tlayout,
            sampler,
            tiles: HashMap::new(),
            draws: Vec::new(),
            count: 0,
        }
    }

    pub fn has(&self, id: u32) -> bool {
        self.tiles.contains_key(&id)
    }

    pub fn upload(
        &mut self,
        device: &Device,
        queue: &Queue,
        id: u32,
        w: u32,
        h: u32,
        rgba: &[u8],
    ) {
        if w == 0 || h == 0 || rgba.len() != w as usize * h as usize * 4 {
            return;
        }
        let texture = device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            rgba,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&TextureViewDescriptor::default());
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.tlayout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.tiles.insert(id, Tile { group });
    }

    pub fn paint(&mut self, queue: &Queue, draws: &[Draw], width: u32, height: u32) {
        let mut uni = Vec::with_capacity(8);
        uni.extend_from_slice(&(width as f32).to_ne_bytes());
        uni.extend_from_slice(&(height as f32).to_ne_bytes());
        queue.write_buffer(&self.uni, 0, &uni);
        let ids: Vec<u32> = draws.iter().map(|d| d.id).collect();
        self.tiles.retain(|id, _| ids.contains(id));
        let n = draws.len().min(MAX);
        let mut raw = Vec::with_capacity(n * 6 * 16);
        for d in &draws[..n] {
            let corners = [
                (d.x, d.y, d.u0, d.v0),
                (d.x + d.w, d.y, d.u1, d.v0),
                (d.x + d.w, d.y + d.h, d.u1, d.v1),
                (d.x, d.y, d.u0, d.v0),
                (d.x + d.w, d.y + d.h, d.u1, d.v1),
                (d.x, d.y + d.h, d.u0, d.v1),
            ];
            for (px, py, u, v) in corners {
                raw.extend_from_slice(&px.to_ne_bytes());
                raw.extend_from_slice(&py.to_ne_bytes());
                raw.extend_from_slice(&u.to_ne_bytes());
                raw.extend_from_slice(&v.to_ne_bytes());
            }
        }
        queue.write_buffer(&self.verts, 0, &raw);
        self.draws = draws[..n].to_vec();
        self.count = (n * 6) as u32;
    }

    pub fn draw<'a>(&'a self, pass: &mut RenderPass<'a>) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipe);
        pass.set_bind_group(0, &self.ugroup, &[]);
        for (i, d) in self.draws.iter().enumerate() {
            let Some(tile) = self.tiles.get(&d.id) else {
                continue;
            };
            pass.set_bind_group(1, &tile.group, &[]);
            pass.set_vertex_buffer(0, self.verts.slice((i * 6 * 16) as u64..((i + 1) * 6 * 16) as u64));
            pass.draw(0..6, 0..1);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::grid::Pic;

    fn geo() -> Geo {
        Geo {
            adv: 10.0,
            step: 20.0,
            pad: 5.0,
            scale: 1.0,
            wide: 200.0,
            high: 100.0,
        }
    }

    #[test]
    fn draw_maps_cells() {
        let pics = vec![Pic {
            id: 3,
            row: 1,
            col: 2,
            rows: 2,
            cols: 3,
            z: 0,
        }];
        let out = rects(&pics, &|_| Some((30, 40)), &geo());
        assert_eq!(out.len(), 1);
        let d = &out[0];
        assert_eq!((d.x, d.y, d.w, d.h), (25.0, 25.0, 30.0, 40.0));
        assert_eq!((d.u0, d.v0, d.u1, d.v1), (0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn draw_scales() {
        let pics = vec![Pic {
            id: 3,
            row: 1,
            col: 2,
            rows: 2,
            cols: 3,
            z: 0,
        }];
        let geo = Geo {
            scale: 2.0,
            pad: 10.0,
            wide: 400.0,
            high: 200.0,
            ..geo()
        };
        let out = rects(&pics, &|_| Some((30, 40)), &geo);
        assert_eq!(out.len(), 1);
        let d = &out[0];
        assert_eq!((d.x, d.y, d.w, d.h), (50.0, 50.0, 60.0, 80.0));
    }

    #[test]
    fn draw_clips_window() {
        let pics = vec![Pic {
            id: 3,
            row: -1,
            col: 0,
            rows: 2,
            cols: 20,
            z: 0,
        }];
        let out = rects(&pics, &|_| Some((400, 40)), &geo());
        assert_eq!(out.len(), 1);
        let d = &out[0];
        assert_eq!((d.x, d.y), (5.0, 0.0));
        assert_eq!((d.u0, d.v0), (0.0, 0.25));
        assert_eq!(d.id, 3);
    }
}
