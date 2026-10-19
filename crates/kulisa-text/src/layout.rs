//! Shaping with cosmic-text and glyph caching in the renderer's atlas.

use std::collections::HashMap;

use cosmic_text::{
    Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, Style as FontStyle, SwashCache,
    SwashContent, Weight, Wrap,
};
use kulisa_core::{Color, Rect, TextureId, Vec2};
use kulisa_render::{Image, Renderer};

use crate::markup::{Rich, Size};

/// Where glyph bitmaps go. [`Renderer`] packs them into its atlas; tests use a fake.
pub trait GlyphSink {
    fn upload(&mut self, image: &Image) -> TextureId;
}

impl GlyphSink for Renderer {
    fn upload(&mut self, image: &Image) -> TextureId {
        self.create_texture(image)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justified,
}

/// Base style of a text box; markup spans override parts of it.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Font family; `None` uses the first loaded sans-serif font.
    pub font: Option<String>,
    /// Font size in design units.
    pub size: f32,
    /// Line height as a multiple of `size`.
    pub line_height: f32,
    pub color: Color,
    /// Wrap width in design units; `None` never wraps.
    pub width: Option<f32>,
    pub align: Align,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: None,
            size: 32.0,
            line_height: 1.4,
            color: Color::WHITE,
            width: None,
            align: Align::Left,
        }
    }
}

/// One positioned glyph with everything the typewriter and effects need.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub texture: TextureId,
    /// Local rectangle in design units (origin: top-left of the text box).
    pub rect: Rect,
    pub color: Color,
    /// Logical character index; drives effect phases.
    pub order: usize,
    /// Typewriter cost before this glyph, in characters (already divided by `{speed}`).
    pub reveal_units: f32,
    /// Seconds of `{w}` pauses before this glyph.
    pub reveal_pause: f32,
    pub wave: Option<f32>,
    pub shake: Option<f32>,
    /// Font size in design units (effects scale with it).
    pub em: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLayout {
    pub glyphs: Vec<Glyph>,
    /// Size of the laid-out text in design units.
    pub size: Vec2,
    pub total_units: f32,
    pub total_pause: f32,
    /// Physical pixels per design unit this layout was rasterized for.
    pub scale: f32,
}

struct Cached {
    texture: TextureId,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

/// Fonts, shaping and the glyph cache.
pub struct TextSystem {
    fonts: FontSystem,
    swash: SwashCache,
    glyphs: HashMap<CacheKey, Option<Cached>>,
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Typewriter timing per character: units before it and pause seconds before it.
fn reveal_table(rich: &Rich) -> Vec<(usize, f32, f32)> {
    let mut table = Vec::with_capacity(rich.text.len());
    let (mut units, mut pause) = (0.0f32, 0.0f32);
    let mut pauses = rich.pauses.iter().peekable();
    let mut spans = rich.spans.iter().peekable();
    for (i, _) in rich.text.char_indices() {
        while pauses.peek().is_some_and(|(at, _)| *at <= i) {
            pause += pauses.next().unwrap().1;
        }
        while spans.peek().is_some_and(|s| s.range.end <= i) {
            spans.next();
        }
        table.push((i, units, pause));
        units += 1.0 / spans.peek().map_or(1.0, |s| s.style.speed);
    }
    table.push((
        rich.text.len(),
        units,
        pause + rich.pauses.iter().filter(|(at, _)| *at >= rich.text.len()).map(|p| p.1).sum::<f32>(),
    ));
    table
}

fn lookup(table: &[(usize, f32, f32)], byte: usize) -> (f32, f32) {
    let i = table.partition_point(|(b, ..)| *b <= byte).saturating_sub(1);
    (table[i].1, table[i].2)
}

impl TextSystem {
    /// A text system with no fonts. Games load theirs with [`TextSystem::load_font`].
    pub fn new() -> Self {
        let fonts = FontSystem::new_with_locale_and_db("en-US".into(), cosmic_text::fontdb::Database::new());
        Self { fonts, swash: SwashCache::new(), glyphs: HashMap::new() }
    }

