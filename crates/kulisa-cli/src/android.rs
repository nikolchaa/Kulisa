//! Android without Gradle: the NDK builds `libkulisa.so`, SDK build-tools package it into a
//! NativeActivity APK (manifest from `kulisa.toml`), or bundletool into an App Bundle for
//! Google Play. Signed with the game's release key (`[android] keystore`), or for APKs a debug
//! key when it has none.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Game, Options, Result, escape, icon, run, write};

/// The keystore password. Tools read it from the environment by name, so it never appears on
/// a command line or in the build log.
const STORE_PASSWORD: &str = "KULISA_KEYSTORE_PASSWORD";
/// The key's own password, when it differs from the keystore's.
const KEY_PASSWORD: &str = "KULISA_KEY_PASSWORD";
const BUNDLETOOL: &str = "1.18.3";
/// Native libraries stay uncompressed and aligned for devices with 16 KB memory pages.
const BUNDLE_CONFIG: &str =
    r#"{"optimizations":{"uncompressNativeLibraries":{"enabled":true,"alignment":"PAGE_ALIGNMENT_16K"}}}"#;

/// Android ABI name and Rust target for each supported architecture.
pub const ABIS: &[(&str, &str)] = &[
    ("arm64-v8a", "aarch64-linux-android"),
    ("x86_64", "x86_64-linux-android"),
    ("armeabi-v7a", "armv7-linux-androideabi"),
];
const MIN_SDK: u32 = 26;
/// Google Play's minimum for new apps and updates since August 2026.
const TARGET_SDK: u32 = 36;

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
    // API 36 ignores orientation locks on tablets except for games (`appCategory`), and stops
    // sending the back key unless `enableOnBackInvokedCallback` is off.
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
    <queries>
        <intent><action android:name="android.intent.action.TTS_SERVICE" /></intent>
    </queries>
    <application android:label="{label}"{icon} android:hasCode="false"
        android:appCategory="game" android:enableOnBackInvokedCallback="false">
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

/// A program from the JDK: `JAVA_HOME`, or PATH.
fn jdk_tool(name: &str) -> PathBuf {
    env::var_os("JAVA_HOME")
        .map(|j| PathBuf::from(j).join("bin").join(exe(name)))
        .unwrap_or_else(|| exe(name).into())
}

/// The game's release key.
struct ReleaseKey {
    keystore: PathBuf,
    alias: String,
    /// The environment variable that holds the key's password.
    key_password: &'static str,
}

/// The key `[android] keystore` names, if any, with its password checked before a long build.
fn release_key(game: &Game) -> Result<Option<ReleaseKey>> {
    let c = &game.config;
    let Some(keystore) = &c.android_keystore else { return Ok(None) };
    let keystore = game.dir.join(keystore);
    if !keystore.is_file() {
        return Err(format!("the keystore {} does not exist", keystore.display()));
    }
    let alias =
        c.android_key_alias.clone().ok_or("kulisa.toml: `android.keystore` needs `android.key_alias`")?;
    if env::var_os(STORE_PASSWORD).is_none() {
        return Err(format!(
            "set {STORE_PASSWORD} to the keystore's password ({KEY_PASSWORD} too if the key has its own)"
        ));
    }
    let key_password = if env::var_os(KEY_PASSWORD).is_some() { KEY_PASSWORD } else { STORE_PASSWORD };
    Ok(Some(ReleaseKey { keystore, alias, key_password }))
}

/// bundletool, downloaded into the engine's `.tools` on first use.
fn bundletool(engine: &Path) -> Result<PathBuf> {
    let jar = engine.join(".tools").join(format!("bundletool-all-{BUNDLETOOL}.jar"));
    if !jar.is_file() {
        let url = format!(
            "https://github.com/google/bundletool/releases/download/{BUNDLETOOL}/bundletool-all-{BUNDLETOOL}.jar"
        );
        let part = jar.with_extension("part");
        fs::create_dir_all(engine.join(".tools")).map_err(|e| e.to_string())?;
        run(Command::new("curl").args(["-fsSL", "-o"]).arg(&part).arg(url))?;
        fs::rename(&part, &jar).map_err(|e| e.to_string())?;
    }
    Ok(jar)
}

/// What the APK and the App Bundle share: the libraries and the game in `staging`, the compiled
/// resources in `compiled`.
struct Parts {
    work: PathBuf,
    staging: PathBuf,
    compiled: PathBuf,
    tools: PathBuf,
    platform: PathBuf,
}

impl Parts {
    /// `aapt2 link` with the manifest, the platform and the compiled resources.
    fn link(&self, out: &Path) -> Command {
        let mut link = Command::new(self.tools.join(exe("aapt2")));
        link.arg("link")
            .arg("-o")
            .arg(out)
            .arg("--manifest")
            .arg(self.work.join("AndroidManifest.xml"))
            .arg("-I")
            .arg(self.platform.join("android.jar"))
            .args(["--min-sdk-version", &MIN_SDK.to_string()])
            .args(["--target-sdk-version", &TARGET_SDK.to_string()]);
        for flat in fs::read_dir(&self.compiled).into_iter().flatten().flatten() {
            link.arg(flat.path());
        }
        link
    }
}

