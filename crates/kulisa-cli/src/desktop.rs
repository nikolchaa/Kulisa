//! Windows and Linux builds: the player renamed after the game, with `game.kpak` next to it,
//! which the player finds on its own. A released tool makes either on any computer; from source
//! each is compiled on its own system.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{Engine, Game, Options, Result, icon, pe, players, run, tar, write};

/// The Steam library for `target` from `[steam] sdk`, if the game uses Steam.
pub fn steam_library(game: &Game, target: &str) -> Result<Option<PathBuf>> {
    let c = &game.config;
    if c.steam_app_id.is_none() {
        return Ok(None);
    }
    let Some(sdk) = &c.steam_sdk else {
        eprintln!("warning: `[steam] app_id` without `sdk`: the build has no Steam library to run with");
        return Ok(None);
    };
    let file = match target {
        "windows" => "win64/steam_api64.dll",
        "macos" => "osx/libsteam_api.dylib",
        _ => "linux64/libsteam_api.so",
    };
    // The SDK's own layout: `sdk/public` and `sdk/redistributable_bin`.
    let path = game.dir.join(sdk).join("redistributable_bin").join(file);
    if path.is_file() {
        Ok(Some(path))
    } else {
        Err(format!("{} does not exist; `[steam] sdk` should name the Steamworks SDK folder", path.display()))
    }
}

/// An error unless this computer can compile for `target`.
pub(crate) fn compiles_here(target: &str) -> Result {
    if target == host() {
        return Ok(());
    }
    Err(format!(
        "from source, {target} builds are compiled on {target} and this computer runs {}; \
         the released kulisa tool builds them anywhere",
        host()
    ))
}

/// The Windows or Linux player: ready-made, or compiled when this computer runs that system.
pub fn player(opts: &Options, target: &str) -> Result<PathBuf> {
    let file = players::files(target)[0];
    match &opts.engine {
        Engine::Released(tool) => Ok(players::released(tool, target)?.join(file)),
        Engine::Source(root) => {
            compiles_here(target)?;
            run(&mut opts.cargo_player(root, &["--bin", "kulisa-player"]))?;
            Ok(root.join("target").join(opts.profile_dir()).join(file))
        }
    }
}

/// Gives the Windows program at `path` the game's icon, and takes away the console window of
/// builds for players (the window takes its icon from the game on every platform).
fn finish_windows(game: &Game, opts: &Options, path: &Path) -> Result {
    let mut exe = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(img) = icon::load(game)? {
        pe::set_icon(&mut exe, &icon::windows_images(&img))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if opts.shipping() {
        pe::hide_console(&mut exe).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    write(path, exe)
}

/// Builds `game` for `target` (`windows` or `linux`) into the folder `out`; returns the program.
pub fn build(game: &Game, opts: &Options, target: &str, out: &Path) -> Result<PathBuf> {
    let steam = steam_library(game, target)?;
    let built = player(opts, target)?;
    let name = game.file_name();
    let program = out.join(if target == "windows" { format!("{name}.exe") } else { name.clone() });
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    fs::copy(&built, &program).map_err(|e| format!("{}: {e}", built.display()))?;
    if target == "windows" {
        finish_windows(game, opts, &program)?;
    }
    if let Some(lib) = &steam {
        let name = lib.file_name().ok_or("the Steam library has no name")?;
        fs::copy(lib, out.join(name)).map_err(|e| format!("{}: {e}", lib.display()))?;
    }
    write(&out.join("game.kpak"), game.pack()?)?;
    if target == "windows" {
        crate::video::copy(game, out)?;
    } else {
        crate::video::copy_converted(game, out)?;
    }
    eprintln!("{target} build ready: {}", program.display());
    if cfg!(windows) && target != "windows" {
        let archive = out.with_file_name(format!("{name}-{target}.tar.gz"));
        let main = format!("{name}/{name}");
        tar::pack(out, &name, &|p| p == main || p.ends_with(".so"), &archive)?;
        eprintln!("ship this archive, which keeps the program runnable: {}", archive.display());
    }
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
