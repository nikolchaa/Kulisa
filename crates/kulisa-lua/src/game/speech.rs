//! `kulisa.speech`: reading text aloud with the system's voice. What scripts ask for here the
//! runtime carries out.

use mlua::{Lua, Table};

use super::Game;
use super::api::with;

/// Requests waiting for the runtime at most (a script must not pile them up).
const MAX_PENDING: usize = 32;

/// What the runtime should do with the system's voice.
#[derive(Clone, Debug, PartialEq)]
pub enum SpeechRequest {
    Say {
        text: String,
        /// A language code (`ja`), which picks the voice where the platform can.
        language: String,
        /// Stops what is being said first; otherwise this waits its turn.
        interrupt: bool,
        /// 0 to 1.
        volume: f32,
    },
    Stop,
}

impl Game {
    /// Speech that scripts asked for since the last call.
    pub fn take_speech(&mut self) -> Vec<SpeechRequest> {
        std::mem::take(&mut self.state.borrow_mut().speech)
    }
}

/// Text markup removed, as it should be read: `{b}Hi{/b} {{x}}` reads `Hi {x}`.
pub(crate) fn plain_text(markup: &str) -> String {
    let mut out = String::new();
    let mut rest = markup;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        if let Some(r) = rest.strip_prefix('{') {
            out.push('{');
            rest = r;
        } else if let Some(end) = rest.find('}') {
            rest = &rest[end + 1..];
        } else {
            rest = "";
        }
    }
    out.push_str(rest);
    out.replace("}}", "}")
}

pub(super) fn install(lua: &Lua, k: &Table) -> mlua::Result<()> {
    let s = lua.create_table()?;
    s.set("available", lua.create_function(|lua, ()| with(lua, |st| Ok(st.display.device.speech)))?)?;
    // say(text, { interrupt = true, volume = 1, language = current }): markup is left out.
    s.set(
        "say",
        lua.create_function(|lua, (text, opts): (String, Option<Table>)| {
            let get = |key: &str| opts.as_ref().map_or(Ok(None), |o| o.get::<Option<mlua::Value>>(key));
            let interrupt = !matches!(get("interrupt")?, Some(mlua::Value::Boolean(false)));
            let volume = match get("volume")? {
                Some(mlua::Value::Number(v)) => v as f32,
                Some(mlua::Value::Integer(v)) => v as f32,
                _ => 1.0,
            };
            let language = match get("language")? {
                Some(mlua::Value::String(l)) => Some(l.to_string_lossy()),
                _ => None,
            };
            with(lua, |st| {
                let language = language.unwrap_or_else(|| {
                    st.lang.current.clone().unwrap_or_else(|| st.config.source_language.clone())
                });
                let text = plain_text(&text);
                if st.speech.len() < MAX_PENDING && !text.trim().is_empty() {
                    let volume = volume.clamp(0.0, 1.0);
                    st.speech.push(SpeechRequest::Say { text, language, interrupt, volume });
                }
                Ok(())
            })
        })?,
    )?;
    s.set(
        "stop",
        lua.create_function(|lua, ()| {
            with(lua, |st| {
                st.speech.clear();
                st.speech.push(SpeechRequest::Stop);
                Ok(())
            })
        })?,
    )?;
    k.set("speech", s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_not_read_aloud() {
        assert_eq!(plain_text("{b}Hi{/b} there"), "Hi there");
        assert_eq!(plain_text("{color=#fff}a{/color} {{b}}"), "a {b}");
        assert_eq!(plain_text("broken {tag"), "broken ");
        assert_eq!(plain_text("plain"), "plain");
    }
}