    /// Loads a TTF/OTF/TTC; returns the family names it provides.
    pub fn load_font(&mut self, bytes: Vec<u8>) -> Vec<String> {
        let db = self.fonts.db_mut();
        let before: Vec<_> = db.faces().map(|f| f.id).collect();
        db.load_font_data(bytes);
        let mut names: Vec<String> = db
            .faces()
            .filter(|f| !before.contains(&f.id))
            .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
            .collect();
        names.dedup();
        names
    }

    /// Adds the operating system's fonts as fallbacks (desktop only; useful for CJK and
    /// emoji during development, but games should ship the fonts they need).
    pub fn load_system_fonts(&mut self) {
        self.fonts.db_mut().load_system_fonts();
    }

    /// Forgets cached glyph textures, e.g. after the renderer was recreated.
    pub fn clear_glyph_cache(&mut self) {
        self.glyphs.clear();
    }

    fn glyph(&mut self, key: CacheKey, sink: &mut dyn GlyphSink) -> Option<&Cached> {
        if !self.glyphs.contains_key(&key) {
            let cached = self.swash.get_image_uncached(&mut self.fonts, key).and_then(|img| {
                let (w, h) = (img.placement.width, img.placement.height);
                if w == 0 || h == 0 {
                    return None;
                }
                let pixels: Vec<u8> = match img.content {
                    SwashContent::Mask => img.data.iter().flat_map(|&a| [255, 255, 255, a]).collect(),
                    SwashContent::SubpixelMask => {
                        img.data.as_chunks::<4>().0.iter().flat_map(|p| [255, 255, 255, p[1]]).collect()
                    }
                    SwashContent::Color => img.data.clone(),
                };
                let texture = sink.upload(&Image::from_straight_rgba(w, h, pixels));
                Some(Cached {
                    texture,
                    left: img.placement.left,
                    top: img.placement.top,
                    width: w,
                    height: h,
                })
            });
            self.glyphs.insert(key, cached);
        }
        self.glyphs[&key].as_ref()
    }

