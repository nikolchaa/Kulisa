use glam::{Affine2, UVec2, Vec2};

use crate::Rect;

/// How the design resolution maps onto the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScaleMode {
    /// Whole design area visible; letterboxed (nothing drawn outside it).
    Fit,
    /// Design area covers the window; edges are cropped.
    Fill,
    /// Like `Fit`, but content outside the design area stays visible (wider/taller
    /// screens see more). The usual choice for 16:9 to 20:9 phones and tablets.
    #[default]
    Expand,
}

/// The game's design resolution and scale mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub design: Vec2,
    pub mode: ScaleMode,
    /// Allowed window aspect ratios (width / height), `[min, max]`. A window outside the
    /// range is clamped to it first (bars beyond it); the scale mode applies inside.
    pub aspect: Option<[f32; 2]>,
    /// Shifts everything by this many window pixels (a negative y lifts the view above an
    /// on-screen keyboard). Input and drawing agree, so pointers keep hitting what they see.
    pub pan: Vec2,
}

/// A viewport resolved for a concrete window size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewLayout {
    /// Window pixels per design unit.
    pub scale: f32,
    /// The design-space rectangle visible in the window.
    pub visible: Rect,
    /// Design-to-window-pixel transform.
    pub to_pixels: Affine2,
    /// The window-pixel rectangle outside which nothing is drawn (`Fit` mode or an aspect
    /// clamp), or `None` when the whole window is drawn.
    pub letterbox: Option<Rect>,
}

impl Viewport {
    pub fn new(width: f32, height: f32, mode: ScaleMode) -> Self {
        Self { design: Vec2::new(width, height), mode, aspect: None, pan: Vec2::ZERO }
    }

    /// The part of the window the game uses after the aspect clamp, in window pixels.
    pub fn area(&self, window: UVec2) -> Rect {
        let win = window.max(UVec2::ONE).as_vec2();
        let mut size = win;
        if let Some([min, max]) = self.aspect {
            let (min, max) = (min.min(max).max(1e-3), max.max(min).max(1e-3));
            let a = win.x / win.y;
            if a > max {
                size.x = win.y * max;
            } else if a < min {
                size.y = win.x / min;
            }
        }
        let min = ((win - size) * 0.5).round();
        Rect { min, max: min + size }
    }

    pub fn layout(&self, window: UVec2) -> ViewLayout {
        let area = self.area(window);
        let size = area.size();
        let ratio = size / self.design;
        let scale = match self.mode {
            ScaleMode::Fill => ratio.x.max(ratio.y),
            ScaleMode::Fit | ScaleMode::Expand => ratio.x.min(ratio.y),
        };
        let center = (area.min + area.max) * 0.5 + self.pan;
        let to_pixels = Affine2::from_translation(center)
            * Affine2::from_scale(Vec2::splat(scale))
            * Affine2::from_translation(-self.design * 0.5);
        let half = size * 0.5 / scale;
        let design_center = self.design * 0.5;
        let visible = Rect { min: design_center - half, max: design_center + half };
        let clamped = size != window.max(UVec2::ONE).as_vec2();
        let letterbox = if self.mode == ScaleMode::Fit {
            let s = self.design * scale;
            Some(Rect { min: center - s * 0.5, max: center + s * 0.5 })
        } else {
            clamped.then_some(area)
        };
        let visible = match (self.mode, letterbox) {
            (ScaleMode::Fit, _) => Rect { min: Vec2::ZERO, max: self.design },
            _ => visible,
        };
        ViewLayout { scale, visible, to_pixels, letterbox }
    }
}

/// Parses an aspect ratio: `"21:9"`, `"4/3"` or a number.
pub fn parse_aspect(s: &str) -> Option<f32> {
    let s = s.trim();
    let split = s.find([':', '/']);
    let v = match split {
        Some(i) => s[..i].trim().parse::<f32>().ok()? / s[i + 1..].trim().parse::<f32>().ok()?,
        None => s.parse().ok()?,
    };
    (v.is_finite() && v > 0.0).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_shows_more_on_wide_screens() {
        let v = Viewport::new(1920.0, 1080.0, ScaleMode::Expand);
        // 20:9 phone in landscape, 2400x1080.
        let l = v.layout(UVec2::new(2400, 1080));
        assert!((l.scale - 1.0).abs() < 1e-6);
        assert!((l.visible.min.x - -240.0).abs() < 1e-3 && (l.visible.max.x - 2160.0).abs() < 1e-3);
        assert_eq!(l.letterbox, None);
        assert!((l.to_pixels.transform_point2(Vec2::ZERO) - Vec2::new(240.0, 0.0)).length() < 1e-3);
    }

    #[test]
    fn fit_letterboxes_and_fill_crops() {
        let v = Viewport::new(1920.0, 1080.0, ScaleMode::Fit);
        let l = v.layout(UVec2::new(1920, 1440)); // 4:3
        let lb = l.letterbox.unwrap();
        assert!((lb.min.y - 180.0).abs() < 1e-3 && (lb.max.y - 1260.0).abs() < 1e-3);
        assert_eq!(l.visible, Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let v = Viewport::new(1920.0, 1080.0, ScaleMode::Fill);
        let l = v.layout(UVec2::new(1920, 1440));
        assert!((l.scale - 1440.0 / 1080.0).abs() < 1e-5);
        assert!(l.visible.min.x > 0.0, "sides are cropped");
    }

    #[test]
    fn aspect_clamp_bars_ultrawide_and_expands_inside() {
        let mut v = Viewport::new(1920.0, 1080.0, ScaleMode::Expand);
        v.aspect = Some([4.0 / 3.0, 21.0 / 9.0]);
        // 32:9 (5120x1440) is clamped to 21:9 (3360x1440), centered.
        let l = v.layout(UVec2::new(5120, 1440));
        let lb = l.letterbox.unwrap();
        assert!((lb.size().x - 3360.0).abs() < 1.0 && (lb.min.x - 880.0).abs() < 1.0, "{lb:?}");
        assert!((l.scale - 1440.0 / 1080.0).abs() < 1e-5);
        // Inside the clamp, expand still shows more than 16:9.
        assert!((l.visible.size().x - 2520.0).abs() < 1.0, "{:?}", l.visible);
        // The design center stays at the window center.
        assert!(
            (l.to_pixels.transform_point2(Vec2::new(960.0, 540.0)) - Vec2::new(2560.0, 720.0)).length() < 1.0
        );

        // Inside the range nothing changes.
        let l = v.layout(UVec2::new(2400, 1080));
        assert_eq!(l.letterbox, None);
        // Too tall (portrait 9:16 under a 4:3 minimum) bars top and bottom.
        let l = v.layout(UVec2::new(1080, 1920));
        let lb = l.letterbox.unwrap();
        assert!((lb.size().y - 810.0).abs() < 1.0, "{lb:?}");
    }

    #[test]
    fn parses_aspects() {
        assert!((parse_aspect("21:9").unwrap() - 21.0 / 9.0).abs() < 1e-6);
        assert!((parse_aspect("4/3").unwrap() - 4.0 / 3.0).abs() < 1e-6);
        assert_eq!(parse_aspect("1.5"), Some(1.5));
        assert_eq!(parse_aspect("0:9"), None);
        assert_eq!(parse_aspect("wide"), None);
    }
}
