//! Android APK without Gradle: the NDK builds `libkulisa.so`, SDK build-tools package it
//! into a NativeActivity APK signed with a debug key.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Result, game_pack, option, root, run};

/// Android ABI name and Rust target for each supported architecture.
const ABIS: &[(&str, &str)] = &[
    ("arm64-v8a", "aarch64-linux-android"),
    ("x86_64", "x86_64-linux-android"),
    ("armeabi-v7a", "armv7-linux-androideabi"),
];
const MIN_SDK: u32 = 26;
const TARGET_SDK: u32 = 35;

const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="dev.kulisa.player" android:versionCode="1" android:versionName="0.1.0">
    <uses-feature android:glEsVersion="0x00030000" android:required="true" />
    <uses-permission android:name="android.permission.VIBRATE" />
    <uses-feature android:name="android.hardware.sensor.gyroscope" android:required="false" />
    <application android:label="Kulisa" android:hasCode="false">
        <activity android:name="android.app.NativeActivity"
            android:exported="true"
            android:launchMode="singleTask"
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
"#;

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

fn script(name: &str) -> String {
    if cfg!(windows) { format!("{name}.bat") } else { name.to_string() }
}

fn sdk() -> Result<PathBuf> {
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(p) = env::var_os(var) {
            return Ok(p.into());
        }
    }
    let local = env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Android/Sdk"));
    local.filter(|p| p.exists()).ok_or_else(|| "Android SDK not found; set ANDROID_HOME".into())
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

pub fn apk(args: &[String]) -> Result {
    let release = args.iter().any(|a| a == "--release");
    let abis: Vec<&str> = option(args, "--abi")?.map_or(vec!["arm64-v8a"], |a| a.split(',').collect());
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
    let root = root();
    let profile = if release { "release" } else { "debug" };
    let work = root.join("target/android");
    let staging = work.join("apk");
    let _ = fs::remove_dir_all(&staging);

    for abi in &abis {
        let &(_, target) = ABIS.iter().find(|(a, _)| a == abi).ok_or(format!("unknown ABI {abi}"))?;
        // The NDK names armv7 compilers armv7a-linux-androideabi.
        let prefix = target.replace("armv7-", "armv7a-");
        let clang = |suffix: &str| {
            let name = format!("{prefix}{MIN_SDK}-{suffix}");
            bin.join(if cfg!(windows) { format!("{name}.cmd") } else { name })
        };
        let env_target = target.replace('-', "_");
        let mut cargo = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cargo
            .current_dir(&root)
            .args(["build", "-p", "kulisa-player", "--lib", "--target", target])
            .env(format!("CARGO_TARGET_{}_LINKER", env_target.to_uppercase()), clang("clang"))
            .env(format!("CC_{env_target}"), clang("clang"))
            .env(format!("CXX_{env_target}"), clang("clang++"))
            .env(format!("AR_{env_target}"), bin.join(exe("llvm-ar")));
        if release {
            cargo.arg("--release");
        }
        run(&mut cargo)?;

        let so = root.join("target").join(target).join(profile).join("libkulisa.so");
        let dir = staging.join("lib").join(abi);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::copy(&so, dir.join("libkulisa.so")).map_err(|e| format!("{}: {e}", so.display()))?;
    }

    // The player reads the game from assets/game.kpak through the AssetManager.
    fs::create_dir_all(staging.join("assets")).map_err(|e| e.to_string())?;
    fs::write(staging.join("assets/game.kpak"), game_pack(args)?).map_err(|e| e.to_string())?;

    let tools = newest(&sdk.join("build-tools"))?;
    let platform = newest(&sdk.join("platforms"))?;
    fs::write(work.join("AndroidManifest.xml"), MANIFEST).map_err(|e| e.to_string())?;

    let unsigned = work.join("unsigned.apk");
    let aligned = work.join("aligned.apk");
    run(Command::new(tools.join(exe("aapt2")))
        .arg("link")
        .arg("-o")
        .arg(&unsigned)
        .arg("--manifest")
        .arg(work.join("AndroidManifest.xml"))
        .arg("-I")
        .arg(platform.join("android.jar"))
        .args(["--min-sdk-version", &MIN_SDK.to_string()])
        .args(["--target-sdk-version", &TARGET_SDK.to_string()]))?;
    // Paths inside the APK are relative to the working directory.
    let mut add = Command::new(tools.join(exe("aapt")));
    add.current_dir(&staging).arg("add").arg(&unsigned).arg("assets/game.kpak");
    for abi in &abis {
        add.arg(format!("lib/{abi}/libkulisa.so"));
    }
    run(&mut add)?;
    run(Command::new(tools.join(exe("zipalign")))
        .args(["-f", "-P", "16", "4"])
        .arg(&unsigned)
        .arg(&aligned))?;

    let keystore = debug_keystore(&work)?;
    let out = root.join("dist/android");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let apk = out.join("kulisa.apk");
    run(Command::new(tools.join(script("apksigner")))
        .args(["sign", "--ks"])
        .arg(&keystore)
        .args(["--ks-pass", "pass:android", "--out"])
        .arg(&apk)
        .arg(&aligned))?;
    eprintln!("apk ready: {}", apk.display());
    eprintln!("install with: adb install -r \"{}\"", apk.display());
    Ok(())
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
