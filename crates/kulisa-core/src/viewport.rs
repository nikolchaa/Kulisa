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
    /// In `Fit` mode, the window-pixel rectangle outside which nothing is drawn.
    pub letterbox: Option<Rect>,
}

impl Viewport {
    pub fn new(width: f32, height: f32, mode: ScaleMode) -> Self {
        Self { design: Vec2::new(width, height), mode }
    }

    pub fn layout(&self, window: UVec2) -> ViewLayout {
        let win = window.max(UVec2::ONE).as_vec2();
        let ratio = win / self.design;
        let scale = match self.mode {
            ScaleMode::Fill => ratio.x.max(ratio.y),
            ScaleMode::Fit | ScaleMode::Expand => ratio.x.min(ratio.y),
        };
        let to_pixels = Affine2::from_translation(win * 0.5)
            * Affine2::from_scale(Vec2::splat(scale))
            * Affine2::from_translation(-self.design * 0.5);
        let half = win * 0.5 / scale;
        let center = self.design * 0.5;
        let visible = Rect { min: center - half, max: center + half };
        let letterbox = (self.mode == ScaleMode::Fit).then(|| {
            let size = self.design * scale;
            Rect { min: (win - size) * 0.5, max: (win + size) * 0.5 }
        });
        ViewLayout { scale, visible, to_pixels, letterbox }
    }
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

        let v = Viewport::new(1920.0, 1080.0, ScaleMode::Fill);
        let l = v.layout(UVec2::new(1920, 1440));
        assert!((l.scale - 1440.0 / 1080.0).abs() < 1e-5);
        assert!(l.visible.min.x > 0.0, "sides are cropped");
    }
}
