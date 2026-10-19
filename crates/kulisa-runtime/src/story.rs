//! Drives the smoke scene from a `.kulisa` script: runs the VM, shows lines and menus,
//! applies stage and audio events, and handles rollback and quick save/load.
//!
//! The engine draws nothing on its own; until Luau handlers (M4) and the classic-vn
//! template (M5) exist, this module plays that part for the smoke scene.

use std::collections::BTreeMap;
use std::sync::Arc;

use kulisa_core::{Color, SaveFile, SaveHeader};
use kulisa_lua::{Scripting, StoryHost};
use kulisa_platform::Storage;
use kulisa_render::Renderer;
use kulisa_story::{Event, Line, MenuItem, Stage, Vm};
use kulisa_text::TextSystem;

use crate::smoke::Smoke;

const SCRIPT: &str = include_str!("scripts/demo.kulisa");
const GAME_ID: &str = "kulisa_smoke";
/// Version of this game's save data (see `Migrations`).
const SAVE_VERSION: u32 = 1;

/// Sound requests for the runtime's audio system.
pub enum AudioCmd {
    Play { channel: String, file: String, fade: f32, looping: bool },
    Stop { channel: String, fade: f32 },
}

/// What the player is looking at.
enum Showing {
    Nothing,
    Line(Line),
    Menu {
        caption: Option<Line>,
        items: Vec<MenuItem>,
    },
    /// A timed pause; seconds left (or waiting for input when `None`).
    Pause(Option<f32>),
    Ended,
    /// A script error, shown in the dialogue box.
    Error(String),
}

pub struct StoryPlayer {
    vm: Vm,
    host: StoryHost,
    showing: Showing,
    pub audio: Vec<AudioCmd>,
    storage: Option<Storage>,
    playtime: f64,
}

fn speaker(line: &Line) -> Option<(String, Color)> {
    let who = line.who.as_ref()?;
    let name = who.data.get("name").as_str().unwrap_or(&who.id).to_string();
    let color = who.data.get("color").as_str().and_then(Color::hex).unwrap_or(Color::WHITE);
    Some((name, color))
}

impl StoryPlayer {
    /// Compiles the demo script and starts it. Errors (with file:line:col) are returned for
    /// display.
    pub fn new(scripting: &Scripting) -> Result<StoryPlayer, String> {
        let program = kulisa_story::compile(&[("demo.kulisa", SCRIPT)]).map_err(|e| e.to_string())?;
        let mut host = scripting.story_host().map_err(|e| e.to_string())?;
        host.prepare(&program).map_err(|e| e.to_string())?;
        let mut vm = Vm::new(Arc::new(program));
        vm.start(&mut host, "start").map_err(|e| e.to_string())?;
        let storage = Storage::new(GAME_ID, "saves").map_err(|e| log::warn!("saves disabled: {e}")).ok();
        Ok(StoryPlayer { vm, host, showing: Showing::Nothing, audio: Vec::new(), storage, playtime: 0.0 })
    }

    /// A player that only shows an error (the script failed to compile).
    pub fn failed(scripting: &Scripting, error: String) -> StoryPlayer {
        let program = Arc::new(kulisa_story::Program::default());
        let host = scripting.story_host().expect("a story host on a working Luau VM");
        StoryPlayer {
            vm: Vm::new(program),
            host,
            showing: Showing::Error(error),
            audio: Vec::new(),
            storage: None,
            playtime: 0.0,
        }
    }

