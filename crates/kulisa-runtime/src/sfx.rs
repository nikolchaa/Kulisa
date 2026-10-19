//! Synthesized placeholder sounds for the smoke scene.

use kulisa_audio::{Sound, wav};

pub struct Sfx {
    pub blip: Sound,
    pub pad: Sound,
}

const RATE: u32 = 22_050;

/// A soft A-major pad that loops seamlessly: every partial completes whole cycles in 4 s.
fn pad() -> Vec<u8> {
    let seconds = 4.0;
    let n = (seconds * RATE as f32) as usize;
    let partials = [(220.0, 0.30), (277.25, 0.22), (329.5, 0.20), (440.0, 0.10)];
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let swell = 0.75 + 0.25 * (t * std::f32::consts::TAU / seconds).sin();
            partials.iter().map(|(f, a)| (t * f * std::f32::consts::TAU).sin() * a).sum::<f32>() * swell * 0.5
        })
        .collect();
    wav::encode_mono16(&samples, RATE)
}

impl Sfx {
    pub fn new() -> Result<Self, String> {
        Ok(Self { blip: Sound::decode(wav::blip(880.0, 0.12, RATE))?, pad: Sound::decode(pad())? })
    }
}
