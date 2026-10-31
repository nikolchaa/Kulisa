//! Samples pushed from code instead of read from a file: a video's sound. How many have been
//! heard is the clock its pictures follow.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use kira::Frame;
use kira::info::Info;
use kira::sound::{Sound, SoundData};
use rtrb::{Consumer, Producer, RingBuffer};

/// Seconds of sound the ring holds.
const RING_SECONDS: usize = 2;
/// How fast the gain follows changes, per output sample: quick, without clicks.
const GAIN_STEP: f32 = 0.002;

#[derive(Default)]
struct Shared {
    written: AtomicU64,
    heard: AtomicU64,
    paused: AtomicBool,
    /// Keeps counting while silent: the sound of a video goes on as its clock.
    muted: AtomicBool,
    closed: AtomicBool,
    /// Linear, as `f32` bits.
    volume: AtomicU32,
}

/// Controls a playing stream from the game's side.
pub struct PcmHandle {
    shared: Arc<Shared>,
    rate: u32,
}

impl PcmHandle {
    /// Seconds of sound heard so far.
    pub fn position(&self) -> f64 {
        self.shared.heard.load(Ordering::Relaxed) as f64 / f64::from(self.rate)
    }

    /// Seconds written but not heard yet.
    pub fn buffered(&self) -> f64 {
        let written = self.shared.written.load(Ordering::Relaxed);
        written.saturating_sub(self.shared.heard.load(Ordering::Relaxed)) as f64 / f64::from(self.rate)
    }

    pub fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Relaxed);
    }

    pub fn set_volume(&self, volume: f32) {
        self.shared.volume.store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Ends the sound for good.
    pub fn close(&self) {
        self.shared.closed.store(true, Ordering::Relaxed);
    }

    pub(crate) fn share(&self) -> PcmHandle {
        PcmHandle { shared: self.shared.clone(), rate: self.rate }
    }

    /// Silences it, for example when something else takes over its channel.
    pub(crate) fn mute(&self) {
        self.shared.muted.store(true, Ordering::Relaxed);
    }

    pub(crate) fn audible(&self) -> bool {
        !self.shared.muted.load(Ordering::Relaxed) && !self.shared.closed.load(Ordering::Relaxed)
    }
}

/// Fills a playing stream, from any thread.
pub struct PcmWriter {
    producer: Producer<[f32; 2]>,
    shared: Arc<Shared>,
    rate: u32,
}

impl PcmWriter {
    /// How many frames fit right now.
    pub fn room(&self) -> usize {
        self.producer.slots()
    }

    /// Seconds written but not heard yet.
    pub fn buffered(&self) -> f64 {
        let written = self.shared.written.load(Ordering::Relaxed);
        written.saturating_sub(self.shared.heard.load(Ordering::Relaxed)) as f64 / f64::from(self.rate)
    }

    /// Writes as many frames as fit and says how many that was.
    pub fn write(&mut self, frames: &[[f32; 2]]) -> usize {
        let mut n = 0;
        for f in frames {
            if self.producer.push(*f).is_err() {
                break;
            }
            n += 1;
        }
        self.shared.written.fetch_add(n as u64, Ordering::Relaxed);
        n
    }
}

struct PcmSound {
    consumer: Consumer<[f32; 2]>,
    shared: Arc<Shared>,
    rate: f64,
    /// The two frames being blended, and how far between them this output sample is.
    from: [f32; 2],
    to: [f32; 2],
    phase: f64,
    started: bool,
    gain: f32,
}

impl PcmSound {
    fn next(&mut self) -> bool {
        match self.consumer.pop() {
            Ok(f) => {
                self.from = self.to;
                self.to = f;
                self.shared.heard.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(_) => false,
        }
    }
}

impl Sound for PcmSound {
    fn process(&mut self, out: &mut [Frame], dt: f64, _info: &Info) {
        let paused = self.shared.paused.load(Ordering::Relaxed);
        let target =
            if self.shared.muted.load(Ordering::Relaxed) || self.shared.closed.load(Ordering::Relaxed) {
                0.0
            } else {
                f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
            };
        for o in out {
            self.gain += (target - self.gain).clamp(-GAIN_STEP, GAIN_STEP);
            if paused {
                *o = Frame::ZERO;
                continue;
            }
            if !self.started {
                self.started = self.next();
                if !self.started {
                    *o = Frame::ZERO;
                    continue;
                }
            }
            self.phase += dt * self.rate;
            let mut fed = true;
            while self.phase >= 1.0 && fed {
                fed = self.next();
                if fed {
                    self.phase -= 1.0;
                }
            }
            if !fed {
                // Ran dry: hold still and wait, so the clock stops rather than the sound skipping.
                self.phase = self.phase.min(1.0);
                *o = Frame::ZERO;
                continue;
            }
            let t = self.phase as f32;
            *o = Frame::new(
                (self.from[0] + (self.to[0] - self.from[0]) * t) * self.gain,
                (self.from[1] + (self.to[1] - self.from[1]) * t) * self.gain,
            );
        }
    }

    fn finished(&self) -> bool {
        self.shared.closed.load(Ordering::Relaxed) && self.gain <= 0.0
    }
}

pub(crate) struct PcmData {
    pub sound: Box<dyn Sound>,
    pub handle: PcmHandle,
}

impl SoundData for PcmData {
    type Error = std::convert::Infallible;
    type Handle = PcmHandle;

    fn into_sound(self) -> Result<(Box<dyn Sound>, PcmHandle), Self::Error> {
        Ok((self.sound, self.handle))
    }
}

/// A new stream at `rate` frames per second, and what fills it.
pub(crate) fn stream(rate: u32, volume: f32) -> (PcmData, PcmWriter) {
    let (producer, consumer) = RingBuffer::new(rate as usize * RING_SECONDS);
    let shared = Arc::new(Shared::default());
    shared.volume.store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    let sound = PcmSound {
        consumer,
        shared: shared.clone(),
        rate: f64::from(rate),
        from: [0.0; 2],
        to: [0.0; 2],
        phase: 0.0,
        started: false,
        gain: volume.clamp(0.0, 1.0),
    };
    let handle = PcmHandle { shared: shared.clone(), rate };
    (PcmData { sound: Box::new(sound), handle }, PcmWriter { producer, shared, rate })
}
