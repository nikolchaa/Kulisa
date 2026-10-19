//! Gamepads and keyboard/gamepad navigation of UI screens.
//!
//! The runtime feeds buttons, axes and connections (from gilrs on desktop and the web); the game
//! sees `pad_*` events and `kulisa.input.pad_*`. While a navigable UI screen is shown, the
//! d-pad, left stick and arrow keys move its focus, and A / Enter / Space click the focused
//! widget; those inputs then do not reach the game.

use std::collections::{BTreeMap, HashMap, HashSet};

use kulisa_ui::{Direction, Nav};
use mlua::{Lua, Table};

use super::{Game, emit, ui};

/// Seconds a held direction waits before repeating, and between repeats.
const REPEAT_DELAY: f32 = 0.4;
const REPEAT_EVERY: f32 = 0.11;
/// Stick deflection that counts as pressing a direction, and as releasing it again.
const STICK_ON: f32 = 0.6;
const STICK_OFF: f32 = 0.4;
/// Design units per second the right stick scrolls a list at full deflection.
const SCROLL_SPEED: f32 = 1400.0;

pub(crate) const BUTTONS: &[&str] = &[
    "a", "b", "x", "y", "lb", "rb", "lt", "rt", "back", "start", "guide", "ls", "rs", "up", "down", "left",
    "right",
];
pub(crate) const AXES: &[&str] = &["lx", "ly", "rx", "ry", "lt", "rt"];

#[derive(Default)]
struct Pad {
    name: String,
    buttons: HashSet<String>,
    axes: HashMap<String, f32>,
    /// The direction the left stick is held in, as a d-pad press.
    stick: Option<Direction>,
}

/// What keeps a direction repeating while held.
#[derive(Clone, Debug, PartialEq)]
enum Source {
    Key(String),
    Button(u32, String),
    Stick(u32),
}

struct Hold {
    source: Source,
    dir: Direction,
    next: f32,
}

#[derive(Default)]
pub(crate) struct Gamepads {
    pads: BTreeMap<u32, Pad>,
    hold: Option<Hold>,
    /// Inputs navigation took, so their release does not reach the game either.
    taken: HashSet<String>,
    /// The player last used direction keys or a pad: screens that open get a focus.
    pub focus_mode: bool,
}

impl Gamepads {
    pub fn down(&self, button: &str, pad: Option<u32>) -> bool {
        self.pads.iter().any(|(id, p)| pad.is_none_or(|w| w == *id) && p.buttons.contains(button))
    }

    /// The value of an axis on `pad`, or the strongest across pads.
    pub fn axis(&self, axis: &str, pad: Option<u32>) -> f32 {
        self.pads
            .iter()
            .filter(|(id, _)| pad.is_none_or(|w| w == **id))
            .map(|(_, p)| p.axes.get(axis).copied().unwrap_or(0.0))
            .fold(0.0, |a: f32, b| if b.abs() > a.abs() { b } else { a })
    }

    pub fn list(&self) -> Vec<(u32, String)> {
        self.pads.iter().map(|(id, p)| (*id, p.name.clone())).collect()
    }
}

fn dpad(button: &str) -> Option<Direction> {
    Some(match button {
        "up" => Direction::Up,
        "down" => Direction::Down,
        "left" => Direction::Left,
        "right" => Direction::Right,
        _ => return None,
    })
}

fn key_dir(key: &str) -> Option<Direction> {
    dpad(key)
}

impl Game {
    /// A gamepad appeared (`Some(name)`) or went away (`None`).
    pub fn gamepad_connected(&mut self, pad: u32, name: Option<&str>) {
        let connected = name.is_some();
        {
            let mut st = self.state.borrow_mut();
            match name {
                Some(n) => {
                    st.pads.pads.entry(pad).or_default().name = n.to_string();
                }
                None => {
                    st.pads.pads.remove(&pad);
                    let ends = |h: &Hold| match &h.source {
                        Source::Button(p, _) | Source::Stick(p) => *p == pad,
                        Source::Key(_) => false,
                    };
                    if st.pads.hold.as_ref().is_some_and(ends) {
                        st.pads.hold = None;
                    }
                }
            }
        }
        if connected {
            emit(&self.state, "pad_connect", (pad, name.unwrap_or_default().to_string()));
        } else {
            emit(&self.state, "pad_disconnect", pad);
        }
    }

