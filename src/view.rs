use std::sync::Arc;
use wgpu::{
    Adapter, CompositeAlphaMode, Device, PresentMode, Queue, Surface, SurfaceColorSpace,
    SurfaceConfiguration, TextureFormat, TextureUsages,
};
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::fill::Fill;
use crate::grid::{Pic, Span};
use crate::pics::Pics;
use crate::text::{Geo, Text};

pub struct View {
    instance: wgpu::Instance,
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    text: Text,
    fill: Fill,
    pics: Pics,
    window: Arc<Window>,
    bg: wgpu::Color,
    geo: Option<Geo>,
}

impl View {
    fn adapter(instance: &wgpu::Instance, surface: &Surface) -> Option<Adapter> {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .ok()
    }

    pub fn open(
        window: Arc<Window>,
        loop_: &ActiveEventLoop,
        bg: [f64; 3],
        font: f32,
        face: &str,
        files: &[std::path::PathBuf],
    ) -> Self {
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let mut kind = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
            loop_.owned_display_handle(),
        ));
        kind.backends = wgpu::Backends::GL.with_env();
        let mut instance = wgpu::Instance::new(kind);
        let mut surface = instance.create_surface(window.clone()).unwrap();
        let mut adapter = Self::adapter(&instance, &surface);
        if adapter.is_none() {
            eprintln!("partty: gl unavailable, trying all backends");
            instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            surface = instance.create_surface(window.clone()).unwrap();
            adapter = Self::adapter(&instance, &surface);
        }
        let adapter = adapter.unwrap();
        let info = adapter.get_info();
        eprintln!("partty: {} ({:?})", info.name, info.backend);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: Default::default(),
            trace: Default::default(),
        }))
        .unwrap();
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: TextureFormat::Bgra8UnormSrgb,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::AutoVsync,
            alpha_mode: CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        let text = Text::open(
            &device,
            &queue,
            config.format,
            size.width.max(1),
            size.height.max(1),
            scale,
            font,
            face,
            files,
        );
        let fill = Fill::open(&device, config.format);
        let pics = Pics::open(&device, config.format);
        Self {
            instance,
            surface,
            device,
            queue,
            config,
            text,
            fill,
            pics,
            window,
            bg: wgpu::Color {
                r: bg[0],
                g: bg[1],
                b: bg[2],
                a: 1.0,
            },
            geo: None,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f32) {        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.text.resize(&self.queue, width.max(1), height.max(1), scale);
    }

    pub fn show(&mut self, lines: &[Vec<Span>]) -> Geo {
        let geo = self.text.show(lines);
        self.geo = Some(geo);
        let boxes = crate::fill::rects(lines, &geo);
        self.fill
            .paint(&self.queue, &boxes, self.config.width, self.config.height);
        geo
    }

    pub fn pics<'a>(
        &mut self,
        pics: &[Pic],
        get: &'a dyn Fn(u32) -> Option<(u32, u32, &'a [u8])>,
    ) {
        let Some(geo) = self.geo else {
            return;
        };
        for p in pics {
            if !self.pics.has(p.id) {
                if let Some((w, h, bytes)) = get(p.id) {
                    self.pics.upload(&self.device, &self.queue, p.id, w, h, bytes);
                }
            }
        }
        let draws = crate::pics::rects(pics, &|id| get(id).map(|(w, h, _)| (w, h)), &geo);
        self.pics.paint(&self.queue, &draws, self.config.width, self.config.height);
    }

    pub fn spot(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let geo = self.geo?;
        let adv = geo.adv * geo.scale;
        let step = geo.step * geo.scale;
        if adv <= 0.0 || step <= 0.0 {
            return None;
        }
        let across = (x - geo.pad) / adv;
        let down = (y - geo.pad) / step;
        if across < -1.0 || down < -1.0 {
            return None;
        }
        Some((down.floor().max(0.0) as usize, across.floor().max(0.0) as usize))
    }

    pub fn draw(&mut self) {
        let (width, height) = (self.config.width, self.config.height);
        self.text.prepare(&self.device, &self.queue, width, height);
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Suboptimal(_) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self.instance.create_surface(self.window.clone()).unwrap();
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => panic!("validation error"),
        };
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: None,
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.bg),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.fill.draw(&mut pass);
            self.pics.below(&mut pass);
            self.text.render(&mut pass);
            self.pics.above(&mut pass);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        self.text.trim();
    }
}