    /// Runs the VM to the next thing the player must see.
    pub fn run(&mut self, smoke: &mut Smoke, r: &mut Renderer, text: &mut TextSystem) {
        if matches!(self.showing, Showing::Error(_) | Showing::Ended) {
            return;
        }
        loop {
            match self.vm.next(&mut self.host) {
                Ok(Event::Say(line)) => {
                    self.showing = Showing::Line(line);
                    break;
                }
                Ok(Event::Menu { caption, items }) => {
                    self.showing = Showing::Menu { caption, items };
                    break;
                }
                Ok(Event::Pause(secs)) => {
                    self.showing = Showing::Pause(secs);
                    break;
                }
                Ok(Event::End) => {
                    self.showing = Showing::Ended;
                    break;
                }
                Ok(Event::Scene { image, .. }) => smoke.set_scene(&image),
                Ok(Event::Show { image, at, .. }) => smoke.show(&image, at.as_deref()),
                Ok(Event::Hide { tag, .. }) => smoke.hide(&tag),
                Ok(Event::With(_)) => {} // transitions arrive with the classic-vn template (M5)
                Ok(Event::Play { channel, file, fade, looping }) => {
                    self.audio.push(AudioCmd::Play { channel, file, fade: fade.unwrap_or(0.0), looping })
                }
                Ok(Event::Stop { channel, fade }) => {
                    self.audio.push(AudioCmd::Stop { channel, fade: fade.unwrap_or(0.0) })
                }
                Err(e) => {
                    log::error!("{e}");
                    self.showing = Showing::Error(e.to_string());
                    break;
                }
            }
        }
        self.display(smoke, r, text);
    }

    /// Called when a (new) scene is ready: starts the story, or redraws where it was
    /// (e.g. after Android recreated the renderer).
    pub fn resume(
        &mut self,
        smoke: &mut Smoke,
        r: &mut Renderer,
        text: &mut TextSystem,
        playing: &BTreeMap<String, String>,
    ) {
        if matches!(self.showing, Showing::Nothing) {
            self.run(smoke, r, text);
        } else {
            self.restore_stage(smoke, playing);
            self.display(smoke, r, text);
        }
    }

    /// Shows the current line or menu (also after a renderer or display change).
    pub fn display(&self, smoke: &mut Smoke, r: &mut Renderer, text: &mut TextSystem) {
        smoke.clear_menu();
        match &self.showing {
            Showing::Nothing => {}
            Showing::Line(line) => smoke.set_line(r, text, speaker(line), &line.text),
            Showing::Menu { caption, items } => {
                match caption {
                    Some(c) => smoke.set_line(r, text, speaker(c), &c.text),
                    None => smoke.set_line(r, text, None, ""),
                }
                let items: Vec<(usize, String)> = items.iter().map(|i| (i.index, i.text.clone())).collect();
                smoke.set_menu(r, text, &items);
            }
            Showing::Pause(_) => smoke.set_line(r, text, None, ""),
            Showing::Ended => smoke.set_line(r, text, None, "{i}The end.{/i}"),
            Showing::Error(e) => smoke.set_error(r, text, e),
        }
    }

    /// Rebuilds visuals and music from the VM's stage (after rollback, load or a new renderer).
    pub fn restore_stage(&mut self, smoke: &mut Smoke, playing: &BTreeMap<String, String>) {
        let stage: Stage = self.vm.stage().clone();
        smoke.reset_stage();
        if let Some(scene) = &stage.scene {
            smoke.set_scene(scene);
        }
        for s in &stage.shown {
            smoke.show(&s.image, s.at.as_deref());
        }
        // Keep music that is already right; switch or stop the rest.
        for (channel, track) in &stage.tracks {
            if playing.get(channel) != Some(&track.file) {
                let cmd = AudioCmd::Play {
                    channel: channel.clone(),
                    file: track.file.clone(),
                    fade: 1.0,
                    looping: true,
                };
                self.audio.push(cmd);
            }
        }
        for channel in playing.keys().filter(|c| !stage.tracks.contains_key(*c)) {
            self.audio.push(AudioCmd::Stop { channel: channel.clone(), fade: 1.0 });
        }
    }

    /// Click, tap, space: finish the typewriter, or continue.
    pub fn advance(&mut self, smoke: &mut Smoke, r: &mut Renderer, text: &mut TextSystem) {
        match self.showing {
            Showing::Line(_) if !smoke.line_complete() => smoke.skip_line(),
            Showing::Line(_) | Showing::Pause(None) => self.run(smoke, r, text),
            _ => {}
        }
    }

