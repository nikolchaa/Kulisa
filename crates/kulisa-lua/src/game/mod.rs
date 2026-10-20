//! The scripted game: engine state that Luau drives through the `kulisa` API.
//!
//! The runtime owns the window and event loop and forwards frames and input here. Rust
//! never holds the state borrow while calling into Luau, so scripts (including story `$`
//! lines) can always call back into the API.

mod api;
mod display;
mod hit;
mod interact;
mod media;
mod mesh;
mod pad;
mod pointer;
mod sensors;
mod typing;
mod ui;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use kulisa_audio::{Audio, PlayOptions, Sound};
use kulisa_core::{NodeId, NodeKind, ScaleMode, Scene, ShaderId, TextureId, Tween, UVec2, Vec2, Viewport};
use kulisa_platform::{Assets, Storage};
use kulisa_render::{Image, Renderer, SurfaceTarget};
use kulisa_story::{Event, Program, Stage, Vm};
use kulisa_text::{TextBox, TextStyle, TextSystem};
use kulisa_ui::Gesture;
use mlua::{Function, IntoLuaMulti, Lua, Table, Value as LuaValue};

use crate::story::{StoryHost, from_value};
pub use display::{Device, FormFactor, Orientation, Requests};

const PRELUDE: &str = include_str!("prelude.luau");

pub(crate) type Shared = Rc<RefCell<State>>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImageInfo {
    pub id: TextureId,
    pub size: UVec2,
}

pub(crate) struct TextNode {
    pub markup: String,
    pub style: TextStyle,
    pub text: TextBox,
}

/// What the story is waiting for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Waiting {
    Nothing,
    Line,
    Menu,
    Pause,
    Ended,
}

pub(crate) struct StoryState {
    pub vm: Vm,
    pub host: StoryHost,
    pub loaded: bool,
    pub waiting: Waiting,
    /// Seconds left on a timed `pause`.
    pub timer: Option<f32>,
    /// Last line shown, for save titles.
    pub last_line: String,
}

pub(crate) struct State {
    pub assets: Assets,
    pub renderer: Renderer,
    pub scene: Scene,
    pub viewport: Viewport,
    pub text: TextSystem,
    pub texts: HashMap<NodeId, TextNode>,
    pub images: HashMap<String, ImageInfo>,
    pub white: ImageInfo,
    pub shaders: HashMap<String, ShaderId>,
    pub tweens: Vec<(u64, Tween)>,
    pub next_id: u64,
    pub audio: Option<Audio>,
    pub sounds: HashMap<String, Sound>,
    /// Looping track per channel, as requested (played once audio is available).
    pub tracks: BTreeMap<String, String>,
    /// Volume per channel ("master" for the mix), as requested.
    pub volumes: BTreeMap<String, f32>,
    /// Taken out while the VM runs (see the module docs).
    pub story: Option<StoryState>,
    pub handlers: HashMap<String, Vec<(u64, Function)>>,
    pub pointer: Option<Vec2>,
    pub keys: HashSet<String>,
    pub time: f64,
    /// Physical pixels per design unit the text was last laid out for.
    pub text_scale: f32,
    pub game_id: String,
    pub storage: Option<Storage>,
    pub save_version: u32,
    pub migrations: Vec<(u32, Function)>,
    pub playtime: f64,
    pub reporting: bool,
    pub media: media::Media,
    /// Persistent data (outside saves and rollback); set up with the save storage.
    pub persist: Option<Storage>,
    /// Lines and menu choices the player has seen, across sessions ([`Vm::line_key`]).
    pub seen: HashSet<u64>,
    pub seen_dirty: bool,
    pub seen_flushed: f64,
    /// Whether the current line had been seen before it was shown.
    pub line_seen: bool,
    pub ui: ui::UiState,
    pub display: display::DisplayState,
    pub alpha_masks: hit::Masks,
    pub interact: interact::Interact,
    /// Every pointer: the mouse (id 0) and the touches that are down (ids from 1).
    pub pointers: BTreeMap<u32, pointer::Pointer>,
    pub pinch: Option<pointer::Pinch>,
    pub pads: pad::Gamepads,
    pub typing: typing::Typing,
    pub sensors: sensors::Sensors,
}

