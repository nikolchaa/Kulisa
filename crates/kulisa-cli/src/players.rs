//! Where players come from. An engine checkout compiles them with cargo. A released `kulisa`
//! needs no compiler: it uses the ready-made players beside it, or downloads the one a build
//! needs into this computer's cache, once per version.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Engine, Options, Result, android, desktop, ios, macos, run, tar, web};

/// Where releases are downloaded from; `KULISA_RELEASES` points somewhere else (a mirror, tests).
const RELEASES: &str = "https://github.com/nikolchaa/Kulisa/releases/download";
const RELEASES_VAR: &str = "KULISA_RELEASES";
/// Every target with a player of its own.
pub const TARGETS: &[&str] = &["windows", "linux", "macos", "web", "android", "ios"];

/// What a player folder has inside, by target. The first file also says the folder is whole.
pub fn files(target: &str) -> &'static [&'static str] {
    match target {
        "windows" => &["kulisa-player.exe"],
        "linux" | "macos" => &["kulisa-player"],
        "web" => &["kulisa_bg.wasm", "kulisa.js"],
        "android" => &["arm64-v8a/libkulisa.so", "x86_64/libkulisa.so", "armeabi-v7a/libkulisa.so"],
        "ios" => &["device/kulisa-player", "sim/kulisa-player"],
        _ => &[],
    }
}

/// The archive of one target's player in a release.
pub fn archive_name(target: &str) -> String {
    format!("kulisa-player-{target}.tar.gz")
}

/// The folder this computer keeps downloads in.
pub fn cache_dir() -> Result<PathBuf> {
    let var = |name: &str| env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        var("LOCALAPPDATA").map(|d| d.join("Kulisa"))
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|d| d.join("Library/Caches/Kulisa"))
    } else {
        var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|d| d.join(".cache"))).map(|d| d.join("kulisa"))
    };
    dir.ok_or_else(|| "no folder for downloads: HOME is not set".to_string())
}

/// Finds `target`'s player for this version of the tool: beside the tool, in the cache, or by
/// downloading it.
pub fn released(tool: &Path, target: &str) -> Result<PathBuf> {
    let version = env!("CARGO_PKG_VERSION");
    let whole = |dir: &Path| files(target).first().is_some_and(|f| dir.join(f).is_file());
    let beside = tool.join("players").join(target);
    if whole(&beside) {
        return Ok(beside);
    }
    let versions = cache_dir()?.join("players").join(version);
    let cached = versions.join(target);
    if whole(&cached) {
        return Ok(cached);
    }

    let base = env::var(RELEASES_VAR).unwrap_or_else(|_| format!("{RELEASES}/v{version}"));
    let url = format!("{}/{}", base.trim_end_matches('/'), archive_name(target));
    eprintln!("downloading the {target} player for Kulisa {version} (only this once)...");
    // Unpacked aside, so a stopped download never looks like a whole player.
    let partial = versions.join(format!("{target}.part"));
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
    let archive = partial.join("player.tar.gz");
    run(Command::new("curl").args(["-fL", "--progress-bar", "-o"]).arg(&archive).arg(&url))
        .map_err(|e| format!("could not download {url}: {e}"))?;
    run(Command::new("tar").arg("xzf").arg(&archive).arg("-C").arg(&partial))?;
    fs::remove_file(&archive).map_err(|e| e.to_string())?;
    if !whole(&partial) {
        return Err(format!("{url} does not hold the {target} player"));
    }
    let _ = fs::remove_dir_all(&cached);
    fs::rename(&partial, &cached).map_err(|e| format!("{}: {e}", cached.display()))?;
    Ok(cached)
}

fn copy_file(from: &Path, to: &Path) -> Result {
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::copy(from, to).map(|_| ()).map_err(|e| format!("{}: {e}", from.display()))
}

/// The linker signs only for Apple silicon; Intel slices and joined programs need this.
fn sign_ad_hoc(program: &Path) -> Result {
    run(Command::new("codesign").args(["--force", "--sign", "-", "--timestamp=none"]).arg(program))
}

