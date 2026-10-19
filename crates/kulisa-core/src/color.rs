/// Linear RGBA color with straight (non-premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

impl Color {
    pub const WHITE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::rgba(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);

    /// A color from linear components.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// A color from sRGB components in 0..=1, as written in design tools.
    pub fn srgb(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::rgba(srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), a)
    }

    /// Parses `#rgb`, `#rrggbb` or `#rrggbbaa` (sRGB).
    pub fn hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#').unwrap_or(s);
        let digit = |i: usize, len: usize| u8::from_str_radix(s.get(i..i + len)?, 16).ok();
        let (r, g, b, a) = match s.len() {
            3 => (digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17, 255),
            6 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, 255),
            8 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, digit(6, 2)?),
            _ => return None,
        };
        let f = |v: u8| v as f32 / 255.0;
        Some(Self::srgb(f(r), f(g), f(b), f(a)))
    }

    pub fn premultiplied(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }
}

/// Component-wise product (tint composition).
impl std::ops::Mul for Color {
    type Output = Color;
    fn mul(self, o: Color) -> Color {
        Color::rgba(self.r * o.r, self.g * o.g, self.b * o.b, self.a * o.a)
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::WHITE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex() {
        let c = Color::hex("#ffffff").unwrap();
        assert!((c.r - 1.0).abs() < 1e-6 && c.a == 1.0);
        let c = Color::hex("#80808080").unwrap();
        assert!((c.r - 0.2158).abs() < 1e-3, "sRGB 0x80 is ~0.216 linear, got {}", c.r);
        assert!((c.a - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(Color::hex("#f00").unwrap(), Color::hex("ff0000").unwrap());
        assert!(Color::hex("#12345").is_none());
        assert!(Color::hex("#gg0000").is_none());
    }
}
