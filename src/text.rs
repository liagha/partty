use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use wgpu::{Device, MultisampleState, Queue, RenderPass, TextureFormat};

use crate::color::FORE;
use crate::grid::Span;

const DEMO: &str = "سلام دنیا!\nمی‌خواهم یک ترمینال فارسی بسازم\nاعداد فارسی: ۰۱۲۳۴۵۶۷۸۹\npartty نسخه ۰.۱ — hello سلام 123\nلا لام‌الف، کتاب‌ها، تهران\nاین یک پاراگراف طولانی فارسی است برای آزمایش شکستن خط و چیدمان راست‌به‌چپ در پنجره با عرض‌های مختلف\nThe quick brown fox jumps over ۱۲۳";
pub(crate) const SIZE: f32 = 30.0;
const PAD: f32 = 24.0;
pub(crate) const FACE: &str = "DejaVu Sans Mono";

#[derive(Clone, Copy)]
pub struct Geo {
    pub adv: f32,
    pub step: f32,
    pub pad: f32,
    pub scale: f32,
    pub wide: f32,
    pub high: f32,
}

pub struct Text {
    font: FontSystem,
    swash: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
    _cache: Cache,
    scale: f32,
    size: f32,
    face: String,
    wide: u32,
    high: u32,
}

impl Text {
    fn fonts(files: &[std::path::PathBuf]) -> FontSystem {
        let mut db = glyphon::fontdb::Database::new();
        db.load_font_data(include_bytes!("../assets/Vazirmatn-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../assets/Vazirmatn-Bold.ttf").to_vec());
        db.load_font_data(include_bytes!("../assets/DejaVuSansMono.ttf").to_vec());
        db.load_font_data(include_bytes!("../assets/DejaVuSansMono-Bold.ttf").to_vec());
        for path in Self::EXTRA {
            let _ = db.load_font_file(Self::expand(std::path::Path::new(path)));
        }
        for path in files {
            let _ = db.load_font_file(Self::expand(path));
        }
        let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en-US"));
        FontSystem::new_with_locale_and_db(locale, db)
    }

    fn expand(path: &std::path::Path) -> std::path::PathBuf {
        match path.strip_prefix("~") {
            Ok(rest) => std::env::var_os("HOME")
                .map(|home| std::path::PathBuf::from(home).join(rest))
                .unwrap_or_else(|| path.to_path_buf()),
            Err(_) => path.to_path_buf(),
        }
    }

    #[cfg(target_os = "macos")]
    const EXTRA: &[&str] = &[
        "/Library/Fonts/NotoSansSymbols2-Regular.ttf",
        "~/Library/Fonts/NotoSansSymbols2-Regular.ttf",
        "/System/Library/Fonts/Apple Color Emoji.ttc",
    ];

    #[cfg(target_os = "windows")]
    const EXTRA: &[&str] = &[
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\seguiemj.ttf",
        "C:\\Windows\\Fonts\\NotoSansSymbols2-Regular.ttf",
    ];

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    const EXTRA: &[&str] = &[
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    ];

    fn layout(width: u32, height: u32, scale: f32) -> (f32, f32, TextBounds) {
        let pad = (PAD * scale).round();
        let inner_w = (width.max(1) as f32 - 2.0 * pad).max(1.0);
        let inner_h = (height.max(1) as f32 - 2.0 * pad).max(1.0);
        let bounds = TextBounds {
            left: pad as i32,
            top: pad as i32,
            right: (pad + inner_w) as i32,
            bottom: (pad + inner_h) as i32,
        };
        (inner_w / scale, inner_h / scale, bounds)
    }

    pub fn cells(width: u32, height: u32, scale: f32, size: f32) -> (usize, usize) {
        let (inner_w, inner_h, _) = Self::layout(width, height, scale);
        let rows = (inner_h / (size * 1.5)).floor().max(1.0) as usize;
        let cols = (inner_w / (size * 0.6)).floor().max(1.0) as usize;
        (rows, cols)
    }

    pub fn advance(width: u32, height: u32, scale: f32, size: f32) -> f32 {
        let (_, inner_h, _) = Self::layout(width, height, scale);
        let (rows, _) = Self::cells(width, height, scale, size);
        inner_h / rows.max(1) as f32
    }

    fn fit(&mut self, width: u32, height: u32) {
        let (inner_w, inner_h, _) = Self::layout(width, height, self.scale);
        self.buffer.set_metrics(Metrics {
            font_size: self.size,
            line_height: Self::advance(width, height, self.scale, self.size),
        });
        self.buffer.set_size(Some(inner_w), Some(inner_h));
        self.buffer.shape_until_scroll(&mut self.font, false);
    }

