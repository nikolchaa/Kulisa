//! Desktop builds for the host (Windows, Linux or macOS): the player renamed after the game,
//! with `game.kpak` next to it, which the player finds on its own.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{Game, Options, Result, icon, run, write};

/// The Steam library for this computer's platform from `[steam] sdk`, if the game uses Steam.
pub fn steam_library(game: &Game) -> Result<Option<PathBuf>> {
    let c = &game.config;
    if c.steam_app_id.is_none() {
        return Ok(None);
    }
    let Some(sdk) = &c.steam_sdk else {
        eprintln!("warning: `[steam] app_id` without `sdk`: the build has no Steam library to run with");
        return Ok(None);
    };
    let file = if cfg!(windows) {
        "win64/steam_api64.dll"
    } else if cfg!(target_os = "macos") {
        "osx/libsteam_api.dylib"
    } else {
        "linux64/libsteam_api.so"
    };
    // The SDK's own layout: `sdk/public` and `sdk/redistributable_bin`.
    let path = game.dir.join(sdk).join("redistributable_bin").join(file);
    if path.is_file() {
        Ok(Some(path))
    } else {
        Err(format!("{} does not exist; `[steam] sdk` should name the Steamworks SDK folder", path.display()))
    }
}

/// Builds `game` for this computer's platform into the folder `out`; returns the program.
/// On Windows the program carries the game's icon (the window takes it from the game on
/// every platform).
pub fn build(game: &Game, opts: &Options, out: &Path) -> Result<std::path::PathBuf> {
    let steam = steam_library(game)?;
    let mut cargo = opts.cargo_player(&["--bin", "kulisa-player"]);
    // The player's build script embeds KULISA_ICON; empty means the default icon.
    let ico = match icon::load(game)? {
        Some(img) if cfg!(windows) => {
            let path = opts.engine.join("target/kulisa-icons").join(format!("{}.ico", game.config.id));
            write(&path, icon::ico(&img))?;
            path.display().to_string()
        }
        _ => String::new(),
    };
    cargo.env("KULISA_ICON", ico);
    run(&mut cargo)?;
    let exe = if cfg!(windows) { "kulisa-player.exe" } else { "kulisa-player" };
    let built = opts.engine.join("target").join(opts.profile()).join(exe);
    let name = if cfg!(windows) { format!("{}.exe", game.file_name()) } else { game.file_name() };
    let program = out.join(name);
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    fs::copy(&built, &program).map_err(|e| format!("{}: {e}", built.display()))?;
    if let Some(lib) = &steam {
        let name = lib.file_name().ok_or("the Steam library has no name")?;
        fs::copy(lib, out.join(name)).map_err(|e| format!("{}: {e}", lib.display()))?;
    }
    write(&out.join("game.kpak"), game.pack()?)?;
    if cfg!(any(windows, target_os = "macos")) {
        crate::video::copy(game, out)?;
    } else {
        crate::video::copy_converted(game, out)?;
    }
    eprintln!("desktop build ready: {}", program.display());
    Ok(program)
}

/// The platform name `kulisa build` uses for this computer.
pub fn host() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}