    /// A gamepad button by name ("a", "start", "up", ...; see [`BUTTONS`]).
    pub fn gamepad_button(&mut self, pad: u32, button: &str, pressed: bool) {
        {
            let mut st = self.state.borrow_mut();
            let p = st.pads.pads.entry(pad).or_default();
            // Axes that stand in for the d-pad report every change; only edges count.
            let changed =
                if pressed { p.buttons.insert(button.to_string()) } else { p.buttons.remove(button) };
            if !changed {
                return;
            }
        }
        let id = format!("p{pad}:{button}");
        if !pressed {
            if self.state.borrow_mut().pads.taken.remove(&id) {
                self.release_hold(&Source::Button(pad, button.to_string()));
                return;
            }
            emit(&self.state, "pad_release", (button.to_string(), pad));
            return;
        }
        let nav = if let Some(dir) = dpad(button) {
            self.nav_direction(dir, Source::Button(pad, button.to_string()))
        } else if button == "a" {
            self.nav_activate()
        } else {
            false
        };
        if nav {
            self.state.borrow_mut().pads.taken.insert(id);
        } else {
            emit(&self.state, "pad_press", (button.to_string(), pad));
        }
    }

    /// A gamepad axis by name ("lx", "ly", "rx", "ry", "lt", "rt") with the dead zone applied.
    /// Sticks run from -1 (left, up) to 1 (right, down); triggers from 0 to 1.
    pub fn gamepad_axis(&mut self, pad: u32, axis: &str, value: f32) {
        let (changed, stick) = {
            let mut st = self.state.borrow_mut();
            let p = st.pads.pads.entry(pad).or_default();
            let old = p.axes.insert(axis.to_string(), value).unwrap_or(0.0);
            let stick = if axis == "lx" || axis == "ly" {
                let (x, y) =
                    (p.axes.get("lx").copied().unwrap_or(0.0), p.axes.get("ly").copied().unwrap_or(0.0));
                let big = x.abs().max(y.abs());
                let now = if big >= if p.stick.is_some() { STICK_OFF } else { STICK_ON } {
                    Some(if x.abs() > y.abs() {
                        if x > 0.0 { Direction::Right } else { Direction::Left }
                    } else if y > 0.0 {
                        Direction::Down
                    } else {
                        Direction::Up
                    })
                } else {
                    None
                };
                let changed = now != p.stick;
                p.stick = now;
                changed.then_some(now)
            } else {
                None
            };
            ((old - value).abs() > 0.005, stick)
        };
        if let Some(now) = stick {
            let source = Source::Stick(pad);
            match now {
                Some(dir) => {
                    self.nav_direction(dir, source);
                }
                None => self.release_hold(&source),
            }
        }
        if changed {
            emit(&self.state, "pad_axis", (axis.to_string(), value, pad));
        }
    }

    /// Keyboard navigation: arrows move focus, Enter and Space click it. Returns whether the
    /// key was taken (it then does not reach the game, nor does its release).
    pub(super) fn nav_key(&mut self, name: &str, pressed: bool) -> bool {
        let id = format!("k:{name}");
        if !pressed {
            let taken = self.state.borrow_mut().pads.taken.remove(&id);
            if taken {
                self.release_hold(&Source::Key(name.to_string()));
            }
            return taken;
        }
        let taken = if let Some(dir) = key_dir(name) {
            self.nav_direction(dir, Source::Key(name.to_string()))
        } else if name == "enter" || name == "space" {
            self.nav_activate()
        } else if name == "pageup" || name == "pagedown" {
            let pages = if name == "pageup" { -1.0 } else { 1.0 };
            self.state.borrow_mut().ui.ui.scroll_pages(pages)
        } else {
            false
        };
        if taken {
            self.state.borrow_mut().pads.taken.insert(id);
        }
        taken
    }

    /// Moves focus in `dir` when a screen takes navigation. Returns whether it was taken.
    fn nav_direction(&mut self, dir: Direction, source: Source) -> bool {
        let (nav, active) = {
            let mut st = self.state.borrow_mut();
            // Direction input means the player is on keys or a pad: screens that open later
            // get a focus without another press.
            st.pads.focus_mode = true;
            let active = st.ui.ui.nav_active();
            let nav = if active { Some(st.ui.ui.focus_move(dir)) } else { None };
            if active {
                let t = st.time as f32;
                st.pads.hold = Some(Hold { source, dir, next: t + REPEAT_DELAY });
            }
            (nav, active)
        };
        if let Some(Nav::Adjusted(events)) = nav {
            ui::run_events(&self.state, events);
        }
        active
    }

