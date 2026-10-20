//! Particle systems simulated on the GPU without compute shaders.
//!
//! The CPU only writes a record when a particle is born (where, how fast, when, for how long,
//! a random seed) into a ring buffer; the renderer keeps the buffer on the GPU and uploads just
//! the records written since the last frame. The vertex shader works out every particle's
//! position, size, rotation, color and sprite frame from its record and the system's clock in
//! closed form (velocity, gravity, linear drag, wander), so the per-frame cost on the CPU does not
//! grow with the number of particles, and it runs anywhere instanced drawing does (WebGL2).

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use glam::{Affine2, Vec2};

use crate::{Color, TextureId};

/// One particle's birth record, as the GPU reads it (an instance attribute block, 48 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Particle {
    /// Where it was born, in the system node's local units.
    pub pos: [f32; 2],
    /// Its starting velocity, units per second.
    pub vel: [f32; 2],
    /// The system clock when it was born, and how long it lives (seconds).
    pub birth: f32,
    pub life: f32,
    /// A multiplier on the system's size curve.
    pub size: f32,
    /// Rotation speed (radians per second) and starting rotation.
    pub spin: f32,
    pub angle: f32,
    /// A random number in 0..1 (wander phase, random sprite frame, shader effects).
    pub seed: f32,
    /// A per-particle tint, sRGB bytes with straight alpha.
    pub tint: [u8; 4],
    pub _pad: f32,
}

/// A particle buffer the renderer owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParticleBufferId(pub u32);

/// Where new particles appear, around the emitter position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Point,
    /// A filled rectangle centered on the emitter.
    Rect(Vec2),
    /// A filled circle, or its outline.
    Circle(f32),
    Ring(f32),
    /// A line of this length through the emitter, along `angle` (radians).
    Line(f32, f32),
}

/// A random value between two bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span(pub f32, pub f32);

impl Span {
    pub fn fixed(v: f32) -> Span {
        Span(v, v)
    }
}

/// How particles are born.
#[derive(Clone, Debug, PartialEq)]
pub struct Emitter {
    /// Emitter position in the node's local units.
    pub pos: Vec2,
    pub shape: Shape,
    /// Particles per second while `emitting`.
    pub rate: f32,
    pub emitting: bool,
    /// Direction (radians, 0 = right, positive is clockwise on screen since y points down)
    /// and the full angle around it new particles spread over.
    pub direction: f32,
    pub spread: f32,
    pub speed: Span,
    pub life: Span,
    /// Multiplier on the size curve.
    pub scale: Span,
    pub spin: Span,
    /// Starting rotation; `None` is random.
    pub angle: Option<f32>,
    /// Per-particle tints picked at random (white when empty).
    pub tints: Vec<Color>,
}

impl Default for Emitter {
    fn default() -> Self {
        Self {
            pos: Vec2::ZERO,
            shape: Shape::Point,
            rate: 30.0,
            emitting: true,
            direction: -std::f32::consts::FRAC_PI_2,
            spread: std::f32::consts::FRAC_PI_4,
            speed: Span(60.0, 120.0),
            life: Span(1.0, 2.0),
            scale: Span(1.0, 1.0),
            spin: Span(0.0, 0.0),
            angle: Some(0.0),
            tints: Vec::new(),
        }
    }
}

/// How particles move and look over their life (the same for every particle of a system).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub gravity: Vec2,
    /// Linear drag (per second): velocity decays as `exp(-drag * age)`.
    pub drag: f32,
    /// A sideways drift of `wander.0` units at `wander.1` cycles per second.
    pub wander: (f32, f32),
    /// Size in local units at birth and at death.
    pub size: (f32, f32),
    /// Color over life: start, middle, end (linear, straight alpha).
    pub colors: [Color; 3],
    /// Fade in and out, as fractions of the life.
    pub fade: (f32, f32),
    /// Sprite sheet columns and rows, and whether frames play over the life (else one is
    /// picked at random).
    pub frames: (u32, u32),
    pub animate: bool,
    /// Stretch along the velocity (sparks, rain): length grows by `stretch * speed` units.
    pub stretch: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            gravity: Vec2::ZERO,
            drag: 0.0,
            wander: (0.0, 0.0),
            size: (16.0, 16.0),
            colors: [Color::WHITE; 3],
            fade: (0.1, 0.3),
            frames: (1, 1),
            animate: false,
            stretch: 0.0,
        }
    }
}

