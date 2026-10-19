//! Luau scripting. The only crate that talks to `mlua`; every binding wraps a public Rust API.

mod game;
mod story;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod web;

pub use game::Game;
pub use story::StoryHost;
