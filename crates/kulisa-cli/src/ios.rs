//! iOS app bundle without an Xcode project: the `kulisa-player` binary (ready-made or built with
//! cargo) goes into a `.app` next to an Info.plist made from `kulisa.toml`, a compiled asset catalog and a
//! privacy manifest. Signed for the App Store and zipped as an `.ipa` with `[ios] identity` and
//! `provisioning_profile`; ad-hoc signed otherwise, which is enough for the simulator. macOS
//! hosts only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Engine, Game, Options, Result, escape, icon, players, run, write};

/// Keep in sync with IPHONEOS_DEPLOYMENT_TARGET in .cargo/config.toml.
const MIN_IOS: &str = "15.0";
const EXECUTABLE: &str = "kulisa-player";

/// A single 1024 px icon: actool makes the sizes each device needs.
const ICON_SET: &str = r#"{"images":[{"filename":"icon.png","idiom":"universal","platform":"ios","size":"1024x1024"}],"info":{"author":"kulisa","version":1}}"#;
const CATALOG: &str = r#"{"info":{"author":"kulisa","version":1}}"#;

/// The privacy manifest: no tracking, no data collected, and why the engine's required-reason
/// APIs are used (reading its own files' metadata, measuring elapsed time).
const PRIVACY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>NSPrivacyTracking</key><false/>
    <key>NSPrivacyTrackingDomains</key><array/>
    <key>NSPrivacyCollectedDataTypes</key><array/>
    <key>NSPrivacyAccessedAPITypes</key>
    <array>
        <dict>
            <key>NSPrivacyAccessedAPIType</key><string>NSPrivacyAccessedAPICategoryFileTimestamp</string>
            <key>NSPrivacyAccessedAPITypeReasons</key><array><string>C617.1</string></array>
        </dict>
        <dict>
            <key>NSPrivacyAccessedAPIType</key><string>NSPrivacyAccessedAPICategorySystemBootTime</string>
            <key>NSPrivacyAccessedAPITypeReasons</key><array><string>35F9.1</string></array>
        </dict>
    </array>
</dict>
</plist>
"#;

/// The SDK and tools a build used, which the App Store checks in the plist.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolchain {
    /// `iphoneos` or `iphonesimulator`.
    pub sdk: String,
    pub sdk_version: String,
    pub sdk_build: String,
    /// `2600` for Xcode 26.0.
    pub xcode: String,
    pub xcode_build: String,
    pub machine_build: String,
}

/// `(DTXcode, DTXcodeBuild)` from `xcodebuild -version`.
fn parse_xcode_version(text: &str) -> (String, String) {
    let mut version = String::new();
    let mut build = String::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Xcode ") {
            let mut parts = v.trim().split('.').map(|p| p.parse::<u32>().unwrap_or(0));
            let (major, minor, patch) =
                (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
            version = format!("{:04}", major * 100 + minor * 10 + patch);
        } else if let Some(b) = line.strip_prefix("Build version ") {
            build = b.trim().to_string();
        }
    }
    (version, build)
}