/// The seen-set key of choice `index` at the menu `line`.
pub(crate) fn choice_key(line: u64, index: usize) -> u64 {
    line ^ (index as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

impl State {
    /// Writes the seen set to persistent storage.
    pub fn flush_seen(&mut self) {
        self.seen_flushed = self.time;
        let Some(p) = &self.persist else { return };
        if !self.seen_dirty {
            return;
        }
        let mut keys: Vec<u64> = self.seen.iter().copied().collect();
        keys.sort_unstable();
        let bytes: Vec<u8> = keys.iter().flat_map(|k| k.to_le_bytes()).collect();
        match p.write("seen", &bytes) {
            Ok(()) => self.seen_dirty = false,
            Err(e) => log::warn!("saving seen lines: {e}"),
        }
    }

    /// Loads the seen set (after `kulisa.save.setup`).
    pub fn load_seen(&mut self) {
        self.seen.clear();
        let Some(p) = &self.persist else { return };
        if let Ok(Some(b)) = p.read("seen") {
            self.seen.extend(b.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c)));
        }
    }

    pub fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn scale(&self) -> f32 {
        self.viewport.layout(self.renderer.size()).scale
    }

    /// Loads (or reuses) a sound; music streams where the platform can.
    fn sound(&mut self, path: &str, stream: bool) -> Result<Sound, String> {
        if let Some(s) = self.sounds.get(path) {
            return Ok(s.clone());
        }
        let bytes = self.assets.read(path)?;
        let sound = if stream { Sound::stream(bytes) } else { Sound::decode(bytes) }
            .map_err(|e| format!("{path}: {e}"))?;
        self.sounds.insert(path.to_string(), sound.clone());
        Ok(sound)
    }

    pub fn play(&mut self, channel: &str, path: &str, opts: PlayOptions) -> Result<(), String> {
        if opts.looping {
            self.tracks.insert(channel.to_string(), path.to_string());
        }
        if self.audio.is_none() {
            return Ok(()); // started with the audio (after the first input on the web)
        }
        let sound = self.sound(path, channel == "music")?;
        self.audio.as_mut().unwrap().play(channel, &sound, opts)
    }

    pub fn stop(&mut self, channel: &str, fade: f32) -> Result<(), String> {
        self.tracks.remove(channel);
        match &mut self.audio {
            Some(a) => a.stop(channel, fade),
            None => Ok(()),
        }
    }
}

/// Collects the handlers for `event`; they are called after the borrow ends.
fn handlers(shared: &Shared, event: &str) -> Vec<Function> {
    shared
        .borrow()
        .handlers
        .get(event)
        .map(|h| h.iter().map(|(_, f)| f.clone()).collect())
        .unwrap_or_default()
}

/// Calls every handler for `event`. Errors are reported, not propagated.
pub(crate) fn emit(shared: &Shared, event: &str, args: impl IntoLuaMulti + Clone) {
    for f in handlers(shared, event) {
        if let Err(e) = f.call::<()>(args.clone()) {
            report(shared, &format!("in `{event}` handler: {e}"));
        }
    }
}

/// Logs a script error and passes it to `error` handlers (never recursively).
pub(crate) fn report(shared: &Shared, message: &str) {
    log::error!("{message}");
    if shared.borrow().reporting {
        return;
    }
    shared.borrow_mut().reporting = true;
    for f in handlers(shared, "error") {
        if let Err(e) = f.call::<()>(message.to_string()) {
            log::error!("in `error` handler: {e}");
        }
    }
    shared.borrow_mut().reporting = false;
}

fn table(lua: &Lua, v: &kulisa_core::Value) -> mlua::Result<Table> {
    match from_value(lua, v)? {
        LuaValue::Table(t) => Ok(t),
        _ => lua.create_table(),
    }
}

fn line_table(lua: &Lua, line: &kulisa_story::Line, seen: bool) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("text", line.text.as_str())?;
    t.set("seen", seen)?;
    if let Some(who) = &line.who {
        let w = table(lua, &who.data)?;
        w.set("id", who.id.as_str())?;
        t.set("who", w)?;
    }
    Ok(t)
}

