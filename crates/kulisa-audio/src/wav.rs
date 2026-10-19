//! Tiny WAV encoder for synthesized placeholder sounds and tests.

/// Encodes mono samples in -1..=1 as a 16-bit PCM WAV file.
pub fn encode_mono16(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

/// A short percussive tone: a sine at `freq` Hz with an exponential decay.
pub fn blip(freq: f32, seconds: f32, sample_rate: u32) -> Vec<u8> {
    let n = (seconds * sample_rate as f32) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (t * freq * std::f32::consts::TAU).sin() * (-t * 6.0 / seconds).exp() * 0.5
        })
        .collect();
    encode_mono16(&samples, sample_rate)
}