/// Builds the player for `abis` and the game's resources.
fn parts(game: &Game, opts: &Options, abis: &[&str]) -> Result<Parts> {
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
    Ok(Parts { work, staging, compiled, tools, platform })
}

/// Builds `game` as an APK at `apk` for the given ABIs (e.g. `arm64-v8a`, `x86_64` for the
/// emulator).
pub fn build(game: &Game, opts: &Options, abis: &[&str], apk: &Path) -> Result {
    let key = release_key(game)?;
    let p = parts(game, opts, abis)?;
    let unsigned = p.work.join("unsigned.apk");
    let aligned = p.work.join("aligned.apk");
    run(&mut p.link(&unsigned))?;
    // Paths inside the APK are relative to the working directory.
    let mut add = Command::new(p.tools.join(exe("aapt")));
    add.current_dir(&p.staging).arg("add").arg(&unsigned).arg("assets/game.kpak");
    for abi in abis {
        add.arg(format!("lib/{abi}/libkulisa.so"));
    }
    run(&mut add)?;
    run(Command::new(p.tools.join(exe("zipalign")))
        .args(["-f", "-P", "16", "4"])
        .arg(&unsigned)
        .arg(&aligned))?;

    if let Some(dir) = apk.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut sign = Command::new(p.tools.join(script("apksigner")));
    sign.args(["sign", "--ks"]);
    match &key {
        Some(key) => {
            sign.arg(&key.keystore)
                .args(["--ks-key-alias", &key.alias])
                .args(["--ks-pass", &format!("env:{STORE_PASSWORD}")])
                .args(["--key-pass", &format!("env:{}", key.key_password)]);
        }
        None => {
            if opts.release {
                eprintln!("warning: signed with the debug key; stores refuse it (see `[android] keystore`)");
            }
            sign.arg(debug_keystore(&p.work)?).args(["--ks-pass", "pass:android"]);
        }
    }
    run(sign.arg("--out").arg(apk).arg(&aligned))?;
    eprintln!("apk ready: {}", apk.display());
    Ok(())
}

/// Builds `game` as a signed Android App Bundle at `aab`, what Google Play takes.
pub fn bundle(game: &Game, opts: &Options, abis: &[&str], aab: &Path) -> Result {
    let key = release_key(game)?.ok_or(
        "an App Bundle needs a release key: set `keystore` and `key_alias` under [android] in kulisa.toml",
    )?;
    let tool = bundletool(&opts.engine)?;
    let p = parts(game, opts, abis)?;

    // A bundle module is a zip of the proto-format app with the manifest in its own folder.
    let proto = p.work.join("proto.apk");
    run(p.link(&proto).arg("--proto-format"))?;
    let base = p.work.join("base");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("manifest")).map_err(|e| e.to_string())?;
    run(Command::new(jdk_tool("jar")).current_dir(&base).arg("xf").arg(&proto))?;
    fs::rename(base.join("AndroidManifest.xml"), base.join("manifest/AndroidManifest.xml"))
        .map_err(|e| e.to_string())?;
    crate::copy_tree(&p.staging, &base)?;
    let module = p.work.join("base.zip");
    run(Command::new(jdk_tool("jar")).arg("cMf").arg(&module).arg("-C").arg(&base).arg("."))?;
    let config = p.work.join("BundleConfig.json");
    write(&config, BUNDLE_CONFIG)?;

    let unsigned = p.work.join("unsigned.aab");
    let _ = fs::remove_file(&unsigned);
    run(Command::new(jdk_tool("java"))
        .arg("-jar")
        .arg(&tool)
        .arg("build-bundle")
        .arg(format!("--modules={}", module.display()))
        .arg(format!("--config={}", config.display()))
        .arg(format!("--output={}", unsigned.display())))?;

    if let Some(dir) = aab.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    run(Command::new(jdk_tool("jarsigner"))
        .arg("-keystore")
        .arg(&key.keystore)
        .args(["-storepass:env", STORE_PASSWORD, "-keypass:env", key.key_password])
        .arg("-signedjar")
        .arg(aab)
        .arg(&unsigned)
        .arg(&key.alias))?;
    eprintln!("app bundle ready: {}", aab.display());
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
        run(Command::new(jdk_tool("keytool"))
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
        assert!(m.contains(r#"android:appCategory="game""#));
        assert!(m.contains(r#"android:enableOnBackInvokedCallback="false""#));
    }

    #[test]
    fn release_keys_are_checked_early() {
        let game = |toml: &str| Game { dir: PathBuf::new(), config: GameConfig::parse(Some(toml)).unwrap() };
        assert!(release_key(&game("[game]\nid = \"a\"")).unwrap().is_none());
        let e = release_key(&game("[android]\nkeystore = \"missing.jks\"")).err().unwrap();
        assert!(e.contains("does not exist"), "{e}");
    }
}
