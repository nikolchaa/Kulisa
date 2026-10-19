//! Audio channels on top of kira. Each channel is a mixer track with its own volume, so a
//! settings screen (a template, not the engine) can bind sliders to them.
//!
//! Built-in channels: `music` and `voice` play one sound at a time (a new one crossfades
//! out the old), `sound` overlaps freely. Games add their own with [`Audio::add_channel`].

pub mod wav;

use std::collections::HashMap;
use std::io::Cursor;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use std::time::Duration;

use kira::backend::{Backend, DefaultBackend};
use kira::sound::PlaybackState;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, Tween};

#[cfg(not(target_arch = "wasm32"))]
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};

/// Decoded or streamable audio. Cheap to clone.
#[derive(Clone)]
pub struct Sound(Kind);

#[derive(Clone)]
enum Kind {
    Static(Box<StaticSoundData>),
    /// Encoded bytes, decoded while playing. Native only (kira streams on a thread).
    #[cfg(not(target_arch = "wasm32"))]
    Stream(Arc<[u8]>),
}

impl Sound {
    /// Decodes the whole file up front. Best for short sounds (effects, voice lines).
    pub fn decode(bytes: Vec<u8>) -> Result<Sound, String> {
        let data = StaticSoundData::from_cursor(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        Ok(Sound(Kind::Static(Box::new(data))))
    }

    /// Decodes while playing, keeping memory low for long music. On the web, where kira
    /// cannot stream, this decodes up front instead.
    pub fn stream(bytes: Vec<u8>) -> Result<Sound, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let bytes: Arc<[u8]> = bytes.into();
            // Validate now so errors surface at load time, not at play time.
            StreamingSoundData::from_cursor(Cursor::new(bytes.clone())).map_err(|e| e.to_string())?;
            Ok(Sound(Kind::Stream(bytes)))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::decode(bytes)
        }
    }

    /// Length in seconds, when known up front.
    pub fn duration(&self) -> Option<f64> {
        match &self.0 {
            Kind::Static(d) => Some(d.duration().as_secs_f64()),
            #[cfg(not(target_arch = "wasm32"))]
            Kind::Stream(_) => None,
        }
    }
}

/// How a channel treats a new sound while another is playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    /// One sound at a time; playing a new one fades out the current one (music, voice).
    Single,
    /// Sounds overlap (effects, UI).
    Multi,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayOptions {
    /// Linear volume, 0..=1.
    pub volume: f32,
    pub looping: bool,
    /// Seconds to fade in.
    pub fade_in: f32,
    /// On single-sound channels: seconds to fade out what was playing.
    pub crossfade: f32,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self { volume: 1.0, looping: false, fade_in: 0.0, crossfade: 0.0 }
    }
}

enum Playing {
    Static(StaticSoundHandle),
    #[cfg(not(target_arch = "wasm32"))]
    Stream(StreamingSoundHandle<kira::sound::FromFileError>),
}

impl Playing {
    fn state(&self) -> PlaybackState {
        match self {
            Playing::Static(h) => h.state(),
            #[cfg(not(target_arch = "wasm32"))]
            Playing::Stream(h) => h.state(),
        }
    }

    fn stop(&mut self, tween: Tween) {
        match self {
            Playing::Static(h) => h.stop(tween),
            #[cfg(not(target_arch = "wasm32"))]
            Playing::Stream(h) => h.stop(tween),
        }
    }
}

struct Channel {
    kind: ChannelKind,
    track: TrackHandle,
    volume: f32,
    playing: Vec<Playing>,
}

/// Linear gain (0..=1) to decibels, with silence below -60 dB.
pub fn to_decibels(linear: f32) -> Decibels {
    if linear <= 0.001 { Decibels::SILENCE } else { Decibels(20.0 * linear.log10()) }
}

fn tween(seconds: f32) -> Tween {
    Tween { duration: Duration::from_secs_f32(seconds.max(0.0)), ..Default::default() }
}

