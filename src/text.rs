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
const FACE: &str = "DejaVu Sans Mono";

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
    wide: u32,
    high: u32,
}

impl Text {
    fn fonts() -> FontSystem {
        let mut db = glyphon::fontdb::Database::new();
        db.load_font_data(include_bytes!("../assets/Vazirmatn-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../assets/Vazirmatn-Bold.ttf").to_vec());
        db.load_font_data(include_bytes!("../assets/DejaVuSansMono.ttf").to_vec());
        db.load_font_data(include_bytes!("../assets/DejaVuSansMono-Bold.ttf").to_vec());
        for path in Self::EXTRA {
            let _ = db.load_font_file(path);
        }
        let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en-US"));
        FontSystem::new_with_locale_and_db(locale, db)
    }

    const EXTRA: &[&str] = &[
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
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
    ) -> Self {
        let mut font = Self::fonts();
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
            family: Family::Name(FACE),
            ..Attrs::new()
        };
        let mut spans: Vec<(&str, Attrs)> = Vec::new();
        for (n, line) in lines.iter().enumerate() {
            if n > 0 {
                spans.push((
                    "\n",
                    Attrs {
                        family: Family::Name(FACE),
                        ..Attrs::new()
                    },
                ));
            }
            for span in line {
                spans.push((
                    span.text.as_str(),
                    Attrs {
                        family: Family::Name(FACE),
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
        let mut font = super::Text::fonts();
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
        let font = super::Text::fonts();
        let found = font.db().query(&fontdb::Query {
            families: &[fontdb::Family::Name("DejaVu Sans Mono")],
            ..Default::default()
        });
        assert!(found.is_some());
    }

    #[test]
    fn emoji_resolves() {
        let font = super::Text::fonts();
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
}
