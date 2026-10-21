//! Engine development tasks: the `kulisa` command's builds with the engine's defaults (the
//! smoke demo, output in `dist/`), as CI runs them.
//!
//!   cargo xtask web [--release] [--serve [PORT]]   build the web player into dist/web
//!   cargo xtask apk [--release] [--abi LIST]       build an Android APK into dist/android
//!   cargo xtask ios-app [--sim] [--release]        build an iOS .app into dist/ios (macOS only)
//!   cargo xtask pack [--out FILE]                  pack a game folder into a .kpak
//!
//! Every command takes `--game DIR`, the game to bundle (default `demos/smoke`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use kulisa_cli::{Game, Options, Result, android, ios, serve, web};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("web") => web_task(&args[1..]),
        Some("apk") => apk(&args[1..]),
        Some("ios-app") => ios_app(&args[1..]),
        Some("pack") => pack(&args[1..]),
        _ => Err("usage: cargo xtask web [--release] [--serve [PORT]] | apk [--release] [--abi LIST] | \
                  ios-app [--sim] [--release] | pack [--out FILE]; all take --game DIR"
            .into()),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// The value after `flag`, if given.
fn option<'a>(args: &'a [String], flag: &str) -> Result<Option<&'a str>> {
    match args.iter().position(|a| a == flag) {
        Some(i) => args.get(i + 1).map(|v| Some(v.as_str())).ok_or(format!("{flag} needs a value")),
        None => Ok(None),
    }
}

fn setup(args: &[String]) -> Result<(Game, Options)> {
    let dir = option(args, "--game")?.map_or_else(|| root().join("demos/smoke"), PathBuf::from);
    let opts = Options { engine: root(), release: args.iter().any(|a| a == "--release") };
    Ok((Game::open(&dir)?, opts))
}

fn web_task(args: &[String]) -> Result {
    let (game, opts) = setup(args)?;
    let out = root().join("dist/web");
    web::build(&game, &opts, &out)?;
    match args.iter().position(|a| a == "--serve") {
        Some(i) => serve::serve(&out, args.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(8080), None),
        None => Ok(()),
    }
}

fn apk(args: &[String]) -> Result {
    let (game, opts) = setup(args)?;
    let abis: Vec<&str> = option(args, "--abi")?.map_or(vec!["arm64-v8a"], |a| a.split(',').collect());
    let apk = root().join("dist/android/kulisa.apk");
    android::build(&game, &opts, &abis, &apk)?;
    eprintln!("install with: adb install -r \"{}\"", apk.display());
    Ok(())
}

fn ios_app(args: &[String]) -> Result {
    let (game, opts) = setup(args)?;
    ios::build(&game, &opts, args.iter().any(|a| a == "--sim"), &root().join("dist/ios/Kulisa.app"))
}

fn pack(args: &[String]) -> Result {
    let (game, _) = setup(args)?;
    let out = option(args, "--out")?.unwrap_or("game.kpak");
    fs::write(out, game.pack()?).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("wrote {out}");
    Ok(())
}
