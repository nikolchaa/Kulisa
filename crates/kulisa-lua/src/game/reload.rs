//! Live reload. Shaders, images, sounds and stories change in place while the game runs;
//! anything else (Luau, `kulisa.toml`, fonts, data files) needs a restart, after which the
//! game gets a `reload` event with a save of where the story was.

use std::sync::Arc;

use kulisa_audio::PlayOptions;
use kulisa_core::{UVec2, Value};
use kulisa_render::Image;
use kulisa_story::Vm;

use super::api::{compile_story, write_save};
use super::{Game, Waiting, emit, report, story_restored, with_story};

/// The save slot the story goes to across a restart (hidden from `kulisa.save.list`).
pub const RELOAD_SLOT: &str = "_reload";

/// What a changed file means for a running game.
#[derive(Debug, PartialEq, Eq)]
enum Change {
    Shader,
    Image,
    Sound,
    Story,
    Restart,
}

fn change(path: &str) -> Change {
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e).to_ascii_lowercase();
    match ext.as_str() {
        "wgsl" => Change::Shader,
        "png" | "jpg" | "jpeg" | "webp" => Change::Image,
        "wav" | "ogg" | "mp3" | "flac" => Change::Sound,
        "kulisa" => Change::Story,
        _ => Change::Restart,
    }
}

impl Game {
    /// Swaps the game's files for a newer copy (a new `.kpak` on the web); follow with
    /// [`Game::reload`] for what changed.
    pub fn set_assets(&mut self, assets: kulisa_platform::Assets) {
        self.state.borrow_mut().assets = assets;
    }

    /// Hides the error view and forgets its errors.
    pub fn clear_errors(&mut self) {
        self.state.borrow_mut().errors.hide();
    }

    /// Applies changed game files (paths relative to the game). Returns true when the game
    /// must restart instead ([`Game::save_for_restart`], a new [`Game`], then
    /// [`Game::restarted`]). Errors go to the error view; what failed keeps its old version.
    pub fn reload(&mut self, changed: &[String]) -> bool {
        if changed.iter().any(|p| change(p) == Change::Restart) {
            return true;
        }
        let mut errors = Vec::new();
        let mut story = false;
        {
            let mut st = self.state.borrow_mut();
            let st = &mut *st;
            for path in changed {
                match change(path) {
                    Change::Shader => {
                        let Some(&id) = st.shaders.get(path) else { continue };
                        let result =
                            st.assets.read_string(path).and_then(|s| st.renderer.reload_shader(id, path, &s));
                        if let Err(e) = result {
                            errors.push(e);
                        }
                    }
                    Change::Image => {
                        let Some(info) = st.images.get_mut(path) else { continue };
                        let decoded = st
                            .assets
                            .read(path)
                            .and_then(|b| Image::decode(&b).map_err(|e| format!("{path}: {e}")));
                        match decoded {
                            Ok(image) => {
                                st.renderer.replace_texture(info.id, &image);
                                info.size = UVec2::new(image.width, image.height);
                                st.alpha_masks.forget(info.id);
                            }
                            Err(e) => errors.push(e),
                        }
                    }
                    Change::Sound => {
                        st.sounds.remove(path);
                        // A looping track restarts with the new sound.
                        let channels: Vec<String> =
                            st.tracks.iter().filter(|(_, f)| *f == path).map(|(c, _)| c.clone()).collect();
                        for c in channels {
                            let opts = PlayOptions { looping: true, crossfade: 0.3, ..Default::default() };
                            if let Err(e) = st.play(&c, path, opts) {
                                errors.push(e);
                            }
                        }
                    }
                    Change::Story => story = true,
                    Change::Restart => unreachable!(),
                }
            }
        }
        if story && let Err(e) = self.reload_story() {
            errors.push(e);
        }
        if errors.is_empty() {
            // A successful reload clears what the last attempt reported.
            self.state.borrow_mut().errors.hide();
        }
        for e in errors {
            report(&self.state, &e);
        }
        log::info!("reloaded {}", changed.join(", "));
        false
    }

    /// Recompiles the story and continues at the same line of the new version (moved lines
    /// are found by their text, as with saves from older versions).
    fn reload_story(&mut self) -> Result<(), String> {
        let files = with_story(&self.state, false, |s| Ok(s.loaded.then(|| s.files.clone())))?;
        let Some(files) = files else { return Ok(()) };
        let program = compile_story(&self.state.borrow(), files)?;
        let resume = with_story(&self.state, false, |s| {
            let at = match s.waiting {
                Waiting::Nothing | Waiting::Ended => None,
                _ => s.vm.save().ok(),
            };
            s.host.prepare(&program).map_err(|e| e.to_string())?;
            s.vm = Vm::new(Arc::new(program));
            let Some(at) = at else {
                s.waiting = Waiting::Nothing;
                return Ok(false);
            };
            let warnings = s.vm.load(&mut s.host, &at).map_err(|e| e.to_string())?;
            warnings.iter().for_each(|w| log::warn!("{w}"));
            Ok(true)
        })?;
        if resume {
            story_restored(&self.lua, &self.state)?;
        }
        Ok(())
    }

    /// Before a restart: saves the story (with registered state) to a hidden slot, if the
    /// game set up saves and the story is waiting for the player. Returns whether it did.
    pub fn save_for_restart(&mut self) -> bool {
        let story = with_story(&self.state, false, |s| match s.waiting {
            Waiting::Nothing | Waiting::Ended => Ok(None),
            _ => Ok(s.vm.save().ok()),
        });
        let Ok(Some(story)) = story else { return false };
        let st = self.state.borrow();
        if st.storage.is_none() {
            return false;
        }
        let body = Value::map().with("story", story).with("data", Value::Nil);
        match write_save(&st, RELOAD_SLOT, "Live reload".into(), body, None) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("saving for the reload: {e}");
                false
            }
        }
    }

    /// After a restart: tells the game where the story was. `reload` handlers get the slot to
    /// load (or nil when there was no story to keep) and usually do what their load menu does.
    pub fn restarted(&mut self, saved: bool) {
        emit(&self.state, "reload", saved.then_some(RELOAD_SLOT));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_changes() {
        assert_eq!(change("shaders/fade.wgsl"), Change::Shader);
        assert_eq!(change("images/bg/Room.PNG"), Change::Image);
        assert_eq!(change("audio/theme.ogg"), Change::Sound);
        assert_eq!(change("story/day1.kulisa"), Change::Story);
        assert_eq!(change("main.luau"), Change::Restart);
        assert_eq!(change("kulisa.toml"), Change::Restart);
        assert_eq!(change("fonts/Inter.ttf"), Change::Restart);
    }
}