/// Runs `f` on the story with the state borrow released (story chunks may call the API).
pub(crate) fn with_story<R>(
    shared: &Shared,
    need_loaded: bool,
    f: impl FnOnce(&mut StoryState) -> Result<R, String>,
) -> Result<R, String> {
    let story = shared.borrow_mut().story.take();
    let mut story = story.ok_or_else(|| "the story is busy (called from a story line?)".to_string())?;
    let result = if need_loaded && !story.loaded {
        Err("no story loaded; call kulisa.story.load() first".to_string())
    } else {
        f(&mut story)
    };
    shared.borrow_mut().story = Some(story);
    result
}

/// Runs the story to its next blocking event and passes every event to the handlers.
pub(crate) fn story_advance(lua: &Lua, shared: &Shared) -> Result<(), String> {
    let (events, key) = with_story(shared, true, |s| {
        s.timer = None;
        let mut events = Vec::new();
        loop {
            let ev = s.vm.next(&mut s.host).map_err(|e| e.to_string())?;
            let key = if ev.blocks() { s.vm.line_key() } else { None };
            let blocks = ev.blocks();
            match &ev {
                Event::Say(l) => {
                    s.last_line = l.text.clone();
                    s.waiting = Waiting::Line;
                }
                Event::Menu { .. } => s.waiting = Waiting::Menu,
                Event::Pause(secs) => {
                    s.timer = *secs;
                    s.waiting = Waiting::Pause;
                }
                Event::End => s.waiting = Waiting::Ended,
                _ => {}
            }
            events.push(ev);
            if blocks {
                return Ok((events, key));
            }
        }
    })?;
    // Seen flags are as of before this display; then the line counts as seen.
    let seen = {
        let mut st = shared.borrow_mut();
        let seen = Seen {
            line: key.is_some_and(|k| st.seen.contains(&k)),
            choices: key
                .map(|k| (0..64).map(|i| st.seen.contains(&choice_key(k, i))).collect())
                .unwrap_or_default(),
        };
        if let Some(k) = key
            && matches!(events.last(), Some(Event::Say(_) | Event::Menu { .. }))
        {
            st.line_seen = seen.line;
            st.seen_dirty |= st.seen.insert(k);
        }
        seen
    };
    dispatch(lua, shared, events, &seen).map_err(|e| e.to_string())
}

/// What the player saw before, for the blocking event that ends a batch.
struct Seen {
    line: bool,
    /// Per menu choice index (the first 64).
    choices: Vec<bool>,
}

fn own_with(e: &Event) -> bool {
    matches!(
        e,
        Event::Scene { with: Some(_), .. }
            | Event::Show { with: Some(_), .. }
            | Event::Hide { with: Some(_), .. }
    )
}

/// Whether a `with` statement follows before the next statement with its own `with`.
fn with_follows(rest: &[Event]) -> bool {
    rest.iter().take_while(|e| !own_with(e)).any(|e| matches!(e, Event::With(_)))
}

/// Freezes the screen for transitions, as in Ren'Py: `x with t` transitions just that
/// statement's change, and `with t` everything since the last interaction or transition.
fn story_freeze(shared: &Shared, hide_transition: bool) {
    let result = media::freeze(&mut shared.borrow_mut(), hide_transition);
    if let Err(e) = result {
        log::warn!("freezing the screen: {e}");
    }
}

fn stage_event(lua: &Lua, kind: &str, fields: &[(&str, LuaValue)]) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("kind", kind)?;
    for (k, v) in fields {
        t.set(*k, v)?;
    }
    Ok(t)
}

