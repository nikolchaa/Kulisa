//! macOS app bundle: the player in a `.app` next to an Info.plist made from `kulisa.toml`, the
//! game and its icon. Signed with the game's Developer ID (`[macos] identity`), and notarized
//! when asked; ad-hoc signed otherwise, which runs here but makes other Macs warn. macOS hosts
//! only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Game, Options, Result, escape, icon, run, write};

const MIN_MACOS: &str = "11.0";
const PLAYER: &str = "kulisa-player";

/// How to package the app.
#[derive(Clone, Copy, Default)]
pub struct Package {
    /// Apple silicon and Intel in one program, instead of this computer's architecture.
    pub universal: bool,
    /// Send it to Apple's notary service and staple the ticket (needs a Developer ID).
    pub notarize: bool,
}

pub fn info_plist(game: &Game, executable: &str, icon: bool) -> String {
    let c = &game.config;
    let icon = if icon { "\n    <key>CFBundleIconFile</key><string>icon</string>" } else { "" };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>{title}</string>
    <key>CFBundleExecutable</key><string>{executable}</string>{icon}
    <key>CFBundleIdentifier</key><string>{id}</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>{title}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>{version}</string>
    <key>CFBundleVersion</key><string>{build}</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
    <key>LSMinimumSystemVersion</key><string>{MIN_MACOS}</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
"#,
        title = escape(&c.title),
        executable = escape(executable),
        id = escape(&c.macos_bundle_id),
        version = escape(&c.version),
        build = c.build,
    )
}

/// The player for this computer, or both architectures joined with `lipo`.
fn player(opts: &Options, universal: bool) -> Result<PathBuf> {
    let build = |args: &[&str]| run(opts.cargo_player(args).env("MACOSX_DEPLOYMENT_TARGET", MIN_MACOS));
    if !universal {
        build(&["--bin", PLAYER])?;
        return Ok(opts.engine.join("target").join(opts.profile()).join(PLAYER));
    }
    let mut lipo = Command::new("lipo");
    lipo.arg("-create");
    for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"] {
        build(&["--bin", PLAYER, "--target", target])?;
        lipo.arg(opts.engine.join("target").join(target).join(opts.profile()).join(PLAYER));
    }
    let joined = opts.engine.join("target").join(opts.profile()).join(format!("{PLAYER}-universal"));
    run(lipo.arg("-output").arg(&joined))?;
    Ok(joined)
}

/// Builds `game` into `out/Name.app`, returns the app. A notarized app also gets a zip to share.
pub fn build(game: &Game, opts: &Options, package: Package, out: &Path) -> Result<PathBuf> {
    let identity = game.config.macos_identity.as_deref();
    let notary = game.config.macos_notary_profile.as_deref();
    if package.notarize && (identity.is_none() || notary.is_none()) {
        return Err("--notarize needs `identity` and `notary_profile` under [macos] in kulisa.toml".into());
    }
    let built = player(opts, package.universal)?;

    let name = game.file_name();
    let app = out.join(format!("{name}.app"));
    let contents = app.join("Contents");
    let _ = fs::remove_dir_all(&app);
    fs::create_dir_all(contents.join("MacOS")).map_err(|e| e.to_string())?;
    fs::copy(&built, contents.join("MacOS").join(&name)).map_err(|e| format!("{}: {e}", built.display()))?;
    let icon = icon::load(game)?;
    if let Some(img) = &icon {
        write(&contents.join("Resources/icon.icns"), icon::icns(img))?;
    }
    write(&contents.join("Info.plist"), info_plist(game, &name, icon.is_some()))?;
    // The player also looks for game.kpak in Contents/Resources.
    write(&contents.join("Resources/game.kpak"), game.pack()?)?;

    let mut sign = Command::new("codesign");
    sign.arg("--force");
    match identity {
        // The hardened runtime and a timestamp are what notarization asks for.
        Some(id) => sign.args(["--options", "runtime", "--timestamp", "--sign", id]),
        None => {
            if opts.release {
                eprintln!("warning: ad-hoc signed; other Macs warn about it (see `[macos] identity`)");
            }
            sign.args(["--timestamp=none", "--sign", "-"])
        }
    };
    run(sign.arg(&app))?;

    if let (true, Some(profile)) = (package.notarize, notary) {
        let zip = out.join(format!("{name}.zip"));
        let zip_app = || run(Command::new("ditto").args(["-c", "-k", "--keepParent"]).arg(&app).arg(&zip));
        zip_app()?;
        run(Command::new("xcrun").args(["notarytool", "submit"]).arg(&zip).args([
            "--keychain-profile",
            profile,
            "--wait",
        ]))?;
        run(Command::new("xcrun").args(["stapler", "staple"]).arg(&app))?;
        // Again, so the zip holds the stapled ticket.
        zip_app()?;
    }
    eprintln!("macOS app ready: {}", app.display());
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kulisa_platform::GameConfig;

    #[test]
    fn plist_follows_the_config() {
        let toml = "[game]\nid = \"cats\"\ntitle = \"Cats & Co\"\nversion = \"2.0\"\nbuild = 4\n[macos]\nbundle_id = \"com.example.cats\"";
        let game = Game { dir: PathBuf::new(), config: GameConfig::parse(Some(toml)).unwrap() };
        let p = info_plist(&game, "Cats Co", true);
        assert!(p.contains("<key>CFBundleIdentifier</key><string>com.example.cats</string>"));
        assert!(p.contains("<key>CFBundleExecutable</key><string>Cats Co</string>"));
        assert!(p.contains("<key>CFBundleName</key><string>Cats &amp; Co</string>"));
        assert!(p.contains("<key>CFBundleVersion</key><string>4</string>"));
        assert!(p.contains("<key>CFBundleIconFile</key><string>icon</string>"));
        assert!(!info_plist(&game, "Cats Co", false).contains("CFBundleIconFile"));
    }
}
