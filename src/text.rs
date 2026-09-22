use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Wrap,
};
use wgpu::{Device, MultisampleState, Queue, RenderPass, TextureFormat};

const DEMO: &str = "سلام دنیا!\nمی‌خواهم یک ترمینال فارسی بسازم\nاعداد فارسی: ۰۱۲۳۴۵۶۷۸۹\npartty نسخه ۰.۱ — hello سلام 123\nلا لام‌الف، کتاب‌ها، تهران\nاین یک پاراگراف طولانی فارسی است برای آزمایش شکستن خط و چیدمان راست‌به‌چپ در پنجره با عرض‌های مختلف\nThe quick brown fox jumps over ۱۲۳";
const SIZE: f32 = 30.0;
const PAD: f32 = 24.0;

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

    pub fn open(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Self {
        let mut font = FontSystem::new();
        let swash = SwashCache::new();
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, MultisampleState::default(), None);
        let viewport = Viewport::new(device, &cache);
        let mut buffer = Buffer::new(&mut font, Metrics::relative(SIZE, 1.5));
        buffer.set_wrap(Wrap::Word);
        let (inner_w, inner_h, _) = Self::layout(width, height, scale);
        buffer.set_size(Some(inner_w), Some(inner_h));
        let attrs = Attrs {
            family: Family::Name("Vazirmatn"),
            ..Attrs::new()
        };
        buffer.set_text(DEMO, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font, false);
        Self {
            font,
            swash,
            atlas,
            renderer,
            viewport,
            buffer,
            _cache: cache,
            scale,
        }
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
        let (inner_w, inner_h, _) = Self::layout(width, height, scale);
        self.buffer.set_size(Some(inner_w), Some(inner_h));
        self.buffer.shape_until_scroll(&mut self.font, false);
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
        let mut font = FontSystem::new();
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
}
