//! Touch-style gestures from one pointer: tap, long press and swipe. They work for the
//! mouse too (click, hold, drag).

use kulisa_core::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    Tap(Vec2),
    LongPress(Vec2),
    /// Direction and where the swipe started.
    Swipe(Direction, Vec2),
}

/// Recognizes gestures from press, move and release, in design units and seconds.
pub struct Gestures {
    /// How far a tap or long press may move.
    pub slop: f32,
    /// How far a swipe must travel.
    pub swipe_distance: f32,
    /// How long a swipe may take.
    pub swipe_time: f64,
    pub long_press_time: f64,
    down: Option<Down>,
}

struct Down {
    start: Vec2,
    at: f64,
    last: Vec2,
    moved: bool,
    long_fired: bool,
}

impl Default for Gestures {
    fn default() -> Self {
        Self { slop: 24.0, swipe_distance: 120.0, swipe_time: 0.6, long_press_time: 0.5, down: None }
    }
}

impl Gestures {
    pub fn down(&mut self, p: Vec2, time: f64) {
        self.down = Some(Down { start: p, at: time, last: p, moved: false, long_fired: false });
    }

    pub fn moved(&mut self, p: Vec2) {
        if let Some(d) = &mut self.down {
            d.last = p;
            d.moved |= (p - d.start).length() > self.slop;
        }
    }

    /// Forgets the current press (the UI took it).
    pub fn cancel(&mut self) {
        self.down = None;
    }

    /// Call every frame: a long press fires while the pointer is still down.
    pub fn tick(&mut self, time: f64) -> Option<Gesture> {
        let d = self.down.as_mut()?;
        if !d.moved && !d.long_fired && time - d.at >= self.long_press_time {
            d.long_fired = true;
            return Some(Gesture::LongPress(d.start));
        }
        None
    }

    pub fn up(&mut self, p: Vec2, time: f64) -> Option<Gesture> {
        let d = self.down.take()?;
        if d.long_fired {
            return None;
        }
        let delta = p - d.start;
        if delta.length() <= self.slop && !d.moved {
            return Some(Gesture::Tap(p));
        }
        if time - d.at <= self.swipe_time && delta.length() >= self.swipe_distance {
            let dir = if delta.x.abs() > delta.y.abs() {
                if delta.x > 0.0 { Direction::Right } else { Direction::Left }
            } else if delta.y > 0.0 {
                Direction::Down
            } else {
                Direction::Up
            };
            return Some(Gesture::Swipe(dir, d.start));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_long_presses_and_swipes() {
        let mut g = Gestures::default();
        g.down(Vec2::new(100.0, 100.0), 0.0);
        g.moved(Vec2::new(105.0, 100.0));
        assert_eq!(g.tick(0.2), None);
        assert_eq!(g.up(Vec2::new(105.0, 100.0), 0.2), Some(Gesture::Tap(Vec2::new(105.0, 100.0))));

        g.down(Vec2::new(100.0, 100.0), 1.0);
        assert_eq!(g.tick(1.6), Some(Gesture::LongPress(Vec2::new(100.0, 100.0))));
        assert_eq!(g.tick(1.7), None, "fires once");
        assert_eq!(g.up(Vec2::new(100.0, 100.0), 2.0), None, "no tap after a long press");

        g.down(Vec2::new(100.0, 100.0), 3.0);
        g.moved(Vec2::new(100.0, 200.0));
        assert_eq!(g.tick(3.6), None, "moving cancels the long press");
        assert_eq!(
            g.up(Vec2::new(110.0, 300.0), 3.3),
            Some(Gesture::Swipe(Direction::Down, Vec2::new(100.0, 100.0)))
        );

        // Too slow for a swipe, too far for a tap.
        g.down(Vec2::new(0.0, 0.0), 5.0);
        g.moved(Vec2::new(-300.0, 0.0));
        assert_eq!(g.up(Vec2::new(-300.0, 0.0), 7.0), None);
        g.down(Vec2::ZERO, 8.0);
        g.cancel();
        assert_eq!(g.up(Vec2::ZERO, 8.1), None);
    }
}