/// Compiles `target`'s player into `out/target`, and its release archive into `out`.
pub fn package(opts: &Options, target: &str, out: &Path) -> Result {
    let folder = out.join(target);
    let _ = fs::remove_dir_all(&folder);
    match target {
        "windows" | "linux" => copy_file(&desktop::player(opts, target)?, &folder.join(files(target)[0]))?,
        "macos" => {
            let to = folder.join(files(target)[0]);
            copy_file(&macos::player(opts, true)?, &to)?;
            sign_ad_hoc(&to)?;
        }
        "web" => {
            let player = web::player(opts)?;
            for file in files(target) {
                copy_file(&player.join(file), &folder.join(file))?;
            }
        }
        "android" => {
            for (abi, _) in android::ABIS {
                copy_file(&android::library(opts, abi)?, &folder.join(abi).join("libkulisa.so"))?;
            }
        }
        "ios" => {
            for sim in [false, true] {
                copy_file(&ios::player(opts, sim)?, &folder.join(files(target)[sim as usize]))?;
            }
        }
        other => return Err(format!("no player for `{other}`; targets: {}", TARGETS.join(", "))),
    }
    let archive = out.join(archive_name(target));
    tar::pack(&folder, "", &|_| true, &archive)?;
    eprintln!("player ready: {}", archive.display());
    Ok(())
}

/// Compiles this computer's download from a release into `out`: the `kulisa` tool, the templates
/// and this computer's player, so a new game runs before anything else is downloaded.
pub fn package_tool(opts: &Options, out: &Path) -> Result {
    let Engine::Source(root) = &opts.engine else {
        return Err("releases are made from the engine's source".into());
    };
    let host = desktop::host();
    package(opts, host, out)?;

    let folder = out.join("kulisa");
    let _ = fs::remove_dir_all(&folder);
    let tool = if host == "macos" {
        macos::compile(opts, root, "kulisa-cli", "kulisa", true)?
    } else {
        run(&mut opts.cargo(root, "kulisa-cli", &["--bin", "kulisa"]))?;
        let name = if host == "windows" { "kulisa.exe" } else { "kulisa" };
        root.join("target").join(opts.profile_dir()).join(name)
    };
    let name = if host == "windows" { "kulisa.exe" } else { "kulisa" };
    copy_file(&tool, &folder.join(name))?;
    if host == "macos" {
        sign_ad_hoc(&folder.join(name))?;
    }
    copy_file(&root.join("LICENSE"), &folder.join("LICENSE"))?;
    crate::copy_game(&opts.engine.templates(), &folder.join("templates"))?;
    crate::copy_tree(&out.join(host), &folder.join("players").join(host))?;

    if host == "windows" {
        // Windows' own tar writes zips, which everyone there can open.
        let archive = out.join("kulisa-windows.zip");
        run(Command::new("tar").current_dir(out).args(["-a", "-c", "-f"]).arg(&archive).arg("kulisa"))?;
        eprintln!("tool ready: {}", archive.display());
    } else {
        let archive = out.join(format!("kulisa-{host}.tar.gz"));
        let executable = |p: &str| p == "kulisa/kulisa" || p.ends_with("/kulisa-player");
        tar::pack(&folder, "kulisa", &executable, &archive)?;
        eprintln!("tool ready: {}", archive.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_has_files() {
        for target in TARGETS {
            assert!(!files(target).is_empty(), "{target}");
        }
        assert!(archive_name("web").ends_with(".tar.gz"));
    }

    #[test]
    fn a_player_beside_the_tool_wins() {
        let tool = env::temp_dir().join(format!("kulisa-players-{}", std::process::id()));
        let web = tool.join("players/web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("kulisa_bg.wasm"), b"x").unwrap();
        assert_eq!(released(&tool, "web").unwrap(), web);
        let _ = fs::remove_dir_all(&tool);
    }
}
