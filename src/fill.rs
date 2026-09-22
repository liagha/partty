use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, Buffer, BufferBindingType, BufferDescriptor, BufferUsages,
    ColorTargetState, ColorWrites, FragmentState, MultisampleState, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PrimitiveState, RenderPass, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, TextureFormat, VertexAttribute,
    VertexBufferLayout, VertexFormat, VertexState, VertexStepMode,
};
use wgpu::{Device, Queue};

use crate::color::flat;
use crate::grid::Span;
use crate::text::Geo;

const MAX: usize = 2048;

const SHADER: &str = r#"
struct U { size: vec2<f32>, };
@group(0) @binding(0) var<uniform> u: U;
struct V { @location(0) pos: vec2<f32>, @location(1) col: vec4<f32>, };
struct O { @builtin(position) p: vec4<f32>, @location(0) c: vec4<f32>, };
@vertex fn vs(v: V) -> O {
    var o: O;
    o.p = vec4<f32>(v.pos.x / u.size.x * 2.0 - 1.0, 1.0 - v.pos.y / u.size.y * 2.0, 0.0, 1.0);
    o.c = v.col;
    return o;
}
@fragment fn fs(o: O) -> @location(0) vec4<f32> { return o.c; }
"#;

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rgb: [f32; 3],
}

pub fn rects(lines: &[Vec<Span>], geo: &Geo) -> Vec<Rect> {
    let mut out = Vec::new();
    for (r, line) in lines.iter().enumerate() {
        for span in line {
            let cols = span.text.chars().count() as f32;
            if cols <= 0.0 {
                continue;
            }
            let x = geo.pad + span.col as f32 * geo.adv * geo.scale;
            let w = cols * geo.adv * geo.scale;
            let top = geo.pad + r as f32 * geo.step * geo.scale;
            if let Some(back) = span.back {
                out.push(Rect {
                    x,
                    y: top,
                    w,
                    h: geo.step * geo.scale,
                    rgb: flat(back),
                });
            }
            let bar = geo.scale.max(1.0);
            if span.under {
                out.push(Rect {
                    x,
                    y: top + geo.step * geo.scale - 3.0 * geo.scale,
                    w,
                    h: bar,
                    rgb: flat(span.hue),
                });
            }
            if span.strike {
                out.push(Rect {
                    x,
                    y: top + (geo.step * geo.scale - bar) / 2.0,
                    w,
                    h: bar,
                    rgb: flat(span.hue),
                });
            }
        }
    }
    out
}

pub struct Fill {
    pipe: RenderPipeline,
    verts: Buffer,
    uni: Buffer,
    group: BindGroup,
    count: u32,
}

impl Fill {
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
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
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
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uni.as_entire_binding(),
            }],
        });
        let pipe = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(
                &device.create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&layout)],
                    ..Default::default()
                }),
            ),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 24,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x4,
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
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let verts = device.create_buffer(&BufferDescriptor {
            label: None,
            size: (MAX * 6 * 24) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipe,
            verts,
            uni,
            group,
            count: 0,
        }
    }

    pub fn paint(&mut self, queue: &Queue, boxes: &[Rect], width: u32, height: u32) {
        let mut uni = Vec::with_capacity(8);
        uni.extend_from_slice(&(width as f32).to_ne_bytes());
        uni.extend_from_slice(&(height as f32).to_ne_bytes());
        queue.write_buffer(&self.uni, 0, &uni);
        let n = boxes.len().min(MAX);
        let mut raw = Vec::with_capacity(n * 6 * 24);
        for b in &boxes[..n] {
            let corners = [
                (b.x, b.y),
                (b.x + b.w, b.y),
                (b.x + b.w, b.y + b.h),
                (b.x, b.y),
                (b.x + b.w, b.y + b.h),
                (b.x, b.y + b.h),
            ];
            for (px, py) in corners {
                raw.extend_from_slice(&px.to_ne_bytes());
                raw.extend_from_slice(&py.to_ne_bytes());
                for c in b.rgb {
                    raw.extend_from_slice(&c.to_ne_bytes());
                }
                raw.extend_from_slice(&1.0f32.to_ne_bytes());
            }
        }
        queue.write_buffer(&self.verts, 0, &raw);
        self.count = (n * 6) as u32;
    }

    pub fn draw<'a>(&'a self, pass: &mut RenderPass<'a>) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipe);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, self.verts.slice(..));
        pass.draw(0..self.count, 0..1);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::color::Color;

    #[test]
    fn boxes() {
        let geo = Geo {
            adv: 10.0,
            step: 20.0,
            pad: 5.0,
            scale: 2.0,
        };
        let lines = vec![vec![
            Span {
                hue: Color::rgb(255, 255, 255),
                back: None,
                text: "ab".into(),
                under: false,
                strike: false,
                col: 0,
            },
            Span {
                hue: Color::rgb(255, 255, 255),
                back: Some(Color::rgb(255, 0, 0)),
                text: "hi".into(),
                under: false,
                strike: false,
                col: 2,
            },
        ]];
        let out = rects(&lines, &geo);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].x, out[0].y, out[0].w, out[0].h), (45.0, 5.0, 40.0, 40.0));
        assert_eq!(out[0].rgb, [1.0, 0.0, 0.0]);
    }
}
