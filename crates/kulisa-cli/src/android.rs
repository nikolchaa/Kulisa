//! Android APK without Gradle: the NDK builds `libkulisa.so`, SDK build-tools package it
//! into a NativeActivity APK (manifest from `kulisa.toml`) signed with a debug key.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Game, Options, Result, escape, icon, run, write};

/// Android ABI name and Rust target for each supported architecture.
pub const ABIS: &[(&str, &str)] = &[
    ("arm64-v8a", "aarch64-linux-android"),
    ("x86_64", "x86_64-linux-android"),
    ("armeabi-v7a", "armv7-linux-androideabi"),
];
const MIN_SDK: u32 = 26;
const TARGET_SDK: u32 = 35;

/// The adaptive launcher icon: the game's art fills the shape, nothing on top.
const ADAPTIVE_ICON: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@mipmap/icon_art" />
    <foreground android:drawable="@android:color/transparent" />
</adaptive-icon>
"#;

/// The manifest: the game's package, name, version, orientation and (`icon`) launcher icon.
pub fn manifest(game: &Game, icon: bool) -> String {
    let c = &game.config;
    // The sensor variants still turn the right way up; the runtime locks the same way.
    let orientation = match c.locked_orientation() {
        "landscape" => "sensorLandscape",
        "portrait" => "sensorPortrait",
        _ => "fullUser",
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="{package}" android:versionCode="{build}" android:versionName="{version}">
    <uses-feature android:glEsVersion="0x00030000" android:required="true" />
    <uses-permission android:name="android.permission.VIBRATE" />
    <uses-feature android:name="android.hardware.sensor.gyroscope" android:required="false" />
    <application android:label="{label}"{icon} android:hasCode="false">
        <activity android:name="android.app.NativeActivity"
            android:exported="true"
            android:launchMode="singleTask"
            android:screenOrientation="{orientation}"
            android:theme="@android:style/Theme.Black.NoTitleBar.Fullscreen"
            android:configChanges="orientation|screenSize|screenLayout|smallestScreenSize|keyboard|keyboardHidden|navigation|uiMode|density">
            <meta-data android:name="android.app.lib_name" android:value="kulisa" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#,
        package = escape(&c.android_package),
        build = c.build,
        version = escape(&c.version),
        label = escape(&c.title),
        icon = if icon { " android:icon=\"@mipmap/icon\"" } else { "" },
    )
}

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

fn script(name: &str) -> String {
    if cfg!(windows) { format!("{name}.bat") } else { name.to_string() }
}

pub fn sdk() -> Result<PathBuf> {
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(p) = env::var_os(var) {
            return Ok(p.into());
        }
    }
    let local = env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Android/Sdk"));
    local.filter(|p| p.exists()).ok_or_else(|| "Android SDK not found; set ANDROID_HOME".into())
}

/// `adb` from the SDK (or PATH).
pub fn adb() -> PathBuf {
    sdk()
        .map(|s| s.join("platform-tools").join(exe("adb")))
        .ok()
        .filter(|p| p.exists())
        .unwrap_or(exe("adb").into())
}

/// Highest-versioned subdirectory of `dir`, e.g. the newest build-tools.
fn newest(dir: &Path) -> Result<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    let key = |p: &PathBuf| -> Vec<u32> {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        name.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect()
    };
    entries.sort_by_key(key);
    entries.pop().ok_or_else(|| format!("nothing in {}", dir.display()))
}

fn ndk(sdk: &Path) -> Result<PathBuf> {
    for var in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "ANDROID_NDK_LATEST_HOME"] {
        if let Some(p) = env::var_os(var) {
            return Ok(p.into());
        }
    }
    newest(&sdk.join("ndk"))
        .map_err(|_| "Android NDK not found; install it with sdkmanager or set ANDROID_NDK_HOME".into())
}