fn dispatch(lua: &Lua, shared: &Shared, events: Vec<Event>, seen: &Seen) -> mlua::Result<()> {
    use mlua::IntoLua;
    if with_follows(&events) {
        story_freeze(shared, false);
    }
    for i in 0..events.len() {
        let ev = events[i].clone();
        let transition = own_with(&ev) || matches!(ev, Event::With(_));
        if own_with(&ev) {
            story_freeze(shared, false);
        }
        match ev {
            Event::Say(line) => emit(shared, "say", line_table(lua, &line, seen.line)?),
            Event::Menu { caption, items } => {
                let caption = caption.map(|c| line_table(lua, &c, seen.line)).transpose()?;
                let list = lua.create_table()?;
                for item in items {
                    let t = lua.create_table()?;
                    t.set("index", item.index + 1)?;
                    t.set("text", item.text)?;
                    t.set("chosen", seen.choices.get(item.index).copied().unwrap_or(false))?;
                    list.push(t)?;
                }
                emit(shared, "menu", (list, caption));
            }
            Event::Pause(secs) => emit(shared, "pause", secs),
            Event::End => emit(shared, "end", ()),
            Event::Scene { image, with } => {
                let fields = [("image", image.into_lua(lua)?), ("with", with.into_lua(lua)?)];
                emit(shared, "stage", stage_event(lua, "scene", &fields)?)
            }
            Event::Show { image, at, with } => {
                let fields = [
                    ("image", image.into_lua(lua)?),
                    ("at", at.into_lua(lua)?),
                    ("with", with.into_lua(lua)?),
                ];
                emit(shared, "stage", stage_event(lua, "show", &fields)?)
            }
            Event::Hide { tag, with } => {
                let fields = [("tag", tag.into_lua(lua)?), ("with", with.into_lua(lua)?)];
                emit(shared, "stage", stage_event(lua, "hide", &fields)?)
            }
            Event::With(transition) => {
                emit(shared, "stage", stage_event(lua, "with", &[("with", transition.into_lua(lua)?)])?)
            }
            Event::Play { channel, file, fade, looping } => {
                let fade = fade.unwrap_or(0.0);
                let opts = PlayOptions { looping, fade_in: fade, crossfade: fade, ..Default::default() };
                let result = shared.borrow_mut().play(&channel, &file, opts);
                if let Err(e) = result {
                    report(shared, &e);
                }
            }
            Event::Stop { channel, fade } => {
                let result = shared.borrow_mut().stop(&channel, fade.unwrap_or(0.0));
                if let Err(e) = result {
                    report(shared, &e);
                }
            }
        }
        if transition && with_follows(&events[i + 1..]) {
            story_freeze(shared, true);
        }
    }
    // A freeze nobody used must not leak into a later transition.
    let mut st = shared.borrow_mut();
    st.media.frozen = None;
    // Screens may show story state: rebuild them.
    ui::refresh_all(&mut st);
    Ok(())
}

/// After rollback or load: re-sync music, let the game rebuild its stage (`restore`), then
/// continue to the restored line.
pub(crate) fn story_restored(lua: &Lua, shared: &Shared) -> Result<(), String> {
    let stage: Stage = with_story(shared, true, |s| Ok(s.vm.stage().clone()))?;
    {
        let mut st = shared.borrow_mut();
        let playing: Vec<String> = st.tracks.keys().cloned().collect();
        for channel in playing.iter().filter(|c| !stage.tracks.contains_key(*c)) {
            st.stop(channel, 1.0)?;
        }
        for (channel, track) in &stage.tracks {
            if st.tracks.get(channel) != Some(&track.file) {
                let opts = PlayOptions { looping: true, fade_in: 1.0, crossfade: 1.0, ..Default::default() };
                st.play(channel, &track.file, opts)?;
            }
        }
    }
    emit(shared, "restore", table(lua, &stage.to_value()).map_err(|e| e.to_string())?);
    story_advance(lua, shared)
}

/// A running game.
pub struct Game {
    lua: Lua,
    state: Shared,
}

impl Game {
    /// Starts the game in `assets` (runs its `main.luau`) on `renderer`, as on a desktop.
    pub fn new(assets: Assets, renderer: Renderer) -> Result<Game, String> {
        Self::with_device(assets, renderer, Device { can_quit: true, ..Default::default() })
    }

