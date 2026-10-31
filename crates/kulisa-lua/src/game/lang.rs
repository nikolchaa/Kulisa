//! `kulisa.lang` and `kulisa.t`: the game's languages from `lang/*.txt`. The story looks its
//! lines up in the current language as it shows them, and `kulisa.t` does the same for the
//! interface. The player's choice is remembered; a first run follows Steam or the system.

use std::collections::HashSet;

use kulisa_platform::Storage;
use kulisa_story::{Language, parse_language};
use mlua::{Lua, Table, Value as LuaValue};

use super::api::{rt, shared, with};
use super::ui::refresh_all;
use super::{State, Waiting, emit, story_restored, with_story};

const FOLDER: &str = "settings";
const FILE: &str = "language";

#[derive(Default)]
pub(crate) struct Lang {
    pub languages: Vec<Language>,
    /// The translation in use; `None` is the game's own language.
    pub current: Option<String>,
    fonts: HashSet<String>,
}

impl Lang {
    fn language(&self, code: &str) -> Option<&Language> {
        self.languages.iter().find(|l| l.code == code)
    }

    fn translate<'a>(&'a self, text: &'a str) -> &'a str {
        let found = self.current.as_deref().and_then(|c| self.language(c)).and_then(|l| l.text(text));
        found.unwrap_or(text)
    }
}

/// Reads the language files; what was loaded before stays when one is broken.
pub(super) fn load(st: &mut State) -> Result<(), String> {
    let mut languages = Vec::new();
    for path in st.assets.list("") {
        if path.starts_with("lang/") && path.ends_with(".txt") {
            let source = st.assets.read_string(&path)?;
            languages.push(parse_language(&path, &source).map_err(|e| e.to_string())?);
        }
    }
    st.lang.languages = languages;
    let current = st.lang.current.clone();
    if current.is_some_and(|c| st.lang.language(&c).is_none()) {
        st.lang.current = None;
    }
    Ok(())
}

/// The best of the game's languages for `wanted` (`ja-JP`, `pt_BR`): that exact one, else the
/// same language in another region (`ja`, or `pt-BR` for `pt-PT`).
fn matching(st: &State, wanted: &str) -> Option<String> {
    let wanted = wanted.replace('_', "-").to_ascii_lowercase();
    let primary = wanted.split('-').next().unwrap_or("");
    let code = |l: &str| l.replace('_', "-").to_ascii_lowercase();
    let source = st.config.source_language.as_str();
    let codes = || st.lang.languages.iter().map(|l| l.code.as_str()).chain([source]);
    let exact = codes().find(|c| code(c) == wanted);
    let same = || codes().find(|c| code(c).split('-').next() == Some(primary));
    exact.or_else(same).map(str::to_string)
}

/// Steam's name for a language as a language code.
fn steam_code(name: &str) -> Option<&'static str> {
    Some(match name {
        "english" => "en",
        "japanese" => "ja",
        "koreana" => "ko",
        "schinese" => "zh-Hans",
        "tchinese" => "zh-Hant",
        "german" => "de",
        "french" => "fr",
        "italian" => "it",
        "spanish" => "es",
        "latam" => "es-419",
        "brazilian" => "pt-BR",
        "portuguese" => "pt",
        "russian" => "ru",
        "ukrainian" => "uk",
        "polish" => "pl",
        "czech" => "cs",
        "hungarian" => "hu",
        "dutch" => "nl",
        "turkish" => "tr",
        "thai" => "th",
        "vietnamese" => "vi",
        "indonesian" => "id",
        _ => return None,
    })
}

fn storage(st: &State) -> Option<Storage> {
    Storage::new(&st.config.id, FOLDER).ok()
}

/// Loads the language's font the first time it is used.
fn load_font(st: &mut State, code: &str) -> Result<(), String> {
    let Some(font) = st.lang.language(code).and_then(|l| l.font.clone()) else { return Ok(()) };
    if st.lang.fonts.insert(font.clone()) {
        let bytes = st.assets.read(&font)?;
        if st.text.load_font(bytes).is_empty() {
            return Err(format!("`{font}` holds no usable font"));
        }
    }
    Ok(())
}

