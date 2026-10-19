//! Text entry: the platform's committed text and IME composition, delivered to scripts as
//! `text` and `preedit` events while a text field is active.
//!
//! The engine only routes text. Text fields themselves (caret, editing keys, `ui.input`) are
//! written in Luau on top of this, so games can build their own.

use mlua::{Lua, Table};

use super::{Game, emit};

#[derive(Default)]
pub(crate) struct Typing {
    /// A script asked for text (a field has the focus).
    pub active: bool,
    /// What the runtime should do with the platform's IME / soft keyboard, once.
    pub ime_request: Option<bool>,
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
    use super::api::with;
    input.set(
        "start_text",
        lua.create_function(|lua, ()| {
            with(lua, |st| {
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
                    st.typing.ime_request = Some(false);
                }
                Ok(())
            })
        })?,
    )?;
    input.set("text_active", lua.create_function(|lua, ()| with(lua, |st| Ok(st.typing.active)))?)?;
    Ok(())
}