    /// Shapes `rich` with `style` for `scale` physical pixels per design unit.
    pub fn layout(
        &mut self,
        sink: &mut dyn GlyphSink,
        rich: &Rich,
        style: &TextStyle,
        scale: f32,
    ) -> TextLayout {
        let scale = scale.max(0.01);
        let reveal = reveal_table(rich);
        // Ruby needs room above its line.
        let line_height = style.line_height + if rich.ruby.is_empty() { 0.0 } else { 0.5 };
        let px = |size: f32| size * scale;
        let span_size = |s: Size| match s {
            Size::Scale(k) => style.size * k,
            Size::Px(p) => p,
        };

        let families: Vec<Option<String>> =
            rich.spans.iter().map(|s| s.style.font.clone().or(style.font.clone())).collect();
        let attrs_for = |i: usize, size: f32| {
            let s = &rich.spans[i].style;
            let family = families[i].as_deref().map_or(Family::SansSerif, Family::Name);
            Attrs::new()
                .family(family)
                .weight(if s.bold { Weight::BOLD } else { Weight::NORMAL })
                .style(if s.italic { FontStyle::Italic } else { FontStyle::Normal })
                .metrics(Metrics::new(px(size), px(size * line_height)))
                .metadata(i)
        };
        let base_family = style.font.as_deref().map_or(Family::SansSerif, Family::Name);
        let default_attrs = Attrs::new().family(base_family);

        let mut buffer =
            Buffer::new(&mut self.fonts, Metrics::new(px(style.size), px(style.size * line_height)));
        buffer.set_wrap(Wrap::WordOrGlyph);
        buffer.set_size(style.width.map(px), None);
        let align = match style.align {
            Align::Left => cosmic_text::Align::Left,
            Align::Center => cosmic_text::Align::Center,
            Align::Right => cosmic_text::Align::Right,
            Align::Justified => cosmic_text::Align::Justified,
        };
        let spans = rich
            .spans
            .iter()
            .enumerate()
            .map(|(i, s)| (&rich.text[s.range.clone()], attrs_for(i, span_size(s.style.size))));
        buffer.set_rich_text(spans, &default_attrs, Shaping::Advanced, Some(align));
        buffer.shape_until_scroll(&mut self.fonts, false);

        // Collect positioned glyphs first; rasterizing needs `&mut self`.
        struct Placed {
            key: CacheKey,
            x: i32,
            y: i32,
            start: usize,
            span: usize,
            font_size: f32,
            line_top: f32,
            line_y: f32,
        }
        let mut placed = Vec::new();
        let mut size = Vec2::ZERO;
        for run in buffer.layout_runs() {
            size.x = size.x.max(run.line_w);
            size.y = size.y.max(run.line_top + run.line_height);
            for g in run.glyphs {
                let p = g.physical((0.0, run.line_y), 1.0);
                placed.push(Placed {
                    key: p.cache_key,
                    x: p.x,
                    y: p.y,
                    start: g.start,
                    span: g.metadata,
                    font_size: g.font_size,
                    line_top: run.line_top,
                    line_y: run.line_y,
                });
            }
        }

        let mut glyphs = Vec::with_capacity(placed.len());
        let mut push = |this: &mut Self,
                        key,
                        x: i32,
                        y: i32,
                        start: usize,
                        span: usize,
                        font_size: f32,
                        glyphs: &mut Vec<Glyph>| {
            let Some(c) = this.glyph(key, sink) else { return };
            let s = &rich.spans.get(span).map(|s| &s.style);
            let (units, pause) = lookup(&reveal, start);
            let rect = Rect::new(
                (x + c.left) as f32 / scale,
                (y - c.top) as f32 / scale,
                c.width as f32 / scale,
                c.height as f32 / scale,
            );
            glyphs.push(Glyph {
                texture: c.texture,
                rect,
                color: s.and_then(|s| s.color).unwrap_or(style.color),
                order: rich.text[..start.min(rich.text.len())].chars().count(),
                reveal_units: units,
                reveal_pause: pause,
                wave: s.and_then(|s| s.wave),
                shake: s.and_then(|s| s.shake),
                em: font_size / scale,
            });
        };
        for p in &placed {
            push(self, p.key, p.x, p.y, p.start, p.span, p.font_size, &mut glyphs);
        }

        // Ruby: shaped at half size and centered above the first line of its base text.
        for ruby in &rich.ruby {
            let base: Vec<&Placed> = placed.iter().filter(|p| ruby.base.contains(&p.start)).collect();
            let Some(first) = base.first() else { continue };
            let line = base.iter().filter(|p| p.line_top == first.line_top);
            let (mut x0, mut x1) = (f32::INFINITY, f32::NEG_INFINITY);
            for p in line {
                let w = self
                    .glyphs
                    .get(&p.key)
                    .and_then(|c| c.as_ref())
                    .map_or(0.0, |c| (c.left + c.width as i32) as f32);
                x0 = x0.min(p.x as f32);
                x1 = x1.max(p.x as f32 + w);
            }
            let ruby_size = first.font_size * 0.5;
            let attrs =
                attrs_for(first.span, ruby_size / scale).metrics(Metrics::new(ruby_size, ruby_size * 1.2));
            let mut rb = Buffer::new(&mut self.fonts, Metrics::new(ruby_size, ruby_size * 1.2));
            rb.set_rich_text([(ruby.text.as_str(), attrs.clone())], &attrs, Shaping::Advanced, None);
            rb.shape_until_scroll(&mut self.fonts, false);
            let mut ruby_glyphs = Vec::new();
            let mut width = 0.0f32;
            for run in rb.layout_runs() {
                width = width.max(run.line_w);
                for g in run.glyphs {
                    ruby_glyphs.push(g.clone());
                }
            }
            let left = (x0 + x1) * 0.5 - width * 0.5;
            // Baseline just above the base glyphs' ascent.
            let baseline = first.line_y - first.font_size * 0.92;
            for g in ruby_glyphs {
                let p = g.physical((left, baseline), 1.0);
                push(self, p.cache_key, p.x, p.y, ruby.base.start, first.span, ruby_size, &mut glyphs);
            }
        }

        let last = reveal.last().copied().unwrap_or_default();
        TextLayout { glyphs, size: size / scale, total_units: last.1, total_pause: last.2, scale }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::markup::parse;

    /// Records uploads instead of touching a GPU.
    #[derive(Default)]
    pub struct FakeSink {
        pub uploads: u32,
    }

    impl GlyphSink for FakeSink {
        fn upload(&mut self, _image: &Image) -> TextureId {
            self.uploads += 1;
            TextureId(self.uploads)
        }
    }

    pub fn system() -> TextSystem {
        let mut t = TextSystem::new();
        let names = t.load_font(include_bytes!("../../../demos/smoke/fonts/Inter-Regular.ttf").to_vec());
        assert_eq!(names, ["Inter"]);
        t.load_font(include_bytes!("../../../demos/smoke/fonts/Inter-Bold.ttf").to_vec());
        t
    }

    fn style() -> TextStyle {
        TextStyle { font: Some("Inter".into()), size: 20.0, ..Default::default() }
    }

    #[test]
    fn lays_out_and_caches_glyphs() {
        let (mut t, mut sink) = (system(), FakeSink::default());
        let rich = parse("aaa bbb").unwrap();
        let l = t.layout(&mut sink, &rich, &style(), 1.0);
        assert_eq!(l.glyphs.len(), 6, "spaces have no bitmap");
        // Glyphs are cached per subpixel position (4 bins), so each one uploads at most once
        // per bin, and re-laying out the same text uploads nothing.
        let first = sink.uploads;
        assert!(first <= 6);
        t.layout(&mut sink, &rich, &style(), 1.0);
        assert_eq!(sink.uploads, first);
        assert!(l.glyphs.windows(2).all(|w| w[0].rect.min.x <= w[1].rect.min.x));
        assert!(l.size.x > 50.0 && l.size.y >= 20.0);
    }

    #[test]
    fn wraps_at_width_and_scales_without_moving() {
        let (mut t, mut sink) = (system(), FakeSink::default());
        let rich = parse("one two three four five six").unwrap();
        let narrow = t.layout(&mut sink, &rich, &TextStyle { width: Some(80.0), ..style() }, 1.0);
        let wide = t.layout(&mut sink, &rich, &style(), 1.0);
        assert!(narrow.size.y > wide.size.y * 2.0, "wrapped onto several lines");
        // Rasterizing at 2x keeps design-space positions (sharper, not bigger).
        let hi = t.layout(&mut sink, &rich, &style(), 2.0);
        assert!((hi.size.x - wide.size.x).abs() < 2.0, "{} vs {}", hi.size.x, wide.size.x);
    }

    #[test]
    fn spans_set_color_effects_and_reveal() {
        let (mut t, mut sink) = (system(), FakeSink::default());
        let rich = parse("a{color=#ff0000}b{/color}{w=0.5}{wave}c{/wave}{speed=2}dd{/speed}").unwrap();
        let l = t.layout(&mut sink, &rich, &style(), 1.0);
        assert_eq!(l.glyphs[0].color, Color::WHITE);
        assert_eq!(l.glyphs[1].color, Color::hex("#ff0000").unwrap());
        assert!(l.glyphs[2].wave.is_some() && l.glyphs[1].wave.is_none());
        let units: Vec<f32> = l.glyphs.iter().map(|g| g.reveal_units).collect();
        assert_eq!(units, vec![0.0, 1.0, 2.0, 3.0, 3.5]);
        assert_eq!(l.glyphs[1].reveal_pause, 0.0);
        assert_eq!(l.glyphs[2].reveal_pause, 0.5);
        assert_eq!(l.total_units, 4.0);
    }

    #[test]
    fn ruby_sits_above_its_base() {
        let (mut t, mut sink) = (system(), FakeSink::default());
        let l = t.layout(&mut sink, &parse("x {ruby=small}BIG{/ruby}").unwrap(), &style(), 1.0);
        let base: Vec<&Glyph> = l.glyphs.iter().filter(|g| g.em > 15.0).collect();
        let ruby: Vec<&Glyph> = l.glyphs.iter().filter(|g| g.em < 15.0).collect();
        assert_eq!(base.len(), 4);
        assert_eq!(ruby.len(), 5);
        let base_top = base[1..].iter().map(|g| g.rect.min.y).fold(f32::INFINITY, f32::min);
        assert!(ruby.iter().all(|g| g.rect.max.y <= base_top + 1.0), "ruby is above the base text");
        let (bx0, bx1) = (base[1].rect.min.x, base[3].rect.max.x);
        let rx = (ruby[0].rect.min.x + ruby[4].rect.max.x) * 0.5;
        assert!((rx - (bx0 + bx1) * 0.5).abs() < 3.0, "ruby is centered over its base");
        assert!(ruby.iter().all(|g| g.reveal_units == base[1].reveal_units), "ruby appears with its base");
    }
}
