use glam::{Affine2, Vec2};

/// 2D camera. The default position is the design center, so an untouched camera shows the
/// design area 1:1 and layers with parallax 0 behave as plain screen space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// World point shown at the center of the screen.
    pub position: Vec2,
    pub zoom: f32,
    pub rotation: f32,
    /// Extra offset added to `position` (screen shake, dolly jitter).
    pub shake: Vec2,
    center: Vec2,
}

impl Camera {
    pub fn new(design_center: Vec2) -> Self {
        Self { position: design_center, zoom: 1.0, rotation: 0.0, shake: Vec2::ZERO, center: design_center }
    }

    /// The design center this camera rests on.
    pub fn center(&self) -> Vec2 {
        self.center
    }

    /// World-to-design transform for content with the given parallax factor. Position,
    /// zoom and rotation all scale with `parallax`: 0 is fixed, 1 moves with the camera.
    pub fn view(&self, parallax: f32) -> Affine2 {
        let pos = self.center + (self.position + self.shake - self.center) * parallax;
        let zoom = 1.0 + (self.zoom - 1.0) * parallax;
        Affine2::from_translation(self.center)
            * Affine2::from_angle(-self.rotation * parallax)
            * Affine2::from_scale(Vec2::splat(zoom))
            * Affine2::from_translation(-pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn default_camera_is_identity() {
        let c = Camera::new(Vec2::new(960.0, 540.0));
        let p = Vec2::new(123.0, 456.0);
        assert!(approx(c.view(1.0).transform_point2(p), p));
        assert!(approx(c.view(0.3).transform_point2(p), p));
    }

    #[test]
    fn parallax_scales_movement() {
        let mut c = Camera::new(Vec2::new(960.0, 540.0));
        c.position.x += 100.0;
        let p = Vec2::new(960.0, 540.0);
        assert!(approx(c.view(1.0).transform_point2(p), Vec2::new(860.0, 540.0)));
        assert!(approx(c.view(0.5).transform_point2(p), Vec2::new(910.0, 540.0)));
        assert!(approx(c.view(0.0).transform_point2(p), p), "parallax 0 is fixed");
    }

    #[test]
    fn zoom_is_around_screen_center() {
        let mut c = Camera::new(Vec2::new(960.0, 540.0));
        c.zoom = 2.0;
        assert!(approx(c.view(1.0).transform_point2(Vec2::new(960.0, 540.0)), Vec2::new(960.0, 540.0)));
        assert!(approx(c.view(1.0).transform_point2(Vec2::new(1060.0, 540.0)), Vec2::new(1160.0, 540.0)));
    }
}
