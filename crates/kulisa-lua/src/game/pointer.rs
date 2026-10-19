//! Pointers: the mouse (id 0) and each touch (ids from 1), and two-finger pinch and rotate.

use std::f32::consts::PI;

use kulisa_core::Vec2;

/// One pointer's state. Positions are design units.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pointer {
    pub pos: Option<Vec2>,
    pub touch: bool,
    pub down: bool,
    /// The press went to the UI, a drag or a hotspot, so it takes no part in pinch gestures.
    pub captured: bool,
}

impl Pointer {
    pub fn new(touch: bool) -> Self {
        Self { pos: None, touch, down: false, captured: false }
    }
}

/// A two-touch gesture in progress.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pinch {
    ids: (u32, u32),
    dist: f32,
    angle: f32,
    start_dist: f32,
    total_angle: f32,
}

/// What a pinch changed since the last step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PinchStep {
    /// Distance now over distance at the last step.
    pub factor: f32,
    /// Distance now over distance when the pinch started.
    pub total: f32,
    /// Radians turned since the last step, counter-clockwise on screen being negative (y down).
    pub delta: f32,
    pub total_angle: f32,
    pub center: Vec2,
}

fn wrap(a: f32) -> f32 {
    let a = (a + PI).rem_euclid(2.0 * PI) - PI;
    if a <= -PI { a + 2.0 * PI } else { a }
}

impl Pinch {
    /// Follows the free touches `(id, position)`: starts a pinch when exactly two are down and
    /// reports the change on later calls. Any other number of touches ends it.
    pub fn track(touches: &[(u32, Vec2)], current: &mut Option<Pinch>) -> Option<PinchStep> {
        let [a, b] = touches else {
            *current = None;
            return None;
        };
        let (a, b) = if a.0 <= b.0 { (*a, *b) } else { (*b, *a) };
        let d = b.1 - a.1;
        let (dist, angle) = (d.length(), d.y.atan2(d.x));
        let center = (a.1 + b.1) * 0.5;
        match current {
            Some(p) if p.ids == (a.0, b.0) => {
                let delta = wrap(angle - p.angle);
                p.total_angle += delta;
                let factor = if p.dist >= 1.0 { dist / p.dist } else { 1.0 };
                let total = if p.start_dist >= 1.0 { dist / p.start_dist } else { 1.0 };
                let step = PinchStep { factor, total, delta, total_angle: p.total_angle, center };
                p.dist = dist;
                p.angle = angle;
                Some(step)
            }
            _ => {
                *current = Some(Pinch { ids: (a.0, b.0), dist, angle, start_dist: dist, total_angle: 0.0 });
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(a: (f32, f32), b: (f32, f32)) -> Vec<(u32, Vec2)> {
        vec![(1, Vec2::new(a.0, a.1)), (2, Vec2::new(b.0, b.1))]
    }

    #[test]
    fn two_touches_start_a_pinch_and_report_scale_and_turn() {
        let mut p = None;
        assert_eq!(Pinch::track(&at((0.0, 0.0), (100.0, 0.0)), &mut p), None, "starting reports nothing");
        let s = Pinch::track(&at((0.0, 0.0), (200.0, 0.0)), &mut p).unwrap();
        assert_eq!((s.factor, s.total, s.delta), (2.0, 2.0, 0.0));
        assert_eq!(s.center, Vec2::new(100.0, 0.0));
        let s = Pinch::track(&at((0.0, 0.0), (100.0, 0.0)), &mut p).unwrap();
        assert_eq!((s.factor, s.total), (0.5, 1.0), "steps are relative, totals are from the start");
        // Turn a quarter circle clockwise on screen (y down): the angle grows.
        let s = Pinch::track(&at((0.0, 0.0), (0.0, 100.0)), &mut p).unwrap();
        assert!((s.delta - PI / 2.0).abs() < 1e-5 && (s.total_angle - PI / 2.0).abs() < 1e-5, "{s:?}");
    }

    #[test]
    fn angles_wrap_instead_of_jumping() {
        let mut p = None;
        Pinch::track(&at((0.0, 0.0), (-100.0, -1.0)), &mut p);
        // Crossing the -x axis flips atan2 from about -pi to +pi; the turn is tiny.
        let s = Pinch::track(&at((0.0, 0.0), (-100.0, 1.0)), &mut p).unwrap();
        assert!(s.delta.abs() < 0.05, "{}", s.delta);
    }

    #[test]
    fn other_touch_counts_end_it() {
        let mut p = None;
        Pinch::track(&at((0.0, 0.0), (10.0, 0.0)), &mut p);
        assert!(p.is_some());
        assert_eq!(Pinch::track(&[(1, Vec2::ZERO)], &mut p), None);
        assert!(p.is_none());
        // A different pair starts fresh.
        Pinch::track(&at((0.0, 0.0), (10.0, 0.0)), &mut p);
        let other = vec![(1, Vec2::ZERO), (3, Vec2::new(20.0, 0.0))];
        assert_eq!(Pinch::track(&other, &mut p), None);
        assert_eq!(Pinch::track(&other, &mut p).unwrap().factor, 1.0);
    }
}