/// The audio system: named channels on one kira manager.
pub struct Audio<B: Backend = DefaultBackend> {
    manager: AudioManager<B>,
    channels: HashMap<String, Channel>,
    master: f32,
}

impl Audio {
    /// Opens the default output device.
    pub fn new() -> Result<Self, String> {
        Self::with_settings(AudioManagerSettings::default())
    }
}

impl<B: Backend> Audio<B>
where
    B::Error: std::fmt::Debug,
{
    /// Uses a specific kira backend (tests use the mock backend).
    pub fn with_settings(settings: AudioManagerSettings<B>) -> Result<Self, String> {
        let manager = AudioManager::<B>::new(settings).map_err(|e| format!("audio: {e:?}"))?;
        let mut audio = Self { manager, channels: HashMap::new(), master: 1.0 };
        audio.add_channel("music", ChannelKind::Single)?;
        audio.add_channel("voice", ChannelKind::Single)?;
        audio.add_channel("sound", ChannelKind::Multi)?;
        Ok(audio)
    }

    /// Adds a channel. Re-adding an existing name keeps the old channel.
    pub fn add_channel(&mut self, name: &str, kind: ChannelKind) -> Result<(), String> {
        if self.channels.contains_key(name) {
            return Ok(());
        }
        let track = self.manager.add_sub_track(TrackBuilder::new()).map_err(|e| format!("audio: {e:?}"))?;
        self.channels.insert(name.to_string(), Channel { kind, track, volume: 1.0, playing: Vec::new() });
        Ok(())
    }

    pub fn channels(&self) -> impl Iterator<Item = &str> {
        self.channels.keys().map(String::as_str)
    }

    fn channel(&mut self, name: &str) -> Result<&mut Channel, String> {
        self.channels.get_mut(name).ok_or_else(|| format!("no audio channel named `{name}`"))
    }

    /// Plays `sound` on `channel`.
    pub fn play(&mut self, channel: &str, sound: &Sound, opts: PlayOptions) -> Result<(), String> {
        let ch = self.channel(channel)?;
        ch.playing.retain(|p| p.state() != PlaybackState::Stopped);
        if ch.kind == ChannelKind::Single {
            for p in &mut ch.playing {
                p.stop(tween(opts.crossfade));
            }
        }
        let volume = to_decibels(opts.volume);
        let fade = (opts.fade_in > 0.0).then(|| tween(opts.fade_in));
        let handle = match &sound.0 {
            Kind::Static(data) => {
                let mut d = data.volume(volume).fade_in_tween(fade);
                if opts.looping {
                    d = d.loop_region(..);
                }
                Playing::Static(ch.track.play(d).map_err(|e| format!("audio: {e:?}"))?)
            }
            #[cfg(not(target_arch = "wasm32"))]
            Kind::Stream(bytes) => {
                let mut d = StreamingSoundData::from_cursor(Cursor::new(bytes.clone()))
                    .map_err(|e| e.to_string())?
                    .volume(volume)
                    .fade_in_tween(fade);
                if opts.looping {
                    d = d.loop_region(..);
                }
                Playing::Stream(ch.track.play(d).map_err(|e| format!("audio: {e:?}"))?)
            }
        };
        ch.playing.push(handle);
        Ok(())
    }

    /// Stops everything on `channel`, fading out over `fade` seconds.
    pub fn stop(&mut self, channel: &str, fade: f32) -> Result<(), String> {
        for p in &mut self.channel(channel)?.playing {
            p.stop(tween(fade));
        }
        Ok(())
    }

    /// True while anything on `channel` is audible (including fade-outs). Story code uses
    /// this on `voice` for auto-advance.
    pub fn is_playing(&mut self, channel: &str) -> bool {
        match self.channels.get_mut(channel) {
            Some(ch) => {
                ch.playing.retain(|p| p.state() != PlaybackState::Stopped);
                !ch.playing.is_empty()
            }
            None => false,
        }
    }

    /// Sets a channel's linear volume (0..=1) over `fade` seconds.
    pub fn set_volume(&mut self, channel: &str, volume: f32, fade: f32) -> Result<(), String> {
        let ch = self.channel(channel)?;
        ch.volume = volume.clamp(0.0, 1.0);
        ch.track.set_volume(to_decibels(ch.volume), tween(fade));
        Ok(())
    }

    pub fn volume(&self, channel: &str) -> Option<f32> {
        self.channels.get(channel).map(|c| c.volume)
    }

    /// Master volume over all channels (linear 0..=1).
    pub fn set_master_volume(&mut self, volume: f32, fade: f32) {
        self.master = volume.clamp(0.0, 1.0);
        self.manager.main_track().set_volume(to_decibels(self.master), tween(fade));
    }

    pub fn master_volume(&self) -> f32 {
        self.master
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kira::backend::mock::{MockBackend, MockBackendSettings};

    fn audio() -> Audio<MockBackend> {
        Audio::with_settings(AudioManagerSettings {
            backend_settings: MockBackendSettings { sample_rate: 48_000 },
            ..Default::default()
        })
        .unwrap()
    }

    fn blip() -> Sound {
        Sound::decode(wav::blip(440.0, 0.25, 22050)).unwrap()
    }

    #[test]
    fn has_builtin_channels_and_custom_ones() {
        let mut a = audio();
        let mut names: Vec<_> = a.channels().collect();
        names.sort();
        assert_eq!(names, ["music", "sound", "voice"]);
        a.add_channel("ambience", ChannelKind::Multi).unwrap();
        assert!(a.play("ambience", &blip(), PlayOptions::default()).is_ok());
        assert!(a.play("nope", &blip(), PlayOptions::default()).unwrap_err().contains("nope"));
    }

    #[test]
    fn decodes_wav_and_rejects_garbage() {
        let s = blip();
        assert!((s.duration().unwrap() - 0.25).abs() < 0.01);
        assert!(Sound::decode(b"definitely not audio".to_vec()).is_err());
        assert!(Sound::stream(b"definitely not audio".to_vec()).is_err());
    }

    /// Lets the mock audio thread apply queued commands.
    fn process(a: &mut Audio<MockBackend>) {
        for _ in 0..4 {
            a.manager.backend_mut().on_start_processing();
            a.manager.backend_mut().process();
        }
    }

    #[test]
    fn single_channels_replace_multi_channels_overlap() {
        let mut a = audio();
        let s = Sound::decode(wav::blip(440.0, 5.0, 22050)).unwrap();
        for _ in 0..3 {
            a.play("sound", &s, PlayOptions::default()).unwrap();
            a.play("music", &s, PlayOptions::default()).unwrap();
            process(&mut a);
        }
        assert!(a.is_playing("music") && a.is_playing("sound"));
        assert_eq!(a.channels["sound"].playing.len(), 3, "effects overlap");
        assert_eq!(a.channels["music"].playing.len(), 1, "earlier music was stopped and pruned");
        a.stop("sound", 0.0).unwrap();
        process(&mut a);
        assert!(!a.is_playing("sound"));
    }

    #[test]
    fn volumes_are_tracked_and_clamped() {
        let mut a = audio();
        a.set_volume("music", 0.25, 0.0).unwrap();
        assert_eq!(a.volume("music"), Some(0.25));
        a.set_volume("voice", 7.0, 0.0).unwrap();
        assert_eq!(a.volume("voice"), Some(1.0));
        a.set_master_volume(-1.0, 0.0);
        assert_eq!(a.master_volume(), 0.0);
        assert_eq!(to_decibels(1.0), Decibels(0.0));
        assert_eq!(to_decibels(0.0), Decibels::SILENCE);
        assert!((to_decibels(0.5).0 - -6.02).abs() < 0.01);
    }

    #[test]
    fn streams_play_on_native() {
        let mut a = audio();
        let s = Sound::stream(wav::blip(220.0, 0.5, 22050)).unwrap();
        a.play("music", &s, PlayOptions { looping: true, ..Default::default() }).unwrap();
        assert!(a.is_playing("music"));
        a.stop("music", 0.0).unwrap();
    }
}