/// Builds `game` as an APK at `apk` for the given ABIs (e.g. `arm64-v8a`, `x86_64` for the
/// emulator).
pub fn build(game: &Game, opts: &Options, abis: &[&str], apk: &Path) -> Result {
    let sdk = sdk()?;
    let ndk = ndk(&sdk)?;
    let host = if cfg!(windows) {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        "linux-x86_64"
    };
    let bin = ndk.join("toolchains/llvm/prebuilt").join(host).join("bin");
    let work = opts.engine.join("target").join("android");
    let staging = work.join("apk");
    let _ = fs::remove_dir_all(&staging);

    for abi in abis {
        let &(_, target) = ABIS.iter().find(|(a, _)| a == abi).ok_or(format!("unknown ABI {abi}"))?;
        // The NDK names armv7 compilers armv7a-linux-androideabi.
        let prefix = target.replace("armv7-", "armv7a-");
        let clang = |suffix: &str| {
            let name = format!("{prefix}{MIN_SDK}-{suffix}");
            bin.join(if cfg!(windows) { format!("{name}.cmd") } else { name })
        };
        let env_target = target.replace('-', "_");
        run(opts
            .cargo_player(&["--lib", "--target", target])
            .env(format!("CARGO_TARGET_{}_LINKER", env_target.to_uppercase()), clang("clang"))
            .env(format!("CC_{env_target}"), clang("clang"))
            .env(format!("CXX_{env_target}"), clang("clang++"))
            .env(format!("AR_{env_target}"), bin.join(exe("llvm-ar"))))?;

        let so = opts.engine.join("target").join(target).join(opts.profile()).join("libkulisa.so");
        let dir = staging.join("lib").join(abi);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::copy(&so, dir.join("libkulisa.so")).map_err(|e| format!("{}: {e}", so.display()))?;
    }

    // The player reads the game from assets/game.kpak through the AssetManager.
    write(&staging.join("assets/game.kpak"), game.pack()?)?;

    let tools = newest(&sdk.join("build-tools"))?;
    let platform = newest(&sdk.join("platforms"))?;

    // The launcher icon. An adaptive icon with the art as its full-bleed background layer
    // (432 px, the xxxhdpi size of 108 dp) fills the launcher's shape; the 192 px icon is
    // for launchers that do not use adaptive icons.
    let compiled = work.join("res-compiled");
    let _ = fs::remove_dir_all(&compiled);
    let _ = fs::remove_dir_all(work.join("res"));
    let icon = icon::load(game)?;
    if let Some(img) = &icon {
        // aapt2 reads the resource type from the path, which must use one kind of separator.
        let res = work.join("res");
        let files = [
            (res.join("mipmap").join("icon.png"), icon::png(img, 192)),
            (res.join("mipmap").join("icon_art.png"), icon::png(img, 432)),
            (res.join("mipmap-anydpi-v26").join("icon.xml"), ADAPTIVE_ICON.as_bytes().to_vec()),
        ];
        fs::create_dir_all(&compiled).map_err(|e| e.to_string())?;
        for (path, bytes) in files {
            write(&path, bytes)?;
            run(Command::new(tools.join(exe("aapt2"))).arg("compile").arg("-o").arg(&compiled).arg(&path))?;
        }
    }
    write(&work.join("AndroidManifest.xml"), manifest(game, icon.is_some()))?;

    let unsigned = work.join("unsigned.apk");
    let aligned = work.join("aligned.apk");
    let mut link = Command::new(tools.join(exe("aapt2")));
    link.arg("link")
        .arg("-o")
        .arg(&unsigned)
        .arg("--manifest")
        .arg(work.join("AndroidManifest.xml"))
        .arg("-I")
        .arg(platform.join("android.jar"))
        .args(["--min-sdk-version", &MIN_SDK.to_string()])
        .args(["--target-sdk-version", &TARGET_SDK.to_string()]);
    for flat in fs::read_dir(&compiled).into_iter().flatten().flatten() {
        link.arg(flat.path());
    }
    run(&mut link)?;
    // Paths inside the APK are relative to the working directory.
    let mut add = Command::new(tools.join(exe("aapt")));
    add.current_dir(&staging).arg("add").arg(&unsigned).arg("assets/game.kpak");
    for abi in abis {
        add.arg(format!("lib/{abi}/libkulisa.so"));
    }
    run(&mut add)?;
    run(Command::new(tools.join(exe("zipalign")))
        .args(["-f", "-P", "16", "4"])
        .arg(&unsigned)
        .arg(&aligned))?;

    let keystore = debug_keystore(&work)?;
    if let Some(dir) = apk.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    run(Command::new(tools.join(script("apksigner")))
        .args(["sign", "--ks"])
        .arg(&keystore)
        .args(["--ks-pass", "pass:android", "--out"])
        .arg(apk)
        .arg(&aligned))?;
    eprintln!("apk ready: {}", apk.display());
    Ok(())
}

/// Installs `apk` on the connected device or emulator and starts it.
pub fn install_and_start(game: &Game, apk: &Path) -> Result {
    let adb = adb();
    run(Command::new(&adb).arg("install").arg("-r").arg(apk))?;
    let activity = format!("{}/android.app.NativeActivity", game.config.android_package);
    run(Command::new(&adb).args(["shell", "am", "start", "-n", &activity]))
}

/// The conventional Android debug keystore (public password "android"), created on first use.
fn debug_keystore(work: &Path) -> Result<PathBuf> {
    let home = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")).map(PathBuf::from);
    if let Some(ks) = home.map(|h| h.join(".android/debug.keystore")).filter(|p| p.exists()) {
        return Ok(ks);
    }
    let ks = work.join("debug.keystore");
    if !ks.exists() {
        let keytool = env::var_os("JAVA_HOME")
            .map(|j| PathBuf::from(j).join("bin").join(exe("keytool")))
            .unwrap_or_else(|| exe("keytool").into());
        run(Command::new(keytool)
            .args(["-genkeypair", "-keystore"])
            .arg(&ks)
            .args(["-storepass", "android", "-keypass", "android", "-alias", "androiddebugkey"])
            .args(["-keyalg", "RSA", "-keysize", "2048", "-validity", "10000"])
            .args(["-dname", "CN=Android Debug,O=Android,C=US"]))?;
    }
    Ok(ks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kulisa_platform::GameConfig;

    #[test]
    fn manifest_follows_the_config() {
        let toml = "[game]\nid = \"cat-tales\"\ntitle = \"Cats & Co\"\nversion = \"1.2\"\nbuild = 7\n\
                    [display]\nwidth = 1080\nheight = 1920";
        let game = Game { dir: PathBuf::new(), config: GameConfig::parse(Some(toml)).unwrap() };
        let m = manifest(&game, true);
        assert!(
            m.contains(r#"package="dev.kulisa.cat_tales" android:versionCode="7" android:versionName="1.2""#)
        );
        assert!(m.contains(r#"android:label="Cats &amp; Co" android:icon="@mipmap/icon""#));
        assert!(!manifest(&game, false).contains("android:icon"));
        assert!(m.contains(r#"android:screenOrientation="sensorPortrait""#));
    }
}