    /// Starts the game for `device` (its form factor picks the display profile).
    pub fn with_device(assets: Assets, mut renderer: Renderer, device: Device) -> Result<Game, String> {
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        crate::web::init();

        let white =
            ImageInfo { id: renderer.create_texture(&Image::solid(4, 4, [255; 4])), size: UVec2::splat(4) };
        let viewport = Viewport::new(1920.0, 1080.0, ScaleMode::Expand);
        let mut state = State {
            assets,
            renderer,
            scene: Scene::new(viewport.design),
            viewport,
            text: TextSystem::new(),
            texts: HashMap::new(),
            images: HashMap::new(),
            white,
            shaders: HashMap::new(),
            tweens: Vec::new(),
            next_id: 0,
            audio: None,
            sounds: HashMap::new(),
            tracks: BTreeMap::new(),
            volumes: BTreeMap::new(),
            story: None,
            handlers: HashMap::new(),
            pointer: None,
            keys: HashSet::new(),
            time: 0.0,
            text_scale: 0.0,
            game_id: String::new(),
            storage: None,
            save_version: 1,
            migrations: Vec::new(),
            playtime: 0.0,
            reporting: false,
            media: Default::default(),
            persist: None,
            seen: HashSet::new(),
            seen_dirty: false,
            seen_flushed: 0.0,
            line_seen: false,
            ui: Default::default(),
            display: Default::default(),
            alpha_masks: Default::default(),
            interact: Default::default(),
            pointers: BTreeMap::new(),
            pinch: None,
            pads: Default::default(),
            typing: Default::default(),
            sensors: Default::default(),
        };
        state.display.device = device;
        let shared = Rc::new(RefCell::new(state));
        let lua = Lua::new();
        lua.set_app_data(shared.clone());
        let err = |e: mlua::Error| e.to_string();
        api::install(&lua).map_err(err)?;
        lua.load(PRELUDE).set_name("@kulisa/prelude.luau").exec().map_err(err)?;
        lua.sandbox(true).map_err(err)?;
        // After the sandbox, so stories see the game's globals.
        let host = StoryHost::new(&lua).map_err(err)?;
        shared.borrow_mut().story = Some(StoryState {
            vm: Vm::new(Arc::new(Program::default())),
            host,
            loaded: false,
            waiting: Waiting::Nothing,
            timer: None,
            last_line: String::new(),
        });

        let mut game = Game { lua, state: shared };
        if !cfg!(target_arch = "wasm32") {
            game.start_audio();
        }
        let main = game.state.borrow().assets.read_string("main.luau")?;
        let main = game.lua.load(&main).set_name("@main.luau").into_function().map_err(err)?;
        let spawn: Function = game.kulisa().and_then(|k| k.get("spawn")).map_err(err)?;
        let (ok, error): (bool, Option<String>) = spawn.call(main).map_err(err)?;
        if !ok {
            return Err(error.unwrap_or_else(|| "main.luau failed".into()));
        }
        Ok(game)
    }

    fn kulisa(&self) -> mlua::Result<Table> {
        self.lua.globals().get("kulisa")
    }

    /// Runs Luau source in the game's VM (tools and tests).
    pub fn eval<R: mlua::FromLuaMulti>(&self, name: &str, source: &str) -> Result<R, String> {
        self.lua.load(source).set_name(format!("@{name}")).eval().map_err(|e| e.to_string())
    }

    /// Advances time, tweens, script tasks, the story's timed pauses and text animation.
    pub fn frame(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.25);
        {
            let mut st = self.state.borrow_mut();
            let st = &mut *st;
            st.time += dt as f64;
            st.playtime += dt as f64;
            let scene = &mut st.scene;
            st.tweens.retain_mut(|(_, t)| !t.step(scene, dt));
            media::step(st, dt);
            if st.seen_dirty && st.time - st.seen_flushed > 5.0 {
                st.flush_seen();
            }
        }
        api::finish_pending_saves(&self.state);
        if let Err(e) = self.kulisa().and_then(|k| k.get::<Function>("_tick")).and_then(|f| f.call::<()>(dt))
        {
            report(&self.state, &e.to_string());
        }
        emit(&self.state, "update", dt);

        let due = {
            let mut st = self.state.borrow_mut();
            match st.story.as_mut().and_then(|s| s.timer.as_mut()) {
                Some(t) => {
                    *t -= dt;
                    *t <= 0.0
                }
                None => false,
            }
        };
        if due && let Err(e) = story_advance(&self.lua, &self.state) {
            report(&self.state, &e);
        }
        let long_press = {
            let mut st = self.state.borrow_mut();
            let t = st.time;
            st.ui.gestures.tick(t)
        };
        if let Some(Gesture::LongPress(p)) = long_press {
            emit(&self.state, "long_press", (p.x, p.y));
        }
        let mouse = self.state.borrow().pointers.get(&0).and_then(|p| p.pos);
        interact::hover(&self.lua, &self.state, mouse);
        self.nav_frame(dt);
        self.update_pan(dt);
        ui::update(&self.lua, &self.state);
        self.report_display();

