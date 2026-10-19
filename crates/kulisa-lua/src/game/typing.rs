//! Text entry: the platform's committed text and IME composition, delivered to scripts as
//! `text` and `preedit` events while a text field is active.
//!
//! The engine only routes text. Text fields themselves (caret, editing keys, `ui.input`) are
//! written in Luau on top of this, so games can build their own.

use kulisa_core::{Rect, Vec2};
use mlua::{Lua, Table, Value as LuaValue};

use super::{Game, State, emit};

/// Gap kept between the active field and the keyboard, in window pixels.
const PAN_MARGIN: f32 = 24.0;

/// What the keyboard must not cover.
#[derive(Clone, Debug)]
pub(crate) enum Focus {
    /// A UI widget by `id`, wherever its screen is.
    Widget(String),
    /// A rectangle in design units (screen space).
    Rect(Rect),
}

#[derive(Default)]
pub(crate) struct Typing {
    /// A script asked for text (a field has the focus).
    pub active: bool,
    /// What the runtime should do with the platform's IME / soft keyboard, once.
    pub ime_request: Option<bool>,
    /// The field to keep above the keyboard.
    pub focus: Option<Focus>,
    /// Height of the on-screen keyboard in window pixels, as the runtime reports it.
    pub keyboard_px: f32,
    /// How far the view is lifted now, in window pixels.
    pub pan: f32,
}

/// A key name that is also a character: while typing these arrive as `text`, not `key`.
pub(super) fn is_character_key(name: &str) -> bool {
    name == "space" || name.chars().count() == 1
}

impl Game {
    /// Whether a text field is active, so the runtime can show the soft keyboard.
    pub fn wants_text(&self) -> bool {
        self.state.borrow().typing.active
    }

    /// What the platform's text services should do: `Some(true)` to enable the IME and soft
    /// keyboard, `Some(false)` to hide them, once after the change.
    pub fn take_ime_request(&mut self) -> Option<bool> {
        self.state.borrow_mut().typing.ime_request.take()
    }

    /// Committed text: typed characters, an IME's result, pasted text. Ignored unless a
    /// script is taking text.
    pub fn text_input(&mut self, text: &str) {
        if text.is_empty() || !self.wants_text() {
            return;
        }
        emit(&self.state, "text", text.to_string());
    }

    /// IME composition in progress (`cursor` is the selection inside `text`, in bytes).
    pub fn ime_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) {
        if !self.wants_text() {
            return;
        }
        let (a, b) = cursor.map_or((None, None), |(a, b)| (Some(a), Some(b)));
        emit(&self.state, "preedit", (text.to_string(), a, b));
    }

    /// A held key repeating. Only text fields care (backspace, arrows), so scripts get it as
    /// `key` while text is active.
    pub fn key_repeat(&mut self, name: &str) {
        if self.wants_text() && !is_character_key(name) {
            emit(&self.state, "key", name.to_string());
        }
    }
}

/// `kulisa.input.start_text()`, `stop_text()` and `text_active()`.
pub(super) fn install(lua: &Lua, input: &Table) -> mlua::Result<()> {
    use super::api::{number, with};
    input.set(
        "start_text",
        // start_text{ id = "widget id" } or start_text{ rect = { x, y, width, height } }: the field
        // the on-screen keyboard must not cover (the view lifts when it would).
        lua.create_function(|lua, opts: Option<Table>| {
            let focus = match opts {
                None => None,
                Some(o) => match (o.get::<LuaValue>("id")?, o.get::<LuaValue>("rect")?) {
                    (LuaValue::String(s), _) => Some(Focus::Widget(s.to_string_lossy().to_string())),
                    (_, LuaValue::Table(t)) => {
                        let c = |i: usize| -> mlua::Result<f32> { number(&t.raw_get(i)?, "rect") };
                        Some(Focus::Rect(Rect::new(c(1)?, c(2)?, c(3)?, c(4)?)))
                    }
                    _ => None,
                },
            };
            with(lua, |st| {
                st.typing.focus = focus;
                if !st.typing.active {
                    st.typing.active = true;
                    st.typing.ime_request = Some(true);
                }
                Ok(())
            })
        })?,
    )?;
    input.set(
        "stop_text",
        lua.create_function(|lua, ()| {
            with(lua, |st| {
                if st.typing.active {
                    st.typing.active = false;
                    st.typing.focus = None;
                    st.typing.ime_request = Some(false);
                }
                Ok(())
            })
        })?,
    )?;
    input.set("text_active", lua.create_function(|lua, ()| with(lua, |st| Ok(st.typing.active)))?)?;
    // keyboard_height() -> the on-screen keyboard's height in design units (0 when hidden).
    input.set(
        "keyboard_height",
        lua.create_function(|lua, ()| with(lua, |st| Ok(st.typing.keyboard_px / st.scale().max(1e-3))))?,
    )?;
    Ok(())
}

/// The window-pixel y range `(top, bottom)` of the field the keyboard must not cover.
fn field_range(st: &State) -> Option<(f32, f32)> {
    let rect = match st.typing.focus.as_ref()? {
        Focus::Rect(r) => *r,
        Focus::Widget(id) => st.ui.ui.screens.iter().find_map(|s| s.root.as_ref()?.find(id))?.rect,
    };
    let mut flat = st.viewport;
    flat.pan = Vec2::ZERO;
    let to_px = flat.layout(st.renderer.size()).to_pixels;
    Some((to_px.transform_point2(rect.min).y, to_px.transform_point2(rect.max).y))
}

impl Game {
    /// The on-screen keyboard's height in window pixels (0 when hidden). The runtime reports it
    /// every frame; while a field is active the view lifts so the field stays above it.
    pub fn set_keyboard_inset(&mut self, px: f32) {
        self.state.borrow_mut().typing.keyboard_px = px.max(0.0);
    }

    /// Eases the view up or down so the active field clears the keyboard.
    pub(super) fn update_pan(&mut self, dt: f32) {
        let mut st = self.state.borrow_mut();
        let window_h = st.renderer.size().y as f32;
        let target = match field_range(&st) {
            Some((top, bottom)) if st.typing.active && st.typing.keyboard_px > 0.0 => {
                let hidden = bottom + PAN_MARGIN - (window_h - st.typing.keyboard_px);
                // Never lift the field's top out of the window.
                hidden.clamp(0.0, top.max(0.0))
            }
            _ => 0.0,
        };
        let pan = st.typing.pan;
        let next =
            if (target - pan).abs() < 0.5 { target } else { pan + (target - pan) * (dt * 14.0).min(1.0) };
        st.typing.pan = next;
        st.viewport.pan = Vec2::new(0.0, -next);
    }
}
