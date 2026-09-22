use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Wrap,
};
use wgpu::{Device, MultisampleState, Queue, RenderPass, TextureFormat};

use crate::color::FORE;
use crate::grid::Span;

const DEMO: &str = "سلام دنیا!\nمی‌خواهم یک ترمینال فارسی بسازم\nاعداد فارسی: ۰۱۲۳۴۵۶۷۸۹\npartty نسخه ۰.۱ — hello سلام 123\nلا لام‌الف، کتاب‌ها، تهران\nاین یک پاراگراف طولانی فارسی است برای آزمایش شکستن خط و چیدمان راست‌به‌چپ در پنجره با عرض‌های مختلف\nThe quick brown fox jumps over ۱۲۳";
const SIZE: f32 = 30.0;
const PAD: f32 = 24.0;
const LINE: f32 = SIZE * 1.5;
const ADV: f32 = SIZE * 0.6;

pub struct Text {
    font: FontSystem,
    swash: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
    _cache: Cache,
    scale: f32,
}

impl Text {
    fn fonts() -> FontSystem {
        let mut db = glyphon::fontdb::Database::new();
        db.load_font_data(include_bytes!("../assets/Vazirmatn-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../assets/DejaVuSansMono.ttf").to_vec());
        let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en-US"));
        FontSystem::new_with_locale_and_db(locale, db)
    }

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

    pub fn cells(width: u32, height: u32, scale: f32) -> (usize, usize) {
        let (inner_w, inner_h, _) = Self::layout(width, height, scale);
        let rows = (inner_h / LINE).floor().max(1.0) as usize;
        let cols = (inner_w / ADV).floor().max(1.0) as usize;
        (rows, cols)
    }

    fn advance(width: u32, height: u32, scale: f32) -> f32 {
        let (_, inner_h, _) = Self::layout(width, height, scale);
        let (rows, _) = Self::cells(width, height, scale);
        inner_h / rows.max(1) as f32
    }

    fn fit(&mut self, width: u32, height: u32) {
        let (inner_w, inner_h, _) = Self::layout(width, height, self.scale);
        self.buffer.set_metrics(Metrics {
            font_size: SIZE,
            line_height: Self::advance(width, height, self.scale),
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
    ) -> Self {
        let mut font = Self::fonts();
        let swash = SwashCache::new();
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, MultisampleState::default(), None);
        let viewport = Viewport::new(device, &cache);
        let mut buffer = Buffer::new(&mut font, Metrics::relative(SIZE, 1.5));
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
        };
        text.fit(width, height);
        let demo: Vec<Vec<Span>> = DEMO
            .lines()
            .map(|line| {
                vec![Span {
                    hue: Color::rgb(FORE[0], FORE[1], FORE[2]),
                    text: line.to_string(),
                }]
            })
            .collect();
        text.show(&demo);
        text
    }

    pub fn show(&mut self, lines: &[Vec<Span>]) {
        let plain = Attrs {
            family: Family::Name("Vazirmatn"),
            ..Attrs::new()
        };
        let mut spans: Vec<(&str, Attrs)> = Vec::new();
        for (n, line) in lines.iter().enumerate() {
            if n > 0 {
                spans.push((
                    "\n",
                    Attrs {
                        family: Family::Name("Vazirmatn"),
                        ..Attrs::new()
                    },
                ));
            }
            for span in line {
                spans.push((
                    span.text.as_str(),
                    Attrs {
                        family: Family::Name("Vazirmatn"),
                        color_opt: Some(span.hue),
                        ..Attrs::new()
                    },
                ));
            }
        }
        self.buffer
            .set_rich_text(spans.into_iter(), &plain, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font, false);
    }

    pub fn resize(&mut self, queue: &Queue, width: u32, height: u32, scale: f32) {
        self.scale = scale;
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
    fn cells_fit() {
        assert_eq!(Text::cells(1920, 1057, 1.0), (22, 104));
    }

    #[test]
    fn advance_fills() {
        let (rows, _) = Text::cells(1920, 1057, 1.0);
        let box_h = 1057.0 - 2.0 * PAD;
        assert!((Text::advance(1920, 1057, 1.0) * rows as f32 - box_h).abs() < 0.01);
    }
}