    /// A / Enter / Space: clicks the focused widget, or focuses the first one.
    fn nav_activate(&mut self) -> bool {
        let events = {
            let mut st = self.state.borrow_mut();
            if !st.ui.ui.nav_active() {
                return false;
            }
            match st.ui.ui.focus_activate() {
                Some(events) => events,
                None => {
                    st.pads.focus_mode = true;
                    st.ui.ui.focus_default();
                    Vec::new()
                }
            }
        };
        ui::run_events(&self.state, events);
        true
    }

    fn release_hold(&mut self, source: &Source) {
        let mut st = self.state.borrow_mut();
        if st.pads.hold.as_ref().is_some_and(|h| &h.source == source) {
            st.pads.hold = None;
        }
    }

    /// Per frame: repeats a held direction, and gives a newly opened navigable screen its
    /// first focus when the player is on keys or a pad.
    pub(super) fn nav_frame(&mut self, dt: f32) {
        let repeat = {
            let mut st = self.state.borrow_mut();
            let now = st.time as f32;
            let still_held = |st: &super::State, s: &Source| match s {
                Source::Key(k) => st.keys.contains(k),
                Source::Button(p, b) => st.pads.down(b, Some(*p)),
                Source::Stick(p) => st.pads.pads.get(p).is_some_and(|x| x.stick.is_some()),
            };
            let drop_hold = match &st.pads.hold {
                Some(h) => !still_held(&st, &h.source),
                None => false,
            };
            if drop_hold {
                st.pads.hold = None;
            }
            match &mut st.pads.hold {
                Some(h) if now >= h.next => {
                    h.next = now + REPEAT_EVERY;
                    Some(h.dir)
                }
                _ => None,
            }
        };
        if let Some(dir) = repeat {
            let nav = self.state.borrow_mut().ui.ui.focus_move(dir);
            if let Nav::Adjusted(events) = nav {
                ui::run_events(&self.state, events);
            }
        }
        let mut st = self.state.borrow_mut();
        if st.pads.focus_mode && st.ui.ui.nav_active() {
            st.ui.ui.focus_default();
        }
        // The right stick scrolls logs and lists.
        let ry = st.pads.axis("ry", None);
        if ry != 0.0 {
            st.ui.ui.scroll_amount(ry * SCROLL_SPEED * dt);
        }
    }

    /// The mouse or a touch is in use again: the keyboard focus goes away.
    pub(super) fn pointer_took_over(&mut self) {
        let mut st = self.state.borrow_mut();
        if st.pads.focus_mode || st.ui.ui.focused().is_some() {
            st.pads.focus_mode = false;
            st.ui.ui.clear_focus();
        }
    }
}

/// `kulisa.input.pad_down`, `pad_axis` and `pads`.
pub(super) fn install(lua: &Lua, input: &Table) -> mlua::Result<()> {
    use super::api::with;
    let check = |name: &str, list: &[&str], what: &str| -> mlua::Result<()> {
        if list.contains(&name) {
            Ok(())
        } else {
            Err(mlua::Error::runtime(format!(
                "unknown gamepad {what} `{name}`; use one of: {}",
                list.join(", ")
            )))
        }
    };
    input.set(
        "pad_down",
        lua.create_function(move |lua, (button, pad): (String, Option<u32>)| {
            check(&button, BUTTONS, "button")?;
            with(lua, |st| Ok(st.pads.down(&button, pad)))
        })?,
    )?;
    input.set(
        "pad_axis",
        lua.create_function(move |lua, (axis, pad): (String, Option<u32>)| {
            check(&axis, AXES, "axis")?;
            with(lua, |st| Ok(st.pads.axis(&axis, pad)))
        })?,
    )?;
    input.set(
        "pads",
        lua.create_function(|lua, ()| {
            let list = with(lua, |st| Ok(st.pads.list()))?;
            let out = lua.create_table()?;
            for (id, name) in list {
                let t = lua.create_table()?;
                t.set("id", id)?;
                t.set("name", name)?;
                out.push(t)?;
            }
            Ok(out)
        })?,
    )?;
    Ok(())
}