    pub fn choose(&mut self, smoke: &mut Smoke, r: &mut Renderer, text: &mut TextSystem, index: usize) {
        if !matches!(self.showing, Showing::Menu { .. }) {
            return;
        }
        match self.vm.choose(&mut self.host, index) {
            Ok(()) => self.run(smoke, r, text),
            Err(e) => log::warn!("{e}"),
        }
    }

    /// The visible menu item at position `n` (keys 1-9).
    pub fn menu_item(&self, n: usize) -> Option<usize> {
        match &self.showing {
            Showing::Menu { items, .. } => items.get(n).map(|i| i.index),
            _ => None,
        }
    }

    fn after_restore(
        &mut self,
        smoke: &mut Smoke,
        r: &mut Renderer,
        text: &mut TextSystem,
        playing: &BTreeMap<String, String>,
    ) {
        self.restore_stage(smoke, playing);
        self.showing = Showing::Nothing;
        self.run(smoke, r, text);
        smoke.skip_line(); // the player already read it
    }

    pub fn rollback(
        &mut self,
        smoke: &mut Smoke,
        r: &mut Renderer,
        text: &mut TextSystem,
        playing: &BTreeMap<String, String>,
    ) {
        match self.vm.rollback(&mut self.host) {
            Ok(true) => self.after_restore(smoke, r, text, playing),
            Ok(false) => {}
            Err(e) => log::error!("{e}"),
        }
    }

    pub fn roll_forward(
        &mut self,
        smoke: &mut Smoke,
        r: &mut Renderer,
        text: &mut TextSystem,
        playing: &BTreeMap<String, String>,
    ) {
        match self.vm.roll_forward(&mut self.host) {
            Ok(true) => self.after_restore(smoke, r, text, playing),
            Ok(false) => self.advance(smoke, r, text),
            Err(e) => log::error!("{e}"),
        }
    }

    /// Counts down timed pauses and playtime.
    pub fn tick(&mut self, dt: f32, smoke: &mut Smoke, r: &mut Renderer, text: &mut TextSystem) {
        self.playtime += dt as f64;
        if let Showing::Pause(Some(left)) = &mut self.showing {
            *left -= dt;
            if *left <= 0.0 {
                self.run(smoke, r, text);
            }
        }
    }

    pub fn quicksave(&mut self) {
        let Some(storage) = &self.storage else { return };
        let title = match &self.showing {
            Showing::Line(l) => l.text.chars().take(80).collect(),
            _ => String::new(),
        };
        let result = self.vm.save().map_err(|e| e.to_string()).and_then(|body| {
            let header = SaveHeader {
                game_id: GAME_ID.into(),
                game_version: env!("CARGO_PKG_VERSION").into(),
                save_version: SAVE_VERSION,
                timestamp: web_time::SystemTime::now()
                    .duration_since(web_time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
                slot: "quick".into(),
                playtime: self.playtime,
                title,
                // Thumbnails arrive with the save/load screens (M5).
                thumbnail: None,
            };
            storage.write("quick", &SaveFile { header, body }.to_bytes())
        });
        match result {
            Ok(()) => log::info!("quicksaved"),
            Err(e) => log::error!("quicksave failed: {e}"),
        }
    }

    pub fn quickload(
        &mut self,
        smoke: &mut Smoke,
        r: &mut Renderer,
        text: &mut TextSystem,
        playing: &BTreeMap<String, String>,
    ) {
        let Some(storage) = &self.storage else { return };
        let loaded = storage
            .read("quick")
            .and_then(|b| b.ok_or_else(|| "no quicksave yet".to_string()))
            .and_then(|bytes| {
                let mut save = SaveFile::from_bytes(&bytes)?;
                kulisa_core::Migrations::default().apply(&mut save, SAVE_VERSION)?;
                self.playtime = save.header.playtime;
                self.vm.load(&mut self.host, &save.body).map_err(|e| e.to_string())
            });
        match loaded {
            Ok(warnings) => {
                warnings.iter().for_each(|w| log::warn!("{w}"));
                log::info!("quickloaded");
                self.after_restore(smoke, r, text, playing);
            }
            Err(e) => log::error!("quickload failed: {e}"),
        }
    }
}
