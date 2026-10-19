//! Easing curves and tweens of node and camera properties.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::{NodeId, Scene};

/// Easing curve: maps progress 0..1 to eased progress (may overshoot for `Back`/`Elastic`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ease {
    #[default]
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InSine,
    OutSine,
    InOutSine,
    InBack,
    OutBack,
    InOutBack,
    OutElastic,
    OutBounce,
}

impl Ease {
    pub const NAMES: &[(&str, Ease)] = &[
        ("linear", Ease::Linear),
        ("in_quad", Ease::InQuad),
        ("out_quad", Ease::OutQuad),
        ("in_out_quad", Ease::InOutQuad),
        ("in_cubic", Ease::InCubic),
        ("out_cubic", Ease::OutCubic),
        ("in_out_cubic", Ease::InOutCubic),
        ("in_sine", Ease::InSine),
        ("out_sine", Ease::OutSine),
        ("in_out_sine", Ease::InOutSine),
        ("in_back", Ease::InBack),
        ("out_back", Ease::OutBack),
        ("in_out_back", Ease::InOutBack),
        ("out_elastic", Ease::OutElastic),
        ("out_bounce", Ease::OutBounce),
    ];

    pub fn from_name(name: &str) -> Option<Ease> {
        Self::NAMES.iter().find(|(n, _)| *n == name).map(|(_, e)| *e)
    }

    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        const C1: f32 = 1.70158;
        const C2: f32 = C1 * 1.525;
        const C3: f32 = C1 + 1.0;
        let bounce = |t: f32| {
            let (n, d) = (7.5625, 2.75);
            if t < 1.0 / d {
                n * t * t
            } else if t < 2.0 / d {
                let t = t - 1.5 / d;
                n * t * t + 0.75
            } else if t < 2.5 / d {
                let t = t - 2.25 / d;
                n * t * t + 0.9375
            } else {
                let t = t - 2.625 / d;
                n * t * t + 0.984375
            }
        };
        match self {
            Ease::Linear => t,
            Ease::InQuad => t * t,
            Ease::OutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Ease::InOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Ease::InCubic => t * t * t,
            Ease::OutCubic => 1.0 - (1.0 - t).powi(3),
            Ease::InOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Ease::InSine => 1.0 - (t * FRAC_PI_2).cos(),
            Ease::OutSine => (t * FRAC_PI_2).sin(),
            Ease::InOutSine => -((PI * t).cos() - 1.0) / 2.0,
            Ease::InBack => C3 * t * t * t - C1 * t * t,
            Ease::OutBack => 1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2),
            Ease::InOutBack => {
                if t < 0.5 {
                    ((2.0 * t).powi(2) * ((C2 + 1.0) * 2.0 * t - C2)) / 2.0
                } else {
                    ((2.0 * t - 2.0).powi(2) * ((C2 + 1.0) * (t * 2.0 - 2.0) + C2) + 2.0) / 2.0
                }
            }
            Ease::OutElastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    2f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * (2.0 * PI / 3.0)).sin() + 1.0
                }
            }
            Ease::OutBounce => bounce(t),
        }
    }
}

/// What a tween animates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prop {
    X,
    Y,
    Rotation,
    ScaleX,
    ScaleY,
    Opacity,
    TintR,
    TintG,
    TintB,
    TintA,
    CameraX,
    CameraY,
    CameraZoom,
    CameraRotation,
}

impl Prop {
    pub fn is_camera(self) -> bool {
        matches!(self, Prop::CameraX | Prop::CameraY | Prop::CameraZoom | Prop::CameraRotation)
    }

    /// Reads the property (`node` is ignored for camera properties). `None` for a removed node.
    pub fn get(self, scene: &Scene, node: NodeId) -> Option<f32> {
        let c = &scene.camera;
        if self.is_camera() {
            return Some(match self {
                Prop::CameraX => c.position.x,
                Prop::CameraY => c.position.y,
                Prop::CameraZoom => c.zoom,
                _ => c.rotation,
            });
        }
        let n = scene.get(node)?;
        Some(match self {
            Prop::X => n.transform.position.x,
            Prop::Y => n.transform.position.y,
            Prop::Rotation => n.transform.rotation,
            Prop::ScaleX => n.transform.scale.x,
            Prop::ScaleY => n.transform.scale.y,
            Prop::Opacity => n.opacity,
            Prop::TintR => n.tint.r,
            Prop::TintG => n.tint.g,
            Prop::TintB => n.tint.b,
            Prop::TintA => n.tint.a,
            _ => unreachable!("camera handled above"),
        })
    }

    pub fn set(self, scene: &mut Scene, node: NodeId, v: f32) {
        if self.is_camera() {
            let c = &mut scene.camera;
            match self {
                Prop::CameraX => c.position.x = v,
                Prop::CameraY => c.position.y = v,
                Prop::CameraZoom => c.zoom = v,
                _ => c.rotation = v,
            }
            return;
        }
        let Some(n) = scene.get_mut(node) else { return };
        match self {
            Prop::X => n.transform.position.x = v,
            Prop::Y => n.transform.position.y = v,
            Prop::Rotation => n.transform.rotation = v,
            Prop::ScaleX => n.transform.scale.x = v,
            Prop::ScaleY => n.transform.scale.y = v,
            Prop::Opacity => n.opacity = v,
            Prop::TintR => n.tint.r = v,
            Prop::TintG => n.tint.g = v,
            Prop::TintB => n.tint.b = v,
            Prop::TintA => n.tint.a = v,
            _ => unreachable!("camera handled above"),
        }
    }
}

