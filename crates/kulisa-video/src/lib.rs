//! Video playback. Pictures come from what each platform already has: Media Foundation on
//! Windows, AVFoundation on Apple systems, MediaCodec on Android and `<video>` on the web. Linux
//! has no decoder every system is sure to have, so `kulisa build linux` converts videos to AV1
//! and Ogg, which [`av1`] plays.
//!
//! Natively, a thread decodes ahead while the game takes the frame that is due. The clock is the
//! sound being heard, so pictures follow it; without sound it is the frame time.

#[cfg(not(target_arch = "wasm32"))]
mod player;
#[cfg(not(target_arch = "wasm32"))]
pub use player::Video;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::Video;

mod yuv;

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
pub use android::set_asset_manager;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(any(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))), test))]
mod av1;
#[cfg(windows)]
mod windows;

/// What is known once a video has opened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    /// Seconds.
    pub duration: f64,
    /// Frames per second of its sound, when it has any.
    pub audio_rate: Option<u32>,
}

/// How a video plays.
#[derive(Clone, Debug)]
pub struct Options {
    pub looping: bool,
    pub paused: bool,
    /// The audio channel its sound plays on; `None` leaves its sound out.
    pub channel: Option<String>,
    /// 0 to 1.
    pub volume: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self { looping: false, paused: false, channel: Some("music".into()), volume: 1.0 }
    }
}

/// A platform's decoder for one file. It lives on the decoding thread. Timestamps are seconds from
/// the start of the video.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) trait Source {
    fn info(&self) -> Info;

    /// Decodes the next picture as RGBA, `width * height * 4` bytes. `None` at the end.
    fn read_video(&mut self, rgba: &mut Vec<u8>) -> Result<Option<f64>, String>;

    /// Adds the next piece of sound as stereo frames and says when it starts. `None` at the end.
    fn read_audio(&mut self, out: &mut Vec<[f32; 2]>) -> Result<Option<f64>, String>;

    /// Moves to a point at or before `time`, usually the key frame before it.
    fn seek(&mut self, time: f64) -> Result<(), String>;
}

/// Opens `location` with this platform's decoder.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn open(location: &kulisa_platform::MediaLocation) -> Result<Box<dyn Source>, String> {
    #[cfg(windows)]
    return windows::open(location);
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    return apple::open(location);
    #[cfg(target_os = "android")]
    return android::open(location);
    #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
    return av1::open(location);
}

/// The platform's real decoder on a real file (Linux reads the converted files beside it).
#[cfg(all(test, not(any(target_arch = "wasm32", target_os = "android", target_os = "ios"))))]
mod tests {
    use super::*;
    use kulisa_platform::MediaLocation;

    fn clip() -> MediaLocation {
        MediaLocation::File(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/clip.mp4"))
    }

    #[test]
    fn the_system_decoder_plays_the_clip() {
        let mut source = open(&clip()).unwrap();
        let info = source.info();
        assert_eq!((info.width, info.height), (160, 90));
        assert!((info.duration - 2.0).abs() < 0.1, "{}", info.duration);
        let rate = info.audio_rate.expect("the clip has sound");

        let mut rgba = Vec::new();
        let (mut count, mut last) = (0, -1.0);
        while let Some(t) = source.read_video(&mut rgba).unwrap() {
            assert_eq!(rgba.len(), 160 * 90 * 4);
            // The middle of the picture: red for a second, then blue.
            let px = &rgba[(45 * 160 + 80) * 4..][..4];
            if t < 0.9 {
                assert!(px[0] > 200 && px[2] < 60, "not red at {t}: {px:?}");
            } else if t > 1.1 {
                assert!(px[2] > 200 && px[0] < 60, "not blue at {t}: {px:?}");
            }
            assert!(t > last, "{t} came after {last}");
            last = t;
            count += 1;
        }
        assert_eq!(count, 30, "every picture came out");

        let mut sound = Vec::new();
        let mut first = None;
        while let Some(t) = source.read_audio(&mut sound).unwrap() {
            first.get_or_insert(t);
        }
        assert!(first.unwrap() < 0.1);
        let seconds = sound.len() as f64 / f64::from(rate);
        assert!((seconds - 2.0).abs() < 0.15, "{seconds} seconds of sound");
        assert!(sound.iter().any(|s| s[0].abs() > 0.05), "there is a tone");

        source.seek(1.5).unwrap();
        let t = source.read_video(&mut rgba).unwrap().expect("a picture after seeking");
        assert!(t <= 1.6, "{t}");
        assert!(source.read_audio(&mut sound).unwrap().is_some(), "sound after seeking");
    }

    #[test]
    fn a_missing_file_is_an_error() {
        let missing = MediaLocation::File("nope/missing.mp4".into());
        assert!(open(&missing).is_err());
    }
}
