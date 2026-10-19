//! Luau scripting. The only crate that talks to `mlua`; every binding wraps a public Rust API.

mod game;
mod story;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod web;

use mlua::{FromLuaMulti, IntoLuaMulti, Lua};

pub use game::Game;
pub use mlua::{Error, Result, Table};
pub use story::StoryHost;

/// A Luau VM with the `kulisa` API installed.
pub struct Scripting {
    lua: Lua,
}

impl Scripting {
    pub fn new() -> Result<Self> {
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        web::init();

        let lua = Lua::new();
        let api = lua.create_table()?;
        api.set("version", env!("CARGO_PKG_VERSION"))?;
        api.set(
            "log",
            lua.create_function(|_, msg: String| {
                log::info!(target: "luau", "{msg}");
                Ok(())
            })?,
        )?;
        lua.globals().set("kulisa", api)?;
        Ok(Self { lua })
    }

    /// Runs a chunk and returns whatever it returns.
    pub fn run<R: FromLuaMulti>(&self, name: &str, source: &str) -> Result<R> {
        self.lua.load(source).set_name(format!("@{name}")).eval()
    }

    /// Calls `module[name](args...)` on a table returned by a script.
    pub fn call<A: IntoLuaMulti, R: FromLuaMulti>(&self, module: &Table, name: &str, args: A) -> Result<R> {
        module.get::<mlua::Function>(name)?.call(args)
    }

    /// A story host sharing this VM's Luau state.
    pub fn story_host(&self) -> Result<StoryHost> {
        StoryHost::new(&self.lua)
    }

    /// Luau version string, e.g. `Luau 0.740`.
    pub fn luau_version(&self) -> String {
        self.lua.globals().get("_VERSION").unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_luau() {
        let s = Scripting::new().unwrap();
        let v: i64 = s.run("test", "return 40 + 2").unwrap();
        assert_eq!(v, 42);
        assert!(s.luau_version().starts_with("Luau"));
    }

    #[test]
    fn reports_errors_with_chunk_name() {
        let s = Scripting::new().unwrap();
        let err = s.run::<()>("hello.luau", "error('boom')").unwrap_err().to_string();
        assert!(err.contains("hello.luau:1"), "{err}");
        let err = s.run::<()>("bad.luau", "return )").unwrap_err().to_string();
        assert!(err.contains("bad.luau"), "{err}");
    }

    #[test]
    fn calls_module_functions() {
        let s = Scripting::new().unwrap();
        let m: Table = s.run("m", "return { add = function(a, b) return a + b end }").unwrap();
        let v: f64 = s.call(&m, "add", (1.5, 2.0)).unwrap();
        assert_eq!(v, 3.5);
    }
}
