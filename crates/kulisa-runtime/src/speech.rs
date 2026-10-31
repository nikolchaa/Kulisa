//! Text to speech with the voices the system already has: SAPI on Windows, AVSpeechSynthesizer on
//! Apple platforms, `TextToSpeech` on Android, speech-dispatcher on Linux, the browser's on the web.

use kulisa_lua::SpeechRequest;

#[cfg(windows)]
mod backend {
    use windows::Win32::Media::Speech::{ISpVoice, SPF_ASYNC, SPF_IS_NOT_XML, SPF_PURGEBEFORESPEAK, SpVoice};
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::core::PCWSTR;

    #[derive(Default)]
    pub struct Backend {
        voice: Option<ISpVoice>,
    }

    pub fn available() -> bool {
        true
    }

    impl Backend {
        fn voice(&mut self) -> Option<&ISpVoice> {
            if self.voice.is_none() {
                // SAFETY: plain COM setup on the thread that keeps using the voice. It may already
                // be initialized on this thread, which is fine.
                let voice = unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                    CoCreateInstance(&SpVoice, None, CLSCTX_ALL)
                };
                match voice {
                    Ok(v) => self.voice = Some(v),
                    Err(e) => log::warn!("speech: no voice: {e}"),
                }
            }
            self.voice.as_ref()
        }

        pub fn say(&mut self, text: &str, _language: &str, interrupt: bool, volume: f32) {
            let Some(voice) = self.voice() else { return };
            let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
            let mut flags = SPF_ASYNC.0 as u32 | SPF_IS_NOT_XML.0 as u32;
            if interrupt {
                flags |= SPF_PURGEBEFORESPEAK.0 as u32;
            }
            // SAFETY: `wide` is NUL-terminated and SAPI copies it for asynchronous speech.
            let result = unsafe {
                let _ = voice.SetVolume((volume * 100.0) as u16);
                voice.Speak(PCWSTR(wide.as_ptr()), flags, None)
            };
            match result {
                Ok(()) => log::debug!("speech: speaking {} characters", text.chars().count()),
                Err(e) => log::warn!("speech: {e}"),
            }
        }

        pub fn stop(&mut self) {
            let flags = SPF_ASYNC.0 as u32 | SPF_PURGEBEFORESPEAK.0 as u32;
            if let Some(voice) = &self.voice {
                // SAFETY: empty text with the purge flag stops what is being said.
                let _ = unsafe { voice.Speak(PCWSTR::null(), flags, None) };
            }
        }

        pub fn poll(&mut self) {}
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod backend {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send, msg_send_id};
    use objc2_foundation::NSString;

    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {}

    #[derive(Default)]
    pub struct Backend {
        synth: Option<Retained<AnyObject>>,
    }

    pub fn available() -> bool {
        true
    }

    impl Backend {
        pub fn say(&mut self, text: &str, language: &str, interrupt: bool, volume: f32) {
            if interrupt {
                self.stop();
            }
            let text = NSString::from_str(text);
            let language = NSString::from_str(language);
            // SAFETY: messages as documented for AVSpeechSynthesizer, AVSpeechUtterance and
            // AVSpeechSynthesisVoice, with their documented argument types.
            unsafe {
                let synth = self.synth.get_or_insert_with(|| msg_send_id![class!(AVSpeechSynthesizer), new]);
                let utterance: Retained<AnyObject> =
                    msg_send_id![class!(AVSpeechUtterance), speechUtteranceWithString: &*text];
                let voice: Option<Retained<AnyObject>> =
                    msg_send_id![class!(AVSpeechSynthesisVoice), voiceWithLanguage: &*language];
                if let Some(voice) = &voice {
                    let _: () = msg_send![&*utterance, setVoice: &**voice];
                }
                let _: () = msg_send![&*utterance, setVolume: volume];
                let _: () = msg_send![&**synth, speakUtterance: &*utterance];
            }
        }