/// Picks the language to start in: forced for testing, the player's choice, Steam's, the system's.
pub(super) fn choose(st: &mut State) -> Result<(), String> {
    if st.lang.languages.is_empty() {
        return Ok(());
    }
    let saved = storage(st)
        .and_then(|s| s.read(FILE).ok().flatten())
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());
    let steam = st.steam.as_ref().and_then(|s| s.language()).and_then(|n| steam_code(&n).map(str::to_string));
    let found = [kulisa_platform::language_override(), saved, steam, kulisa_platform::system_language()]
        .into_iter()
        .flatten()
        .find_map(|wanted| matching(st, &wanted));
    if let Some(code) = found.filter(|c| *c != st.config.source_language) {
        load_font(st, &code)?;
        st.lang.current = Some(code);
    }
    Ok(())
}

pub(super) fn install(lua: &Lua, k: &Table) -> mlua::Result<()> {
    // t(text, vars?) -> the text in the current language. `{name}` takes `vars.name`, in the
    // original and the translation alike.
    k.set(
        "t",
        lua.create_function(|lua, (text, vars): (String, Option<Table>)| {
            let mut out = with(lua, |st| Ok(st.lang.translate(&text).to_string()))?;
            if let Some(vars) = vars {
                for pair in vars.pairs::<String, LuaValue>() {
                    let (key, value) = pair?;
                    let value = value.to_string()?;
                    out = out.replace(&format!("{{{key}}}"), &value);
                }
            }
            Ok(out)
        })?,
    )?;

    let l = lua.create_table()?;
    // mark(text) -> text: shows `kulisa translate` a string that goes through `kulisa.t` later,
    // like an item's name in a table built when the game starts.
    l.set("mark", lua.create_function(|_, text: String| Ok(text))?)?;
    // list() -> { { code, name }, ... }: the game's own language first.
    l.set(
        "list",
        lua.create_function(|lua, ()| {
            with(lua, |st| {
                let list = lua.create_table()?;
                let add = |code: &str, name: &str| -> mlua::Result<()> {
                    let item = lua.create_table()?;
                    item.set("code", code)?;
                    item.set("name", name)?;
                    list.raw_push(item)
                };
                add(&st.config.source_language, &st.config.source_language_name)?;
                for language in &st.lang.languages {
                    add(&language.code, &language.name)?;
                }
                Ok(list)
            })
        })?,
    )?;
    l.set(
        "current",
        lua.create_function(|lua, ()| {
            with(lua, |st| Ok(st.lang.current.clone().unwrap_or_else(|| st.config.source_language.clone())))
        })?,
    )?;
    // set(code): switches now, including the current line and every screen, and remembers it.
    l.set(
        "set",
        lua.create_function(|lua, code: String| {
            with(lua, |st| {
                let own = code == st.config.source_language;
                if !own && st.lang.language(&code).is_none() {
                    let known: Vec<&str> = st.lang.languages.iter().map(|l| l.code.as_str()).collect();
                    return Err(mlua::Error::runtime(format!(
                        "the game has no language `{code}`; it has {}, {}",
                        st.config.source_language,
                        known.join(", ")
                    )));
                }
                if !own {
                    rt(load_font(st, &code))?;
                }
                st.lang.current = (!own).then(|| code.clone());
                if let Some(s) = storage(st)
                    && let Err(e) = s.write(FILE, code.as_bytes())
                {
                    log::warn!("remembering the language: {e}");
                }
                refresh_all(st);
                Ok(())
            })?;
            let sh = shared(lua);
            let language = sh.borrow().lang.current.clone();
            // From a story line the story is busy; the next line shows the new language.
            if sh.borrow().story.is_some() {
                let reshow = rt(with_story(&sh, false, |s| {
                    s.vm.set_language(language);
                    Ok(matches!(s.waiting, Waiting::Line | Waiting::Menu) && s.vm.reshow())
                }))?;
                if reshow {
                    rt(story_restored(lua, &sh))?;
                }
            }
            emit(&sh, "language", code);
            Ok(())
        })?,
    )?;
    k.set("lang", l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_names_become_codes() {
        assert_eq!(steam_code("japanese"), Some("ja"));
        assert_eq!(steam_code("brazilian"), Some("pt-BR"));
        assert_eq!(steam_code("klingon"), None);
    }
}