/// The standard output of a command, trimmed.
fn output(cmd: &mut Command) -> Result<String> {
    let o = cmd.output().map_err(|e| format!("failed to run {:?}: {e}", cmd.get_program()))?;
    if !o.status.success() {
        return Err(format!("{:?} failed: {}", cmd.get_program(), String::from_utf8_lossy(&o.stderr)));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn toolchain(sdk: &str) -> Result<Toolchain> {
    let (xcode, xcode_build) = parse_xcode_version(&output(Command::new("xcodebuild").arg("-version"))?);
    let xcrun = |flag: &str| output(Command::new("xcrun").args(["--sdk", sdk, flag]));
    Ok(Toolchain {
        sdk: sdk.to_string(),
        sdk_version: xcrun("--show-sdk-version")?,
        sdk_build: xcrun("--show-sdk-build-version")?,
        xcode,
        xcode_build,
        machine_build: output(Command::new("sw_vers").arg("-buildVersion"))?,
    })
}

pub fn info_plist(game: &Game, platform: &str, tools: &Toolchain) -> String {
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
    <key>BuildMachineOSBuild</key><string>{machine_build}</string>
    <key>DTCompiler</key><string>com.apple.compilers.llvm.clang.1_0</string>
    <key>DTPlatformBuild</key><string>{sdk_build}</string>
    <key>DTPlatformName</key><string>{sdk}</string>
    <key>DTPlatformVersion</key><string>{sdk_version}</string>
    <key>DTSDKBuild</key><string>{sdk_build}</string>
    <key>DTSDKName</key><string>{sdk}{sdk_version}</string>
    <key>DTXcode</key><string>{xcode}</string>
    <key>DTXcodeBuild</key><string>{xcode_build}</string>
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
        machine_build = escape(&tools.machine_build),
        sdk = escape(&tools.sdk),
        sdk_version = escape(&tools.sdk_version),
        sdk_build = escape(&tools.sdk_build),
        xcode = escape(&tools.xcode),
        xcode_build = escape(&tools.xcode_build),
    )
}

/// Compiles the game's icon into `app` (`Assets.car` and the older icon files) and merges the
/// plist keys actool asks for into its Info.plist.
fn compile_icon(game: &Game, img: &image::DynamicImage, sdk: &str, work: &Path, app: &Path) -> Result {
    let catalog = work.join("Assets.xcassets");
    let _ = fs::remove_dir_all(&catalog);
    write(&catalog.join("Contents.json"), CATALOG)?;
    write(&catalog.join("AppIcon.appiconset/Contents.json"), ICON_SET)?;
    write(
        &catalog.join("AppIcon.appiconset/icon.png"),
        icon::png_opaque(img, 1024, &game.config.background),
    )?;
    let partial = work.join("icon-keys.plist");
    run(Command::new("xcrun")
        .args(["actool", "--compile"])
        .arg(app)
        .args(["--platform", sdk, "--minimum-deployment-target", MIN_IOS])
        .args(["--app-icon", "AppIcon", "--target-device", "iphone", "--target-device", "ipad"])
        .arg("--output-partial-info-plist")
        .arg(&partial)
        .arg(&catalog))?;
    run(Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg(format!("Merge {}", partial.display()))
        .arg(app.join("Info.plist")))
}

/// The App Store signing setup: `[ios] identity` and its provisioning profile.
fn store_signing(game: &Game) -> Result<Option<(String, PathBuf)>> {
    let c = &game.config;
    match (&c.ios_identity, &c.ios_provisioning_profile) {
        (None, None) => Ok(None),
        (Some(identity), Some(profile)) => {
            let profile = game.dir.join(profile);
            if !profile.is_file() {
                return Err(format!("the provisioning profile {} does not exist", profile.display()));
            }
            Ok(Some((identity.clone(), profile)))
        }
        _ => Err("kulisa.toml: `ios.identity` and `ios.provisioning_profile` go together".into()),
    }
}

/// Signs `app` for devices with the profile's entitlements, and zips it as `app`'s `.ipa`.
fn sign_for_store(app: &Path, identity: &str, profile: &Path, work: &Path) -> Result {
    fs::copy(profile, app.join("embedded.mobileprovision"))
        .map_err(|e| format!("{}: {e}", profile.display()))?;
    let decoded = work.join("profile.plist");
    run(Command::new("security").args(["cms", "-D", "-i"]).arg(profile).arg("-o").arg(&decoded))?;
    let entitlements = work.join("entitlements.plist");
    let xml = output(
        Command::new("/usr/libexec/PlistBuddy").args(["-x", "-c", "Print :Entitlements"]).arg(&decoded),
    )?;
    write(&entitlements, xml)?;
    run(Command::new("codesign")
        .args(["--force", "--timestamp", "--sign", identity, "--entitlements"])
        .arg(&entitlements)
        .arg(app))?;

    // An .ipa is a zip with the app in a Payload folder.
    let payload = work.join("Payload");
    let _ = fs::remove_dir_all(&payload);
    let inside = payload.join(app.file_name().ok_or("the app has no name")?);
    run(Command::new("ditto").arg(app).arg(&inside))?;
    let ipa = app.with_extension("ipa");
    run(Command::new("ditto").args(["-c", "-k", "--keepParent"]).arg(&payload).arg(&ipa))?;
    eprintln!("App Store package ready: {}", ipa.display());
    Ok(())
}

/// The player for devices or the simulator.
pub fn player(opts: &Options, sim: bool) -> Result<PathBuf> {
    match &opts.engine {
        Engine::Released(tool) => {
            let file = players::files("ios")[sim as usize];
            Ok(players::released(tool, "ios")?.join(file))
        }
        Engine::Source(root) => {
            let target = if sim { "aarch64-apple-ios-sim" } else { "aarch64-apple-ios" };
            run(&mut opts.cargo_player(root, &["--bin", EXECUTABLE, "--target", target]))?;
            Ok(root.join("target").join(target).join(opts.profile_dir()).join(EXECUTABLE))
        }
    }
}

/// Builds `game` into the app bundle `app` (`Name.app`), for the simulator when `sim`.
pub fn build(game: &Game, opts: &Options, sim: bool, app: &Path) -> Result {
    let (platform, sdk) = if sim { ("iPhoneSimulator", "iphonesimulator") } else { ("iPhoneOS", "iphoneos") };
    let store = if sim { None } else { store_signing(game)? };
    let tools = toolchain(sdk)?;
    let bin = player(opts, sim)?;

    let work = opts.work_dir("ios-package")?.join(&game.config.id);
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(app);
    fs::create_dir_all(app).map_err(|e| e.to_string())?;
    fs::copy(&bin, app.join(EXECUTABLE)).map_err(|e| format!("{}: {e}", bin.display()))?;
    write(&app.join("Info.plist"), info_plist(game, platform, &tools))?;
    write(&app.join("PrivacyInfo.xcprivacy"), PRIVACY)?;
    match icon::load(game)? {
        Some(img) => compile_icon(game, &img, sdk, &work, app)?,
        None if store.is_some() => {
            return Err("the App Store needs an app icon: add icon.png to the game".into());
        }
        None => {}
    }
    // The player looks for game.kpak next to its executable.
    write(&app.join("game.kpak"), game.pack()?)?;
    crate::video::copy(game, app)?;

    match &store {
        Some((identity, profile)) => sign_for_store(app, identity, profile, &work)?,
        // The simulator refuses unsigned arm64 apps; an ad-hoc signature is enough there.
        None => {
            if opts.shipping() && !sim {
                eprintln!("warning: ad-hoc signed; devices refuse it (see `[ios] identity`)");
            }
            run(Command::new("codesign").args(["--force", "--sign", "-", "--timestamp=none"]).arg(app))?
        }
    }
    eprintln!("iOS app ready: {} ({sdk})", app.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kulisa_platform::GameConfig;

    fn game(toml: &str) -> Game {
        Game { dir: PathBuf::new(), config: GameConfig::parse(Some(toml)).unwrap() }
    }

    #[test]
    fn plist_follows_the_config() {
        let game = game(
            "[game]\nid = \"cats\"\ntitle = \"Cats\"\nversion = \"2.0\"\n[ios]\nbundle_id = \"com.example.cats\"",
        );
        let tools = Toolchain {
            sdk: "iphoneos".into(),
            sdk_version: "26.0".into(),
            xcode: "2600".into(),
            ..Default::default()
        };
        let p = info_plist(&game, "iPhoneOS", &tools);
        assert!(p.contains("<key>CFBundleIdentifier</key><string>com.example.cats</string>"));
        assert!(p.contains("<key>CFBundleShortVersionString</key><string>2.0</string>"));
        assert!(p.contains("UIInterfaceOrientationLandscapeLeft") && !p.contains("OrientationPortrait"));
        assert!(p.contains("<key>DTSDKName</key><string>iphoneos26.0</string>"));
        assert!(p.contains("<key>DTXcode</key><string>2600</string>"));
    }

    #[test]
    fn xcode_versions() {
        let parse = |t: &str| parse_xcode_version(t);
        assert_eq!(parse("Xcode 26.0\nBuild version 17A324\n"), ("2600".into(), "17A324".into()));
        assert_eq!(parse("Xcode 26.1.1\nBuild version 17B100\n"), ("2611".into(), "17B100".into()));
        assert_eq!(parse("Xcode 16.4\nBuild version 16F6\n").0, "1640");
    }

    #[test]
    fn store_signing_needs_both_settings() {
        assert_eq!(store_signing(&game("[game]\nid = \"a\"")).unwrap(), None);
        let e = store_signing(&game("[ios]\nidentity = \"Apple Distribution\"")).unwrap_err();
        assert!(e.contains("go together"), "{e}");
        let e =
            store_signing(&game("[ios]\nidentity = \"x\"\nprovisioning_profile = \"no.mobileprovision\""))
                .unwrap_err();
        assert!(e.contains("does not exist"), "{e}");
    }
}
