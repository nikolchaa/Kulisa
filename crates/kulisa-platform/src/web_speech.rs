//! Text to speech in the browser.

use web_sys::SpeechSynthesisUtterance;

fn synthesis() -> Option<web_sys::SpeechSynthesis> {
    web_sys::window()?.speech_synthesis().ok()
}

pub fn web_speech_available() -> bool {
    synthesis().is_some()
}

pub fn web_speak(text: &str, language: &str, interrupt: bool, volume: f32) {
    let Some(speech) = synthesis() else { return };
    if interrupt {
        speech.cancel();
    }
    if let Ok(utterance) = SpeechSynthesisUtterance::new_with_text(text) {
        utterance.set_lang(language);
        utterance.set_volume(volume);
        speech.speak(&utterance);
    }
}

pub fn web_speech_stop() {
    if let Some(speech) = synthesis() {
        speech.cancel();
    }
}