        pub fn stop(&mut self) {
            if let Some(synth) = &self.synth {
                // SAFETY: as above; 0 is AVSpeechBoundaryImmediate.
                let _: bool = unsafe { msg_send![&**synth, stopSpeakingAtBoundary: 0isize] };
            }
        }

        pub fn poll(&mut self) {}
    }
}

#[cfg(target_os = "android")]
mod backend {
    use crate::android::{self, Tts};

    #[derive(Default)]
    pub struct Backend {
        tts: Option<Tts>,
        /// What waits for the service, which connects a moment after it is created.
        pending: Option<(String, String, bool, f32, u32)>,
    }

    /// Frames to keep trying while the service connects.
    const PATIENCE: u32 = 600;

    pub fn available() -> bool {
        true
    }

    impl Backend {
        pub fn say(&mut self, text: &str, language: &str, interrupt: bool, volume: f32) {
            self.pending = Some((text.to_string(), language.to_string(), interrupt, volume, PATIENCE));
            self.poll();
        }

        pub fn stop(&mut self) {
            self.pending = None;
            if let (Some(tts), Some(app)) = (&self.tts, android::app()) {
                android::tts_stop(&app, tts);
            }
        }

        pub fn poll(&mut self) {
            let Some((text, language, interrupt, volume, left)) = &mut self.pending else { return };
            let Some(app) = android::app() else { return };
            if self.tts.is_none() {
                self.tts = android::tts_create(&app);
            }
            let Some(tts) = &self.tts else {
                self.pending = None;
                return log::warn!("speech: this device has no text to speech");
            };
            if android::tts_say(&app, tts, text, language, *interrupt, *volume) {
                self.pending = None;
            } else if *left == 0 {
                self.pending = None;
                log::warn!("speech: the text to speech service did not connect");
            } else {
                *left -= 1;
            }
        }
    }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
mod backend {
    use std::process::{Child, Command, Stdio};

    #[derive(Default)]
    pub struct Backend {
        running: Option<Child>,
    }

    fn spd_say() -> Command {
        let mut c = Command::new("spd-say");
        c.stdout(Stdio::null()).stderr(Stdio::null());
        c
    }

    pub fn available() -> bool {
        spd_say().arg("--version").status().is_ok_and(|s| s.success())
    }

    impl Backend {
        pub fn say(&mut self, text: &str, language: &str, interrupt: bool, volume: f32) {
            if interrupt {
                self.stop();
            }
            // -100 to 100, where 0 is the default loudness.
            let loudness = ((volume.clamp(0.0, 1.0) * 200.0) as i32 - 100).to_string();
            match spd_say().args(["-l", language, "-i", &loudness, "--", text]).spawn() {
                Ok(child) => self.running = Some(child),
                Err(e) => log::warn!("speech: spd-say: {e}"),
            }
        }

        pub fn stop(&mut self) {
            let _ = spd_say().arg("-C").status();
        }

        /// Collects the finished `spd-say`.
        pub fn poll(&mut self) {
            if let Some(child) = &mut self.running
                && !matches!(child.try_wait(), Ok(None))
            {
                self.running = None;
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod backend {
    #[derive(Default)]
    pub struct Backend;

    pub fn available() -> bool {
        kulisa_platform::web_speech_available()
    }

    impl Backend {
        pub fn say(&mut self, text: &str, language: &str, interrupt: bool, volume: f32) {
            kulisa_platform::web_speak(text, language, interrupt, volume);
        }

        pub fn stop(&mut self) {
            kulisa_platform::web_speech_stop();
        }

        pub fn poll(&mut self) {}
    }
}

pub use backend::available;

#[derive(Default)]
pub struct Speech {
    backend: backend::Backend,
}

impl Speech {
    pub fn apply(&mut self, requests: Vec<SpeechRequest>) {
        for request in requests {
            match request {
                SpeechRequest::Say { text, language, interrupt, volume } => {
                    self.backend.say(&text, &language, interrupt, volume);
                }
                SpeechRequest::Stop => self.backend.stop(),
            }
        }
    }

    /// Called every frame.
    pub fn poll(&mut self) {
        self.backend.poll();
    }
}
