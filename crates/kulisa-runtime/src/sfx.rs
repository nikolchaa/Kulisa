//! Synthesized placeholder sounds for the smoke scene.

use kulisa_audio::{Sound, wav};

pub struct Sfx {
    pub blip: Sound,
    pad: Sound,
    night: Sound,
}

const RATE: u32 = 22_050;
const LOOP_SECONDS: f32 = 4.0;

/// A soft pad that loops seamlessly: every partial completes whole cycles in 4 s, so pick
/// frequencies that are multiples of 0.25 Hz.
fn pad(partials: &[(f32, f32)]) -> Vec<u8> {
    let n = (LOOP_SECONDS * RATE as f32) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let swell = 0.75 + 0.25 * (t * std::f32::consts::TAU / LOOP_SECONDS).sin();
            partials.iter().map(|(f, a)| (t * f * std::f32::consts::TAU).sin() * a).sum::<f32>() * swell * 0.5
        })
        .collect();
    wav::encode_mono16(&samples, RATE)
}

impl Sfx {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            blip: Sound::decode(wav::blip(880.0, 0.12, RATE))?,
            // A major, then F# minor for the "night" track.
            pad: Sound::decode(pad(&[(220.0, 0.30), (277.25, 0.22), (329.5, 0.20), (440.0, 0.10)]))?,
            night: Sound::decode(pad(&[(185.0, 0.30), (220.0, 0.22), (277.25, 0.20), (370.0, 0.10)]))?,
        })
    }

    /// The sound a story file name refers to.
    pub fn get(&self, file: &str) -> Option<&Sound> {
        match file {
            "pad" => Some(&self.pad),
            "night" => Some(&self.night),
            "blip" => Some(&self.blip),
            _ => None,
        }
    }
}
