//! iOS app bundle without an Xcode project: cargo builds the `kulisa-player` binary, which goes
//! into a `.app` next to a generated Info.plist and is ad-hoc signed. macOS hosts only.

use std::env;
use std::fs;
use std::process::Command;

use crate::{Result, game_pack, root, run};

/// Keep in sync with IPHONEOS_DEPLOYMENT_TARGET in .cargo/config.toml.
const MIN_IOS: &str = "15.0";
const BUNDLE_ID: &str = "dev.kulisa.player";
const EXECUTABLE: &str = "kulisa-player";

fn info_plist(platform: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>Kulisa</string>
    <key>CFBundleExecutable</key><string>{EXECUTABLE}</string>
    <key>CFBundleIdentifier</key><string>{BUNDLE_ID}</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>Kulisa</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>{version}</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>CFBundleSupportedPlatforms</key><array><string>{platform}</string></array>
    <key>LSRequiresIPhoneOS</key><true/>
    <key>MinimumOSVersion</key><string>{MIN_IOS}</string>
    <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
    <key>UILaunchScreen</key><dict/>
    <key>UIRequiresFullScreen</key><true/>
    <key>UIStatusBarHidden</key><true/>
    <key>UISupportedInterfaceOrientations</key>
    <array>
        <string>UIInterfaceOrientationPortrait</string>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
    </array>
</dict>
</plist>
"#
    )
}

pub fn app(args: &[String]) -> Result {
    let release = args.iter().any(|a| a == "--release");
    let sim = args.iter().any(|a| a == "--sim");
    let (target, platform) =
        if sim { ("aarch64-apple-ios-sim", "iPhoneSimulator") } else { ("aarch64-apple-ios", "iPhoneOS") };

    let root = root();
    let mut cargo = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo.current_dir(&root).args(["build", "-p", "kulisa-player", "--bin", EXECUTABLE, "--target", target]);
    if release {
        cargo.arg("--release");
    }
    run(&mut cargo)?;

    let profile = if release { "release" } else { "debug" };
    let bin = root.join("target").join(target).join(profile).join(EXECUTABLE);
    let app = root.join("dist/ios/Kulisa.app");
    let _ = fs::remove_dir_all(&app);
    fs::create_dir_all(&app).map_err(|e| e.to_string())?;
    fs::copy(&bin, app.join(EXECUTABLE)).map_err(|e| format!("{}: {e}", bin.display()))?;
    fs::write(app.join("Info.plist"), info_plist(platform)).map_err(|e| e.to_string())?;
    // The player looks for game.kpak next to its executable.
    fs::write(app.join("game.kpak"), game_pack(args)?).map_err(|e| e.to_string())?;

    // The simulator refuses unsigned arm64 apps; an ad-hoc signature is enough there.
    // Devices need a real identity and provisioning profile, which this does not provide.
    run(Command::new("codesign").args(["--force", "--sign", "-", "--timestamp=none"]).arg(&app))?;
    eprintln!("iOS app ready: {} ({target})", app.display());
    Ok(())
}
