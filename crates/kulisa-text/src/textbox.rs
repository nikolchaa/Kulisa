//! Typewriter reveal and per-glyph effects over a [`TextLayout`].

use kulisa_core::{Color, Quad, Rect, Vec2};

use crate::TextLayout;

/// Reveal settings. Story and UI code tune these per text box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typewriter {
    /// Characters per second; 0 shows everything at once.
    pub cps: f32,
    /// Seconds each glyph takes to fade in.
    pub fade: f32,
}

impl Default for Typewriter {
    fn default() -> Self {
        Self { cps: 40.0, fade: 0.08 }
    }
}

/// A laid-out text with reveal progress.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextBox {
    pub layout: TextLayout,
    pub typewriter: Typewriter,
    /// Seconds since the reveal started.
    pub elapsed: f32,
}

/// Small deterministic hash in -1..=1 for shake offsets.
fn jitter(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 32767.5 - 1.0
}

impl TextBox {
    pub fn new(layout: TextLayout, typewriter: Typewriter) -> Self {
        Self { layout, typewriter, elapsed: 0.0 }
    }

    /// When a glyph starts to appear, in seconds.
    fn reveal_at(&self, units: f32, pause: f32) -> f32 {
        if self.typewriter.cps <= 0.0 { 0.0 } else { units / self.typewriter.cps + pause }
    }

    /// Seconds until the whole text is shown.
    pub fn duration(&self) -> f32 {
        if self.typewriter.cps <= 0.0 {
            return 0.0;
        }
        self.reveal_at(self.layout.total_units, self.layout.total_pause) + self.typewriter.fade
    }

    pub fn advance(&mut self, dt: f32) {
        self.elapsed += dt.max(0.0);
    }

    pub fn is_complete(&self) -> bool {
        self.elapsed >= self.duration()
    }

    /// Reveals everything now (the player clicked during the typewriter).
    pub fn skip(&mut self) {
        self.elapsed = self.elapsed.max(self.duration());
    }

    /// Writes this frame's glyph quads. `time` drives the wave and shake effects.
    pub fn quads(&self, time: f32, out: &mut Vec<Quad>) {
        out.clear();
        let frame = (time * 24.0) as u32; // shake updates at 24 Hz, like hand-drawn jitter
        for g in &self.layout.glyphs {
            let start = self.reveal_at(g.reveal_units, g.reveal_pause);
            let alpha = if self.typewriter.fade <= 0.0 {
                if self.elapsed >= start { 1.0 } else { 0.0 }
            } else {
                ((self.elapsed - start) / self.typewriter.fade).clamp(0.0, 1.0)
            };
            if alpha <= 0.0 {
                continue;
            }
            let mut offset = Vec2::ZERO;
            if let Some(amp) = g.wave {
                offset.y += (time * 5.0 - g.order as f32 * 0.45).sin() * amp * g.em;
            }
            if let Some(amp) = g.shake {
                let k = g.order as u32;
                offset += Vec2::new(jitter(k, frame), jitter(k ^ 0xABCD, frame)) * amp * g.em;
            }
            let rect = Rect { min: g.rect.min + offset, max: g.rect.max + offset };
            out.push(Quad {
                texture: g.texture,
                rect,
                uv: Rect::UNIT,
                color: Color { a: g.color.a * alpha, ..g.color },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::tests::{FakeSink, system};
    use crate::{TextStyle, parse};

    fn textbox(markup: &str, typewriter: Typewriter) -> TextBox {
        let (mut t, mut sink) = (system(), FakeSink::default());
        let style = TextStyle { font: Some("Inter".into()), ..Default::default() };
        TextBox::new(t.layout(&mut sink, &parse(markup).unwrap(), &style, 1.0), typewriter)
    }

    fn visible(b: &TextBox) -> usize {
        let mut q = Vec::new();
        b.quads(0.0, &mut q);
        q.len()
    }

    #[test]
    fn reveals_over_time_with_pauses() {
        let mut b = textbox("ab{w=1}cd", Typewriter { cps: 10.0, fade: 0.0 });
        assert_eq!(visible(&b), 1, "first glyph at t=0");
        b.advance(0.15);
        assert_eq!(visible(&b), 2);
        b.advance(0.5);
        assert_eq!(visible(&b), 2, "waiting on the pause");
        b.advance(0.6); // t = 1.25: `c` is due at 2/10 s + 1 s pause
        assert_eq!(visible(&b), 3);
        assert!(!b.is_complete());
        b.skip();
        assert!(b.is_complete());
        assert_eq!(visible(&b), 4);
        assert!((b.duration() - 1.4).abs() < 1e-4, "4 chars at 10 cps + 1s pause");
    }

    #[test]
    fn instant_text_and_fade() {
        let b = textbox("abc", Typewriter { cps: 0.0, fade: 0.0 });
        assert_eq!(visible(&b), 3);
        assert!(b.is_complete());
        let mut b = textbox("a", Typewriter { cps: 10.0, fade: 0.2 });
        b.advance(0.1);
        let mut q = Vec::new();
        b.quads(0.0, &mut q);
        assert!((q[0].color.a - 0.5).abs() < 1e-4, "half faded in");
    }

    #[test]
    fn wave_and_shake_move_glyphs() {
        let still = textbox("ab", Typewriter { cps: 0.0, fade: 0.0 });
        let wave = textbox("{wave}ab{/wave}", Typewriter { cps: 0.0, fade: 0.0 });
        let shake = textbox("{shake}ab{/shake}", Typewriter { cps: 0.0, fade: 0.0 });
        let (mut a, mut w, mut s1, mut s2) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        still.quads(0.3, &mut a);
        wave.quads(0.3, &mut w);
        shake.quads(0.30, &mut s1);
        shake.quads(0.50, &mut s2);
        assert_ne!(a[0].rect.min.y, w[0].rect.min.y);
        assert_eq!(a[0].rect.min.x, w[0].rect.min.x, "wave is vertical");
        assert_ne!(s1[0].rect.min, s2[0].rect.min, "shake changes between frames");
        let d = (s1[0].rect.min - a[0].rect.min).length();
        assert!(d <= 0.04 * 32.0 * 1.5, "shake stays within its amplitude ({d})");
    }
}
