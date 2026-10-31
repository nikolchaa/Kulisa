//! `kulisa.steam`: achievements and stats when the game runs with Steam (`[steam] app_id`). Every
//! call is safe without it, so one script serves every platform and store.

use kulisa_platform::SteamApi;
use mlua::{Lua, Table};

use super::api::with;
use super::{Game, State};

impl Game {
    /// Replaces the Steam connection (tests).
    pub fn set_steam(&mut self, steam: Option<Box<dyn SteamApi>>) {
        self.state.borrow_mut().steam = steam;
    }
}

/// Connects when `kulisa.toml` asks for Steam. Running without it is normal: players can own
/// the game elsewhere.
pub(super) fn connect(st: &mut State) {
    let Some(app_id) = st.config.steam_app_id else { return };
    match kulisa_platform::steam_connect(app_id) {
        Ok(steam) => {
            log::info!("Steam connected as app {app_id}");
            st.steam = Some(steam);
        }
        Err(e) => log::info!("running without Steam: {e}"),
    }
}

pub(super) fn install(lua: &Lua, k: &Table) -> mlua::Result<()> {
    let s = lua.create_table()?;
    s.set("available", lua.create_function(|lua, ()| with(lua, |st| Ok(st.steam.is_some())))?)?;
    // unlock(id) -> whether Steam took it. Sends it to Steam at once, which shows the toast.
    s.set(
        "unlock",
        lua.create_function(|lua, id: String| {
            with(lua, |st| Ok(st.steam.as_mut().is_some_and(|s| s.unlock(&id) && s.store())))
        })?,
    )?;
    // clear(id): locks an achievement again (for testing).
    s.set(
        "clear",
        lua.create_function(|lua, id: String| {
            with(lua, |st| Ok(st.steam.as_mut().is_some_and(|s| s.clear(&id) && s.store())))
        })?,
    )?;
    // unlocked(id) -> true or false; nil without Steam or for an unknown achievement.
    s.set(
        "unlocked",
        lua.create_function(|lua, id: String| {
            with(lua, |st| Ok(st.steam.as_ref().and_then(|s| s.unlocked(&id))))
        })?,
    )?;
    // progress(id, current, max): shows Steam's progress toast.
    s.set(
        "progress",
        lua.create_function(|lua, (id, current, max): (String, u32, u32)| {
            with(lua, |st| Ok(st.steam.as_mut().is_some_and(|s| s.progress(&id, current, max))))
        })?,
    )?;
    // stat(name) -> number; nil without Steam or for an unknown stat.
    s.set(
        "stat",
        lua.create_function(|lua, name: String| {
            with(lua, |st| Ok(st.steam.as_ref().and_then(|s| s.stat(&name))))
        })?,
    )?;
    // set_stat(name, value) -> whether Steam took it. Kept locally until store().
    s.set(
        "set_stat",
        lua.create_function(|lua, (name, value): (String, f64)| {
            with(lua, |st| Ok(st.steam.as_mut().is_some_and(|s| s.set_stat(&name, value))))
        })?,
    )?;
    // store(): sends changed stats to Steam. For meaningful moments, not every frame.
    s.set(
        "store",
        lua.create_function(|lua, ()| with(lua, |st| Ok(st.steam.as_mut().is_some_and(|s| s.store()))))?,
    )?;
    // language() -> Steam's language name for this game (`english`, `japanese`), or nil.
    s.set(
        "language",
        lua.create_function(|lua, ()| with(lua, |st| Ok(st.steam.as_ref().and_then(|s| s.language()))))?,
    )?;
    k.set("steam", s)
}