    pub fn open(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        width: u32,
        height: u32,
        scale: f32,
        size: f32,
        face: &str,
        files: &[std::path::PathBuf],
    ) -> Self {
        let mut font = Self::fonts(files);
        let hit = |name: &str| {
            font.db()
                .query(&glyphon::fontdb::Query {
                    families: &[glyphon::fontdb::Family::Name(name)],
                    ..Default::default()
                })
                .is_some()
        };
        eprintln!(
            "partty: faces jetbrains={} dejavu={} face={face}",
            hit("JetBrains Mono"),
            hit("DejaVu Sans Mono")
        );
        let swash = SwashCache::new();
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, MultisampleState::default(), None);
        let viewport = Viewport::new(device, &cache);
        let mut buffer = Buffer::new(&mut font, Metrics::relative(size, 1.5));
        buffer.set_wrap(Wrap::None);
        let mut text = Self {
            font,
            swash,
            atlas,
            renderer,
            viewport,
            buffer,
            _cache: cache,
            scale,
            size,
            face: face.into(),
            wide: width.max(1),
            high: height.max(1),
        };
        text.fit(width, height);
        let demo: Vec<Vec<Span>> = DEMO
            .lines()
            .map(|line| {
                vec![Span {
                    hue: Color::rgb(FORE[0], FORE[1], FORE[2]),
                    back: None,
                    text: line.to_string(),
                    under: false,
                    strike: false,
                    bold: false,
                    col: 0,
                }]
            })
            .collect();
        text.show(&demo);
        text
    }

    pub fn show(&mut self, lines: &[Vec<Span>]) -> Geo {
        let plain = Attrs {
            family: Family::Name(&self.face),
            ..Attrs::new()
        };
        let mut spans: Vec<(&str, Attrs)> = Vec::new();
        for (n, line) in lines.iter().enumerate() {
            if n > 0 {
                spans.push((
                    "\n",
                    Attrs {
                        family: Family::Name(&self.face),
                        ..Attrs::new()
                    },
                ));
            }
            for span in line {
                spans.push((
                    span.text.as_str(),
                    Attrs {
                        family: Family::Name(&self.face),
                        color_opt: Some(span.hue),
                        weight: if span.bold {
                            Weight::BOLD
                        } else {
                            Weight::NORMAL
                        },
                        ..Attrs::new()
                    },
                ));
            }
        }
        self.buffer
            .set_rich_text(spans.into_iter(), &plain, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font, false);
        let adv = self
            .buffer
            .layout_runs()
            .filter_map(|run| {
                let xs: Vec<f32> = run.glyphs.iter().map(|g| g.x).collect();
                (xs.len() >= 2).then(|| (xs[1] - xs[0]).abs())
            })
            .next()
            .unwrap_or(self.size * 0.6);
        Geo {
            adv,
            step: self.buffer.metrics().line_height,
            pad: PAD * self.scale,
            scale: self.scale,
            wide: self.wide as f32,
            high: self.high as f32,
        }
    }

    pub fn resize(&mut self, queue: &Queue, width: u32, height: u32, scale: f32) {
        self.scale = scale;
        self.wide = width.max(1);
        self.high = height.max(1);
        self.viewport.update(
            queue,
            Resolution {
                width: width.max(1),
                height: height.max(1),
            },
        );
        self.fit(width, height);
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, width: u32, height: u32) {
        self.viewport.update(
            queue,
            Resolution {
                width: width.max(1),
                height: height.max(1),
            },
        );
        let (_, _, bounds) = Self::layout(width, height, self.scale);
        self.renderer
            .prepare(
                device,
                queue,
                &mut self.font,
                &mut self.atlas,
                &self.viewport,
                [TextArea {
                    buffer: &self.buffer,
                    left: PAD * self.scale,
                    top: PAD * self.scale,
                    scale: self.scale,
                    bounds,
                    default_color: Color::rgb(230, 230, 235),
                    custom_glyphs: &[],
                }],
                &mut self.swash,
            )
            .unwrap();
    }

    pub fn render(&self, pass: &mut RenderPass) {
        self.renderer
            .render(&self.atlas, &self.viewport, pass)
            .unwrap();
    }

    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use glyphon::fontdb;

    #[test]
    fn persian_shapes() {
        let mut font = super::Text::fonts(&[]);
        let found = font.db().query(&fontdb::Query {
            families: &[fontdb::Family::Name("Vazirmatn")],
            ..Default::default()
        });
        assert!(found.is_some());
        let mut buffer = Buffer::new(&mut font, Metrics::relative(SIZE, 1.5));
        buffer.set_wrap(Wrap::Word);
        buffer.set_size(Some(800.0), Some(600.0));
        let attrs = Attrs {
            family: Family::Name("Vazirmatn"),
            ..Attrs::new()
        };
        buffer.set_text(DEMO, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font, false);
        let glyphs: usize = buffer.layout_runs().map(|run| run.glyphs.len()).sum();
        assert!(buffer.lines.len() >= 7);
        assert!(glyphs > 100);
    }

    #[test]
    fn bundled_fallback() {
        let font = super::Text::fonts(&[]);
        let found = font.db().query(&fontdb::Query {
            families: &[fontdb::Family::Name("DejaVu Sans Mono")],
            ..Default::default()
        });
        assert!(found.is_some());
    }

    #[test]
    fn emoji_resolves() {
        let font = super::Text::fonts(&[]);
        let found = font.db().query(&fontdb::Query {
            families: &[fontdb::Family::Name("Noto Color Emoji")],
            ..Default::default()
        });
        assert_eq!(found.is_some(), std::path::Path::new(super::Text::EXTRA[0]).exists());
    }

    #[test]
    fn cells_fit() {
        assert_eq!(Text::cells(1920, 1057, 1.0, SIZE), (22, 104));
    }

    #[test]
    fn smaller_font_fits_more() {
        let (rows, cols) = Text::cells(1920, 1057, 1.0, 20.0);
        assert_eq!((rows, cols), (33, 156));
    }

    #[test]
    fn advance_fills() {
        let (rows, _) = Text::cells(1920, 1057, 1.0, SIZE);
        let box_h = 1057.0 - 2.0 * PAD;
        assert!((Text::advance(1920, 1057, 1.0, SIZE) * rows as f32 - box_h).abs() < 0.01);
    }

    #[test]
    fn corner_resolves() {
        let paths = [
            "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
            "/usr/share/fonts/TTF/JetBrainsMono-Bold.ttf",
        ];
        let have = paths.iter().all(|p| std::path::Path::new(p).exists());
        let mut font = Text::fonts(&paths.map(std::path::PathBuf::from));
        for weight in [Weight::NORMAL, Weight::BOLD] {
            let mut attrs = Attrs::new().family(Family::Name("JetBrains Mono"));
            attrs.weight = weight;
            let mut buffer = Buffer::new(&mut font, Metrics::new(SIZE, SIZE * 1.5));
            buffer.set_size(Some(500.0), Some(200.0));
            buffer.set_text("┌", &attrs, Shaping::Advanced, None);
            buffer.shape_until_scroll(&mut font, false);
            let mut out = vec![];
            for run in buffer.layout_runs() {
                for g in run.glyphs {
                    let fam = font
                        .db()
                        .face(g.font_id)
                        .map(|f| f.families.first().map(|(n, _)| n.clone()).unwrap_or("?".into()))
                        .unwrap_or("?".into());
                    out.push((g.glyph_id, fam));
                }
            }
            if have {
                assert_eq!(out.len(), 1);
                assert!(out[0].0 != 0);
                assert_eq!(out[0].1, "JetBrains Mono");
            }
        }
    }

    #[test]
    fn braille_resolves() {
        let mut font = Text::fonts(&[]);
        let mut attrs = Attrs::new().family(Family::Name("JetBrains Mono"));
        attrs.weight = Weight::NORMAL;
        let mut buffer = Buffer::new(&mut font, Metrics::new(SIZE, SIZE * 1.5));
        buffer.set_size(Some(500.0), Some(200.0));
        buffer.set_text("⣀", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font, false);
        let mut out = vec![];
        for run in buffer.layout_runs() {
            for g in run.glyphs {
                let fam = font
                    .db()
                    .face(g.font_id)
                    .map(|f| f.families.first().map(|(n, _)| n.clone()).unwrap_or("?".into()))
                    .unwrap_or("?".into());
                out.push((g.glyph_id, fam));
            }
        }
        if std::path::Path::new("/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf").exists() {
            assert_eq!(out.len(), 1);
            assert!(out[0].0 != 0);
            assert_eq!(out[0].1, "Noto Sans Symbols 2");
        }
    }

    #[test]
    fn expand_home() {
        assert_eq!(
            Text::expand(std::path::Path::new("/a/b")),
            std::path::PathBuf::from("/a/b")
        );
        if let Some(home) = std::env::var_os("HOME") {
            assert_eq!(
                Text::expand(std::path::Path::new("~/f.ttf")),
                std::path::PathBuf::from(home).join("f.ttf")
            );
        }
    }

    #[test]
    fn raster_draws_corners() {
        let paths = [
            "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
            "/usr/share/fonts/TTF/JetBrainsMono-Bold.ttf",
        ]
        .map(std::path::PathBuf::from);
        if !paths.iter().all(|p| p.exists()) {
            return;
        }
        let mut kind = wgpu::InstanceDescriptor::new_without_display_handle();
        kind.backends = wgpu::Backends::GL;
        let instance = wgpu::Instance::new(kind);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .ok()
        .expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: Default::default(),
            trace: Default::default(),
        }))
        .expect("device");
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let mut text = Text::open(
            &device,
            &queue,
            format,
            768,
            600,
            1.0,
            24.0,
            "JetBrains Mono",
            &paths,
        );
        let white = glyphon::Color::rgb(255, 255, 255);
        let lines = vec![vec![crate::grid::Span {
            hue: white,
            back: None,
            text: "┌┐│─╭".into(),
            under: false,
            strike: false,
            bold: false,
            col: 0,
        }]];
        text.show(&lines);
        text.prepare(&device, &queue, 768, 600);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 768,
                height: 600,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: None,
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.07,
                            b: 0.09,
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
            text.render(&mut pass);
        }
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (768 * 600 * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(768 * 4),
                    rows_per_image: Some(600),
                },
            },
            wgpu::Extent3d {
                width: 768,
                height: 600,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = slice.get_mapped_range().expect("map").to_vec();
        let lit = data
            .chunks_exact(4)
            .filter(|px| px[0] > 100 || px[1] > 100 || px[2] > 100)
            .count();
        assert!(lit > 400, "lit pixels: {lit}");
    }
}
