//! iOS app bundle without an Xcode project: cargo builds the `kulisa-player` binary, which goes
//! into a `.app` next to an Info.plist made from `kulisa.toml` and is ad-hoc signed. macOS
//! hosts only.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::{Game, Options, Result, escape, run, write};

/// Keep in sync with IPHONEOS_DEPLOYMENT_TARGET in .cargo/config.toml.
const MIN_IOS: &str = "15.0";
const EXECUTABLE: &str = "kulisa-player";

pub fn info_plist(game: &Game, platform: &str) -> String {
    let c = &game.config;
    let orientations: &[&str] = match c.locked_orientation() {
        "landscape" => &["UIInterfaceOrientationLandscapeLeft", "UIInterfaceOrientationLandscapeRight"],
        "portrait" => &["UIInterfaceOrientationPortrait"],
        _ => &[
            "UIInterfaceOrientationPortrait",
            "UIInterfaceOrientationLandscapeLeft",
            "UIInterfaceOrientationLandscapeRight",
        ],
    };
    let orientations: String =
        orientations.iter().map(|o| format!("\n        <string>{o}</string>")).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>{title}</string>
    <key>CFBundleExecutable</key><string>{EXECUTABLE}</string>
    <key>CFBundleIdentifier</key><string>{id}</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>{title}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>{version}</string>
    <key>CFBundleVersion</key><string>{build}</string>
    <key>CFBundleSupportedPlatforms</key><array><string>{platform}</string></array>
    <key>LSRequiresIPhoneOS</key><true/>
    <key>MinimumOSVersion</key><string>{MIN_IOS}</string>
    <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
    <key>UILaunchScreen</key><dict/>
    <key>UIRequiresFullScreen</key><true/>
    <key>UIStatusBarHidden</key><true/>
    <key>UISupportedInterfaceOrientations</key>
    <array>{orientations}
    </array>
</dict>
</plist>
"#,
        title = escape(&c.title),
        id = escape(&c.ios_bundle_id),
        version = escape(&c.version),
        build = c.build,
    )
}

/// Builds `game` into the app bundle `app` (`Name.app`), for the simulator when `sim`.
pub fn build(game: &Game, opts: &Options, sim: bool, app: &Path) -> Result {
    let (target, platform) =
        if sim { ("aarch64-apple-ios-sim", "iPhoneSimulator") } else { ("aarch64-apple-ios", "iPhoneOS") };
    run(&mut opts.cargo_player(&["--bin", EXECUTABLE, "--target", target]))?;

    let bin = opts.engine.join("target").join(target).join(opts.profile()).join(EXECUTABLE);
    let _ = fs::remove_dir_all(app);
    fs::create_dir_all(app).map_err(|e| e.to_string())?;
    fs::copy(&bin, app.join(EXECUTABLE)).map_err(|e| format!("{}: {e}", bin.display()))?;
    write(&app.join("Info.plist"), info_plist(game, platform))?;
    // The player looks for game.kpak next to its executable.
    write(&app.join("game.kpak"), game.pack()?)?;

    // The simulator refuses unsigned arm64 apps; an ad-hoc signature is enough there.
    // Devices need a real identity and provisioning profile, which this does not provide.
    run(Command::new("codesign").args(["--force", "--sign", "-", "--timestamp=none"]).arg(app))?;
    eprintln!("iOS app ready: {} ({target})", app.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kulisa_platform::GameConfig;
    use std::path::PathBuf;

    #[test]
    fn plist_follows_the_config() {
        let toml = "[game]\nid = \"cats\"\ntitle = \"Cats\"\nversion = \"2.0\"\n[ios]\nbundle_id = \"com.example.cats\"";
        let game = Game { dir: PathBuf::new(), config: GameConfig::parse(Some(toml)).unwrap() };
        let p = info_plist(&game, "iPhoneOS");
        assert!(p.contains("<key>CFBundleIdentifier</key><string>com.example.cats</string>"));
        assert!(p.contains("<key>CFBundleShortVersionString</key><string>2.0</string>"));
        assert!(p.contains("UIInterfaceOrientationLandscapeLeft") && !p.contains("OrientationPortrait"));
    }
}