/// What the particle vertex shader needs besides the records: eleven `vec4`s that follow the
/// material's four params in the uniform block (see the renderer's particle prelude).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ParticleUniform {
    /// The node-to-pixels transform: x axis, y axis, translation; then the clock and opacity.
    pub axes: [f32; 4],
    pub origin_clock: [f32; 4],
    pub motion: [f32; 4],
    pub wander_size: [f32; 4],
    pub colors: [[f32; 4]; 3],
    pub fade_frames: [f32; 4],
    pub animate: [f32; 4],
    pub tint: [f32; 4],
    pub _pad: [f32; 4],
}

/// A particle system: a ring buffer of records, its emitter, its look and its own clock.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleSystem {
    pub texture: TextureId,
    pub emitter: Emitter,
    pub look: Look,
    /// Time scale of the clock (0 pauses everything).
    pub speed: f32,
    /// Seconds since the system started (its own clock: precision stays good in long sessions).
    pub clock: f32,
    /// The renderer's buffer for the records (set by the host).
    pub buffer: Option<ParticleBufferId>,
    records: Vec<Particle>,
    cursor: usize,
    /// Records written since the last upload.
    dirty: Option<Range<usize>>,
    /// Whole records written past the end of `dirty` wrapped around: upload everything.
    wrapped: bool,
    carry: f32,
    rng: u64,
}