        let mut st = self.state.borrow_mut();
        let st = &mut *st;
        let scale = st.scale();
        if (st.text_scale - scale).abs() > scale * 0.01 {
            st.text_scale = scale;
            let ids: Vec<NodeId> = st.texts.keys().copied().collect();
            for id in ids {
                if let Err(e) = api::layout_text(st, id, false) {
                    log::warn!("{e}");
                }
            }
        }
        st.texts.retain(|id, _| st.scene.contains(*id));
        let time = st.time as f32;
        let mut quads = Vec::new();
        for (id, node) in &mut st.texts {
            node.text.advance(dt);
            node.text.quads(time, &mut quads);
            if let Some(n) = st.scene.get_mut(*id)
                && let NodeKind::Quads(q) = &mut n.kind
            {
                std::mem::swap(q, &mut quads);
            }
        }
    }

    pub fn render(&mut self) {
        let mut st = self.state.borrow_mut();
        let st = &mut *st;
        st.renderer.render(&st.scene, &st.viewport, st.time as f32);
    }

    /// The renderer (stats, screenshots, tests).
    pub fn renderer(&self) -> std::cell::RefMut<'_, Renderer> {
        std::cell::RefMut::map(self.state.borrow_mut(), |st| &mut st.renderer)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.state.borrow_mut().renderer.resize(width, height);
        emit(&self.state, "resize", (width, height));
        self.report_display();
    }

    /// Emits `display` when the profile, orientation or rotate hint changed.
    fn report_display(&mut self) {
        let now = {
            let st = self.state.borrow();
            (st.display.device.form, display::orientation(&st), display::mismatch(&st))
        };
        let changed = self.state.borrow().display.reported != Some(now);
        if !changed {
            return;
        }
        self.state.borrow_mut().display.reported = Some(now);
        let t = self.lua.create_table().and_then(|t| {
            t.set("profile", now.0.name())?;
            t.set("orientation", now.1)?;
            t.set("mismatch", now.2)?;
            Ok(t)
        });
        if let Ok(t) = t {
            emit(&self.state, "display", t);
        }
    }

    /// The window's insets in pixels (left, top, right, bottom): notches, system bars,
    /// rounded corners. Screens with `area = "safe"` stay inside.
    pub fn set_safe_area(&mut self, insets: [f32; 4]) {
        self.state.borrow_mut().ui.safe_px = insets.map(|v| v.max(0.0));
    }

    /// What scripts asked the runtime to do since the last call.
    pub fn take_requests(&mut self) -> Requests {
        let mut st = self.state.borrow_mut();
        display::update_cursor(&mut st);
        std::mem::take(&mut st.display.requests)
    }

    /// Tells scripts whether the window is fullscreen now.
    pub fn set_fullscreen(&mut self, on: bool) {
        self.state.borrow_mut().display.fullscreen = on;
    }

    pub fn suspend(&mut self) {
        let mut st = self.state.borrow_mut();
        st.flush_seen();
        st.renderer.suspend();
    }

    pub fn resume(&mut self, target: impl SurfaceTarget, width: u32, height: u32) -> Result<(), String> {
        self.state.borrow_mut().renderer.resume(target, width, height)
    }

    /// Opens the audio device and starts what the game already asked for. Browsers allow
    /// this only after a user gesture, so the runtime calls it on the first input there.
    pub fn start_audio(&mut self) {
        if self.state.borrow().audio.is_some() {
            return;
        }
        let mut audio = match Audio::new() {
            Ok(a) => a,
            Err(e) => return log::warn!("audio unavailable: {e}"),
        };
        let mut st = self.state.borrow_mut();
        for (channel, v) in &st.volumes {
            if channel == "master" {
                audio.set_master_volume(*v, 0.0);
            } else if let Err(e) = audio.set_volume(channel, *v, 0.0) {
                log::warn!("{e}");
            }
        }
        st.audio = Some(audio);
        let tracks: Vec<(String, String)> = st.tracks.iter().map(|(c, f)| (c.clone(), f.clone())).collect();
        for (channel, file) in tracks {
            let opts = PlayOptions { looping: true, fade_in: 1.0, ..Default::default() };
            if let Err(e) = st.play(&channel, &file, opts) {
                log::warn!("{e}");
            }
        }
    }

    fn to_design(&self, window: Vec2) -> Vec2 {
        let st = self.state.borrow();
        st.viewport.layout(st.renderer.size()).to_pixels.inverse().transform_point2(window)
    }

    /// Mouse position in window pixels (`None` when it left the window).
    pub fn pointer_moved(&mut self, window: Option<Vec2>) {
        self.pointer_moved_id(0, window);
    }

    /// Pointer `id` (0 is the mouse, touches count from 1) moved. The first touch down (or the
    /// mouse when none is) is the primary pointer: it drives hover, UI and the gestures.
    pub fn pointer_moved_id(&mut self, id: u32, window: Option<Vec2>) {
        let p = window.map(|w| self.to_design(w));
        let shifted;
        let events = {
            let mut st = self.state.borrow_mut();
            let ptr = st.pointers.entry(id).or_insert_with(|| pointer::Pointer::new(id != 0));
            shifted = p.is_some_and(|p| ptr.pos.is_none_or(|q| (q - p).length() > 0.5));
            ptr.pos = p;
            if is_primary(&st, id) {
                st.pointer = p;
                match p {
                    Some(p) => {
                        st.ui.gestures.moved(p);
                        st.ui.ui.pointer_move(p)
                    }
                    None => {
                        st.ui.ui.pointer_left();
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            }
        };
        ui::run_events(&self.lua, &self.state, events);
        if shifted {
            // A real mouse or touch move: the keyboard focus gives way to it.
            self.pointer_took_over();
        }
        match p {
            Some(p) => {
                interact::moved(&self.lua, &self.state, id, p);
                if id == 0 {
                    interact::hover(&self.lua, &self.state, Some(p));
                }
                emit(&self.state, "move", (p.x, p.y, id));
                self.update_pinch(true);
            }
            None if id == 0 => interact::hover(&self.lua, &self.state, None),
            None => {}
        }
    }

    /// A button of the mouse went down: "left", "right" or "middle".
    pub fn pressed(&mut self, button: &str) {
        self.pressed_id(0, button);
    }

    /// Pointer `id` went down: a mouse button, or "touch". Screens get the left button and
    /// touches first, then draggables and hotspots; what they take does not reach `press`.
    pub fn pressed_id(&mut self, id: u32, button: &str) {
        let left = matches!(button, "left" | "touch");
        if left {
            self.pointer_took_over();
        }
        let (p, primary) = {
            let mut st = self.state.borrow_mut();
            let ptr = st.pointers.entry(id).or_insert_with(|| pointer::Pointer::new(id != 0));
            let p = ptr.pos.unwrap_or_default();
            if left {
                ptr.down = true;
                ptr.captured = false;
            }
            let primary = is_primary(&st, id);
            // A second finger turns a tap or long press into a multi-touch gesture.
            if id != 0 && !primary {
                st.ui.gestures.cancel();
            }
            (p, primary)
        };
        if left {
            let mut taken = false;
            if primary {
                let (by_ui, events) = {
                    let mut st = self.state.borrow_mut();
                    let (taken, events) = st.ui.ui.pointer_down(p);
                    st.ui.pressed_by_ui = taken;
                    (taken, events)
                };
                ui::run_events(&self.lua, &self.state, events);
                taken = by_ui;
            }
            if !taken {
                taken = interact::down(&self.lua, &self.state, id, p);
            }
            let mut st = self.state.borrow_mut();
            if taken {
                if let Some(ptr) = st.pointers.get_mut(&id) {
                    ptr.captured = true;
                }
                if primary {
                    st.ui.gestures.cancel();
                }
            } else if primary {
                let t = st.time;
                st.ui.gestures.down(p, t);
            }
            drop(st);
            if taken {
                self.update_pinch(false);
                return;
            }
        }
        emit(&self.state, "press", (p.x, p.y, button.to_string(), id));
        self.update_pinch(false);
    }

    /// A button of the mouse went up.
    pub fn released(&mut self, button: &str) {
        self.released_id(0, button);
    }

    pub fn released_id(&mut self, id: u32, button: &str) {
        let left = matches!(button, "left" | "touch");
        let (p, primary) = {
            let st = self.state.borrow();
            (st.pointers.get(&id).and_then(|p| p.pos).unwrap_or_default(), is_primary(&st, id))
        };
        let finished = |g: &mut Game| {
            if id != 0 {
                g.state.borrow_mut().pointers.remove(&id);
            } else if let Some(ptr) = g.state.borrow_mut().pointers.get_mut(&0) {
                ptr.down = false;
            }
            g.update_pinch(false);
        };
        if left {
            if let Some(ptr) = self.state.borrow_mut().pointers.get_mut(&id) {
                ptr.down = false;
            }
            if interact::up(&self.lua, &self.state, id, p) {
                self.state.borrow_mut().ui.pressed_by_ui = false;
                return finished(self);
            }
            let (taken, events, gesture) = {
                let mut st = self.state.borrow_mut();
                if primary {
                    let by_ui = std::mem::take(&mut st.ui.pressed_by_ui);
                    let (taken, events) = st.ui.ui.pointer_up(p);
                    let t = st.time;
                    let gesture = if taken || by_ui { None } else { st.ui.gestures.up(p, t) };
                    (taken || by_ui, events, gesture)
                } else {
                    (false, Vec::new(), None)
                }
            };
            ui::run_events(&self.lua, &self.state, events);
            if taken {
                return finished(self);
            }
            emit(&self.state, "release", (p.x, p.y, button.to_string(), id));
            match gesture {
                Some(Gesture::Tap(p)) => emit(&self.state, "tap", (p.x, p.y)),
                Some(Gesture::Swipe(dir, p)) => emit(&self.state, "swipe", (dir.name(), p.x, p.y)),
                _ => {}
            }
            return finished(self);
        }
        emit(&self.state, "release", (p.x, p.y, button.to_string(), id));
    }

    /// Reports pinch and rotate while exactly two uncaptured touches are down.
    fn update_pinch(&mut self, report_steps: bool) {
        let step = {
            let mut st = self.state.borrow_mut();
            let touches: Vec<(u32, Vec2)> = st
                .pointers
                .iter()
                .filter(|(_, p)| p.touch && p.down && !p.captured)
                .filter_map(|(id, p)| p.pos.map(|pos| (*id, pos)))
                .collect();
            let mut pinch = st.pinch.take();
            let step = pointer::Pinch::track(&touches, &mut pinch);
            st.pinch = pinch;
            step
        };
        let Some(s) = step.filter(|_| report_steps) else { return };
        if (s.factor - 1.0).abs() > 1e-6 {
            emit(&self.state, "pinch", (s.factor, s.center.x, s.center.y, s.total));
        }
        if s.delta.abs() > 1e-6 {
            emit(&self.state, "rotate", (s.delta, s.center.x, s.center.y, s.total_angle));
        }
    }

    /// A key by name ("space", "enter", "left", "f5", "a", ...).
    pub fn key(&mut self, name: &str, pressed: bool) {
        {
            let mut st = self.state.borrow_mut();
            if pressed {
                st.keys.insert(name.to_string());
            } else {
                st.keys.remove(name);
            }
        }
        // A text field wants characters as `text`, not as keys.
        if self.wants_text() && typing::is_character_key(name) {
            return;
        }
        // Arrows and Enter drive a navigable UI screen; the game never sees what they take.
        if self.nav_key(name, pressed) {
            return;
        }
        emit(&self.state, if pressed { "key" } else { "key_up" }, name.to_string());
    }

    /// Scroll wheel; positive is up (away from the player).
    pub fn wheel(&mut self, dy: f32) {
        let taken = {
            let mut st = self.state.borrow_mut();
            let p = st.pointer.unwrap_or_default();
            st.ui.ui.wheel(p, dy)
        };
        if !taken {
            emit(&self.state, "wheel", dy);
        }
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        // The story host and handlers hold Luau references; release them before the VM.
        if let Ok(mut st) = self.state.try_borrow_mut() {
            st.flush_seen();
            st.story = None;
            st.handlers.clear();
            st.migrations.clear();
        }
        self.lua.remove_app_data::<Shared>();
    }
}

/// Whether pointer `id` drives the UI and gestures: the lowest touch that is down, or the
/// mouse (and any touch not yet down) when no touch is.
fn is_primary(st: &State, id: u32) -> bool {
    match st.pointers.iter().find(|(_, p)| p.touch && p.down) {
        Some((first, _)) => *first == id,
        None => true,
    }
}