/// Animates properties of one node (or the camera) from their values at start to targets.
#[derive(Clone, Debug, PartialEq)]
pub struct Tween {
    pub node: NodeId,
    /// `(property, from, to)`; `from` is captured when the delay ends.
    pub props: Vec<(Prop, f32, f32)>,
    pub duration: f32,
    pub delay: f32,
    pub ease: Ease,
    /// Repeat count after the first run; `None` repeats forever.
    pub repeat: Option<u32>,
    /// Play forward then backward on repeats.
    pub yoyo: bool,
    elapsed: f32,
    started: bool,
    forward: bool,
}

impl Tween {
    pub fn new(node: NodeId, targets: Vec<(Prop, f32)>, duration: f32) -> Self {
        Self {
            node,
            props: targets.into_iter().map(|(p, to)| (p, 0.0, to)).collect(),
            duration: duration.max(0.0),
            delay: 0.0,
            ease: Ease::Linear,
            repeat: Some(0),
            yoyo: false,
            elapsed: 0.0,
            started: false,
            forward: true,
        }
    }

    /// Advances by `dt`, writing into `scene`. Returns true when finished (or its node is gone).
    pub fn step(&mut self, scene: &mut Scene, dt: f32) -> bool {
        let mut dt = dt.max(0.0);
        if self.delay > 0.0 {
            let used = self.delay.min(dt);
            self.delay -= used;
            dt -= used;
            if self.delay > 0.0 {
                return false;
            }
        }
        if !self.started {
            for (p, from, _) in &mut self.props {
                match p.get(scene, self.node) {
                    Some(v) => *from = v,
                    None => return true,
                }
            }
            self.started = true;
        }
        if !self.props.iter().all(|(p, ..)| p.is_camera()) && !scene.contains(self.node) {
            return true;
        }
        if self.duration <= 0.0 {
            // Instant: jump to the end. A forever-repeating instant tween never finishes.
            self.apply(scene, 1.0);
            return self.repeat.is_some();
        }
        self.elapsed += dt;
        // Finish whole cycles a large `dt` skipped over (bounded by the repeat count).
        while self.elapsed >= self.duration {
            match self.repeat {
                Some(0) => {
                    self.apply(scene, 1.0);
                    return true;
                }
                Some(n) => self.repeat = Some(n - 1),
                None => {}
            }
            self.elapsed -= self.duration;
            if self.yoyo {
                self.forward = !self.forward;
            }
        }
        self.apply(scene, self.elapsed / self.duration);
        false
    }

    /// Writes the properties at `progress` (0..1) of the current direction.
    fn apply(&self, scene: &mut Scene, progress: f32) {
        let k = self.ease.apply(if self.forward { progress } else { 1.0 - progress });
        for &(p, from, to) in &self.props {
            p.set(scene, self.node, from + (to - from) * k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Node, Vec2};

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn eases_start_and_end_in_place() {
        for (name, e) in Ease::NAMES {
            assert!(approx(e.apply(0.0), 0.0), "{name} at 0");
            assert!(approx(e.apply(1.0), 1.0), "{name} at 1");
            assert_eq!(Ease::from_name(name), Some(*e));
        }
        assert!(Ease::OutBack.apply(0.7) > 1.0, "out_back overshoots");
        assert!(approx(Ease::InOutQuad.apply(0.5), 0.5));
        assert_eq!(Ease::from_name("wobbly"), None);
    }

    #[test]
    fn tweens_properties_with_delay_and_ease() {
        let mut s = Scene::new(Vec2::new(100.0, 100.0));
        let n = s.add(s.root(), Node::group().at(10.0, 0.0));
        let mut t = Tween::new(n, vec![(Prop::X, 30.0), (Prop::Opacity, 0.0)], 2.0);
        t.delay = 1.0;
        assert!(!t.step(&mut s, 1.0), "still delayed");
        assert_eq!(s[n].transform.position.x, 10.0);
        assert!(!t.step(&mut s, 1.0));
        assert!(approx(s[n].transform.position.x, 20.0));
        assert!(approx(s[n].opacity, 0.5));
        assert!(t.step(&mut s, 5.0), "finished");
        assert_eq!(s[n].transform.position.x, 30.0);
    }

    #[test]
    fn repeats_yoyo_and_stops_for_removed_nodes() {
        let mut s = Scene::new(Vec2::new(100.0, 100.0));
        let n = s.add(s.root(), Node::group());
        let mut t = Tween::new(n, vec![(Prop::Y, 10.0)], 1.0);
        t.repeat = Some(1);
        t.yoyo = true;
        assert!(!t.step(&mut s, 1.0));
        assert!(approx(s[n].transform.position.y, 10.0));
        assert!(!t.step(&mut s, 0.5));
        assert!(approx(s[n].transform.position.y, 5.0), "{}", s[n].transform.position.y);
        assert!(t.step(&mut s, 0.5));
        assert!(approx(s[n].transform.position.y, 0.0));

        let mut forever = Tween::new(n, vec![(Prop::Rotation, 1.0)], 0.5);
        forever.repeat = None;
        for _ in 0..10 {
            assert!(!forever.step(&mut s, 0.3));
        }
        s.remove(n);
        assert!(forever.step(&mut s, 0.1), "node gone");
    }

    #[test]
    fn camera_tweens_need_no_node() {
        let mut s = Scene::new(Vec2::new(100.0, 100.0));
        let root = s.root();
        let mut t = Tween::new(root, vec![(Prop::CameraZoom, 2.0)], 1.0);
        t.step(&mut s, 0.5);
        assert!(approx(s.camera.zoom, 1.5));
    }
}
