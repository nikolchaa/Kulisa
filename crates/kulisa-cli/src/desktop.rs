//! Desktop builds for the host (Windows, Linux or macOS): the player renamed after the game,
//! with `game.kpak` next to it, which the player finds on its own.

use std::fs;
use std::path::Path;

use crate::{Game, Options, Result, icon, run, write};

/// Builds `game` for this computer's platform into the folder `out`; returns the program.
/// On Windows the program carries the game's icon (the window takes it from the game on
/// every platform).
pub fn build(game: &Game, opts: &Options, out: &Path) -> Result<std::path::PathBuf> {
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
    write(&out.join("game.kpak"), game.pack()?)?;
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