impl ParticleSystem {
    pub fn new(texture: TextureId, capacity: usize) -> Self {
        Self {
            texture,
            emitter: Emitter::default(),
            look: Look::default(),
            speed: 1.0,
            clock: 0.0,
            buffer: None,
            records: vec![Particle::default(); capacity.max(1)],
            cursor: 0,
            dirty: None,
            wrapped: false,
            carry: 0.0,
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn capacity(&self) -> usize {
        self.records.len()
    }

    pub fn records(&self) -> &[Particle] {
        &self.records
    }

    /// Particles alive at the current clock.
    pub fn alive(&self) -> usize {
        self.records
            .iter()
            .filter(|p| p.life > 0.0 && self.clock - p.birth < p.life && self.clock >= p.birth)
            .count()
    }

    /// Reseeds the random generator (tests, replays).
    pub fn seed(&mut self, seed: u64) {
        self.rng = seed.max(1);
    }

    fn random(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        ((self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    fn between(&mut self, s: Span) -> f32 {
        s.0 + (s.1 - s.0) * self.random()
    }

    /// Advances the clock by `dt` (times `speed`) and emits at the emitter's rate.
    pub fn step(&mut self, dt: f32) {
        let dt = dt * self.speed.max(0.0);
        if dt <= 0.0 {
            return;
        }
        let start = self.clock;
        self.clock += dt;
        if self.emitter.emitting && self.emitter.rate > 0.0 {
            self.carry += dt * self.emitter.rate;
            let n = self.carry.floor();
            self.carry -= n;
            let n = n as usize;
            // Spread births over the step so a slow frame does not emit in clumps.
            for i in 0..n {
                let at = start + dt * (i as f32 + 1.0) / (n as f32);
                let pos = self.emitter.pos;
                self.spawn(pos, at);
            }
        }
    }

    /// Emits `n` particles now, at `at` (local units) or the emitter position.
    pub fn burst(&mut self, n: usize, at: Option<Vec2>) {
        let pos = at.unwrap_or(self.emitter.pos);
        for _ in 0..n {
            self.spawn(pos, self.clock);
        }
    }

    /// Emits as if the system had been running for `secs` (backdated births).
    pub fn warm_up(&mut self, secs: f32) {
        let n = (secs * self.emitter.rate).floor() as usize;
        for i in 0..n {
            let at = self.clock - secs + secs * (i as f32 + 0.5) / n as f32;
            let pos = self.emitter.pos;
            self.spawn(pos, at);
        }
    }

    /// Removes every particle.
    pub fn clear(&mut self) {
        self.records.fill(Particle::default());
        self.wrapped = true;
    }

    fn spawn(&mut self, center: Vec2, birth: f32) {
        let e = &self.emitter;
        let (shape, direction, spread) = (e.shape, e.direction, e.spread);
        let (speed, life, scale, spin, angle) = (e.speed, e.life, e.scale, e.spin, e.angle);
        let tints = e.tints.len();
        let tint = match tints {
            0 => [255; 4],
            n => {
                let k = ((self.random() * n as f32) as usize).min(n - 1);
                srgb_bytes(self.emitter.tints[k])
            }
        };
        let offset = match shape {
            Shape::Point => Vec2::ZERO,
            Shape::Rect(size) => Vec2::new(self.random() - 0.5, self.random() - 0.5) * size,
            Shape::Circle(r) => {
                let (a, d) = (self.random() * std::f32::consts::TAU, self.random().sqrt() * r);
                Vec2::from_angle(a) * d
            }
            Shape::Ring(r) => Vec2::from_angle(self.random() * std::f32::consts::TAU) * r,
            Shape::Line(len, a) => Vec2::from_angle(a) * (self.random() - 0.5) * len,
        };
        let dir = direction + (self.random() - 0.5) * spread;
        let v = Vec2::from_angle(dir) * self.between(speed);
        let p = Particle {
            pos: (center + offset).to_array(),
            vel: v.to_array(),
            birth,
            life: self.between(life).max(0.01),
            size: self.between(scale),
            spin: self.between(spin),
            angle: angle.unwrap_or_else(|| self.random() * std::f32::consts::TAU),
            seed: self.random(),
            tint,
            _pad: 0.0,
        };
        let i = self.cursor;
        self.records[i] = p;
        self.cursor = (i + 1) % self.records.len();
        self.dirty = Some(match self.dirty.take() {
            None => i..i + 1,
            Some(r) if r.end == i => r.start..i + 1,
            Some(r) => {
                // Wrapped past the end of the buffer: the range would not be contiguous.
                self.wrapped = true;
                r
            }
        });
    }

    /// The records written since the last call, as (first index, records): one range, or the
    /// whole buffer after a wrap or a clear.
    pub fn take_dirty(&mut self) -> Option<(usize, &[Particle])> {
        let range = if std::mem::take(&mut self.wrapped) {
            self.dirty = None;
            0..self.records.len()
        } else {
            self.dirty.take()?
        };
        Some((range.start, &self.records[range]))
    }

    /// Marks every record for upload (a new GPU buffer).
    pub fn mark_all_dirty(&mut self) {
        self.wrapped = true;
    }

    /// The uniform block for drawing with node-to-pixels transform `m`, node tint (linear,
    /// straight) and opacity.
    pub fn uniform(&self, m: Affine2, tint: Color, opacity: f32) -> ParticleUniform {
        let l = &self.look;
        let c = |c: Color| [c.r, c.g, c.b, c.a];
        let (x, y) = (m.matrix2.x_axis, m.matrix2.y_axis);
        ParticleUniform {
            axes: [x.x, x.y, y.x, y.y],
            origin_clock: [m.translation.x, m.translation.y, self.clock, opacity],
            motion: [l.gravity.x, l.gravity.y, l.drag.max(0.0), l.stretch.max(0.0)],
            wander_size: [l.wander.0, l.wander.1, l.size.0, l.size.1],
            colors: [c(l.colors[0]), c(l.colors[1]), c(l.colors[2])],
            fade_frames: [l.fade.0, l.fade.1, l.frames.0.max(1) as f32, l.frames.1.max(1) as f32],
            animate: [if l.animate { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            tint: c(tint),
            _pad: [0.0; 4],
        }
    }
}

fn srgb_bytes(c: Color) -> [u8; 4] {
    let enc = |v: f32| {
        let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.max(0.0).powf(1.0 / 2.4) - 0.055 };
        (s.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    [enc(c.r), enc(c.g), enc(c.b), (c.a.clamp(0.0, 1.0) * 255.0).round() as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(capacity: usize) -> ParticleSystem {
        let mut s = ParticleSystem::new(TextureId(0), capacity);
        s.seed(7);
        s
    }

    #[test]
    fn records_are_48_bytes() {
        assert_eq!(size_of::<Particle>(), 48);
        assert_eq!(size_of::<ParticleUniform>(), 11 * 16);
    }

    #[test]
    fn emits_at_the_rate_and_uploads_only_new_records() {
        let mut s = system(100);
        s.emitter.rate = 10.0;
        s.emitter.life = Span::fixed(5.0);
        s.step(0.5);
        let (first, recs) = s.take_dirty().unwrap();
        assert_eq!((first, recs.len()), (0, 5));
        // Births are spread over the step.
        assert!(recs[0].birth > 0.0 && recs[4].birth <= 0.5 + 1e-6);
        assert!(s.take_dirty().is_none(), "nothing new");
        s.step(0.25);
        s.step(0.05);
        let (first, recs) = s.take_dirty().unwrap();
        assert_eq!(first, 5);
        assert_eq!(recs.len(), 3);
        assert_eq!(s.alive(), 8);
        s.emitter.emitting = false;
        s.step(10.0);
        assert_eq!(s.alive(), 0, "all expired");
    }

    #[test]
    fn wrapping_uploads_the_whole_ring() {
        let mut s = system(4);
        s.burst(3, None);
        s.take_dirty();
        s.burst(2, Some(Vec2::new(10.0, 0.0)));
        let (first, recs) = s.take_dirty().unwrap();
        assert_eq!((first, recs.len()), (0, 4));
        assert_eq!(recs[0].pos, [10.0, 0.0], "the oldest record was replaced");
    }

    #[test]
    fn shapes_and_spans_stay_in_bounds() {
        let mut s = system(500);
        s.emitter.shape = Shape::Circle(20.0);
        s.emitter.speed = Span(10.0, 20.0);
        s.emitter.direction = 0.0;
        s.emitter.spread = 0.0;
        s.emitter.angle = None;
        s.emitter.tints = vec![Color::hex("#ff0000").unwrap()];
        s.burst(500, Some(Vec2::new(100.0, 50.0)));
        for p in s.records() {
            assert!(Vec2::from(p.pos).distance(Vec2::new(100.0, 50.0)) <= 20.0 + 1e-3);
            let v = Vec2::from(p.vel);
            assert!(v.x >= 10.0 - 1e-3 && v.x <= 20.0 + 1e-3 && v.y.abs() < 1e-3);
            assert!((0.0..1.0).contains(&p.seed));
            assert_eq!(p.tint, [255, 0, 0, 255]);
        }
    }

    #[test]
    fn warm_up_backdates_births_and_speed_scales_time() {
        let mut s = system(200);
        s.emitter.rate = 20.0;
        s.emitter.life = Span::fixed(10.0);
        s.warm_up(2.0);
        assert_eq!(s.alive(), 40);
        assert!(s.records()[0].birth < 0.0);
        s.speed = 0.0;
        s.step(1.0);
        assert_eq!(s.clock, 0.0, "paused");
        s.speed = 2.0;
        s.step(0.5);
        assert_eq!(s.clock, 1.0);
        let u = s.uniform(Affine2::from_translation(Vec2::new(3.0, 4.0)), Color::WHITE, 0.5);
        assert_eq!(u.origin_clock, [3.0, 4.0, 1.0, 0.5]);
        assert_eq!(u.axes, [1.0, 0.0, 0.0, 1.0]);
    }
}
