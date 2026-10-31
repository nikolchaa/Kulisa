//! The web: a `<video>` element, off the page. It keeps its own time and plays its own sound; each
//! new frame is copied to the sprite's texture.

use kulisa_audio::Audio;
use kulisa_platform::MediaLocation;
use wasm_bindgen::JsCast;
use web_sys::HtmlVideoElement;

use crate::{Info, Options};

/// `HTMLMediaElement.readyState` once the size and the current frame are known.
const HAVE_METADATA: u16 = 1;
const HAVE_CURRENT_DATA: u16 = 2;

/// A playing video.
pub struct Video {
    element: HtmlVideoElement,
    options: Options,
    info: Option<Info>,
    /// The time of the frame copied last.
    shown: Option<f64>,
    /// `play()` was called. Browsers only let sound start after a click, so it starts muted.
    started: bool,
    took_channel: bool,
    paused: bool,
    error: Option<String>,
}

impl Video {
    /// Starts loading `location`. It plays once [`Video::poll`] sees it has opened.
    pub fn open(location: MediaLocation, options: Options) -> Video {
        let MediaLocation::Url(url) = location;
        let element = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.create_element("video").ok())
            .and_then(|e| e.dyn_into::<HtmlVideoElement>().ok())
            .expect("a browser page can make video elements");
        element.set_muted(true);
        element.set_loop(options.looping);
        // Without this, iPhones take the video fullscreen.
        let _ = element.set_attribute("playsinline", "");
        element.set_preload("auto");
        element.set_src(&url);
        let paused = options.paused;
        Video {
            element,
            options,
            info: None,
            shown: None,
            started: false,
            took_channel: false,
            paused,
            error: None,
        }
    }

    pub fn element(&self) -> &HtmlVideoElement {
        &self.element
    }

    fn play(&mut self) {
        // The promise is only refused while sound is not allowed yet, which `poll` retries.
        let _ = self.element.play();
    }

    /// Keeps it going. True when a new frame is ready to be copied.
    pub fn poll(&mut self, audio: Option<&mut Audio>) -> bool {
        if self.error.is_none()
            && let Some(e) = self.element.error()
        {
            self.error = Some(format!("{}: {}", self.element.src(), e.message()));
        }
        if self.error.is_some() {
            return false;
        }
        let state = self.element.ready_state();
        if self.info.is_none() && state >= HAVE_METADATA {
            self.info = Some(Info {
                width: self.element.video_width(),
                height: self.element.video_height(),
                duration: self.element.duration(),
                audio_rate: None,
            });
        }
        if !self.started && !self.paused {
            self.started = true;
            self.play();
        }
        if let Some(audio) = audio {
            self.sound(audio);
        }
        let time = self.element.current_time();
        if state >= HAVE_CURRENT_DATA && self.shown != Some(time) && self.info.is_some() {
            self.shown = Some(time);
            return true;
        }
        false
    }

    /// Audio has started, so the player has clicked: sound can play, at the channel's volume.
    fn sound(&mut self, audio: &mut Audio) {
        let Some(channel) = self.options.channel.clone() else { return };
        if !self.took_channel {
            self.took_channel = true;
            // Like any new sound on a channel that holds one at a time; multi-sound channels
            // have no such rule, so stopping nothing there is harmless to try.
            if channel == "music" || channel == "voice" {
                let _ = audio.stop(&channel, 0.0);
            }
        }
        let volume = audio.volume(&channel).unwrap_or(1.0) * audio.master_volume() * self.options.volume;
        self.element.set_volume(f64::from(volume));
        self.element.set_muted(false);
        if !self.paused && self.element.paused() && !self.element.ended() {
            self.play();
        }
    }

    pub fn info(&self) -> Option<Info> {
        self.info
    }

    pub fn time(&self) -> f64 {
        self.element.current_time()
    }

    pub fn done(&self) -> bool {
        self.error.is_some() || self.element.ended()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            let _ = self.element.pause();
        } else {
            self.play();
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.options.volume = volume;
    }

    pub fn seek(&mut self, time: f64, _audio: Option<&mut Audio>) {
        self.element.set_current_time(time);
        self.shown = None;
        if !self.paused {
            self.play();
        }
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        let _ = self.element.pause();
        // Lets the browser stop downloading it.
        let _ = self.element.remove_attribute("src");
        self.element.load();
    }
}
