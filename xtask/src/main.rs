//! Engine development tasks: the `kulisa` command's builds with the engine's defaults (the
//! smoke demo, output in `dist/`), as CI runs them.
//!
//!   cargo xtask web [--release] [--serve [PORT]]   build the web player into dist/web
//!   cargo xtask apk [--release] [--abi LIST]       build an Android APK into dist/android
//!   cargo xtask ios-app [--sim] [--release]        build an iOS .app into dist/ios (macOS only)
//!   cargo xtask pack [--out FILE]                  pack a game folder into a .kpak
//!   cargo xtask player TARGET [--out DIR]          a release's player for TARGET
//!   cargo xtask tool [--out DIR] [--players DIR]   a release's download for this computer
//!   cargo xtask licenses [--check]                 the list of third-party licenses
//!
//! The first four take `--game DIR`, the game to bundle (default `demos/smoke`). Releases go to
//! `dist/release`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kulisa_cli::{Engine, Game, Options, Profile, Result, android, ios, players, serve, web};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("web") => web_task(&args[1..]),
        Some("apk") => apk(&args[1..]),
        Some("ios-app") => ios_app(&args[1..]),
        Some("pack") => pack(&args[1..]),
        Some("player") => player(&args[1..]),
        Some("tool") => tool(&args[1..]),
        Some("licenses") => licenses(&args[1..]),
        _ => Err("usage: cargo xtask web [--release] [--serve [PORT]] | apk [--release] [--abi LIST] | \
                  ios-app [--sim] [--release] | pack [--out FILE] (all take --game DIR) | \
                  player TARGET [--out DIR] | tool [--out DIR] [--players DIR] | licenses [--check]"
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
    let profile = if args.iter().any(|a| a == "--release") { Profile::Release } else { Profile::Debug };
    Ok((Game::open(&dir)?, Options { engine: Engine::Source(root()), profile }))
}

/// How releases are built, and the folder they go to.
fn release_setup(args: &[String]) -> Result<(Options, PathBuf)> {
    let out = option(args, "--out")?.map_or_else(|| root().join("dist/release"), PathBuf::from);
    Ok((Options { engine: Engine::Source(root()), profile: Profile::Dist }, out))
}

fn player(args: &[String]) -> Result {
    let (opts, out) = release_setup(args)?;
    let target = args.first().ok_or("cargo xtask player TARGET [--out DIR]")?;
    players::package(&opts, target, &out)
}

fn tool(args: &[String]) -> Result {
    let (opts, out) = release_setup(args)?;
    players::package_tool(&opts, &out, option(args, "--players")?.map(Path::new))
}

/// Writes the list games and downloads ship with, or with `--check` fails if it is out of date.
fn licenses(args: &[String]) -> Result {
    let root = root();
    let text = |path: &str| {
        fs::read_to_string(root.join(path))
            .map(|t| t.replace("\r\n", "\n"))
            .map_err(|e| format!("{path}: {e}"))
    };
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(&cargo).current_dir(&root).args(["fetch", "--locked"]).status();
    status.map_err(|e| e.to_string())?;
    let about = "target/third-party-licenses.txt";
    let generated = Command::new(&cargo)
        .current_dir(&root)
        .args(["about", "generate", "--locked", "--offline", "-c", "licenses/about.toml"])
        .args(["-o", about, "licenses/about.hbs"])
        .output()
        .map_err(|e| e.to_string())?;
    if !generated.status.success() {
        return Err(format!(
            "cargo about failed (install it with `cargo install cargo-about --locked --features cli`):\n{}",
            String::from_utf8_lossy(&generated.stderr)
        ));
    }
    let list =
        format!("Kulisa\n======\n\n{}\n{}\n{}", text("LICENSE")?, text(about)?, text("licenses/extra.txt")?);

    let file = "crates/kulisa-cli/THIRD-PARTY-LICENSES.txt";
    if !args.iter().any(|a| a == "--check") {
        fs::write(root.join(file), list).map_err(|e| format!("{file}: {e}"))?;
        eprintln!("wrote {file}");
    } else if text(file)? != list {
        return Err(format!("{file} is out of date: run `cargo xtask licenses`"));
    }
    Ok(())
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
