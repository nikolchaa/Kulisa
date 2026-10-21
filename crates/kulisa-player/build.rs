//! Windows: embeds the icon `KULISA_ICON` names (an `.ico`, which `kulisa build windows`
//! makes from the game's icon) into the program, so Explorer and shortcuts show it. The
//! resource is compiled with `llvm-rc` or the Windows SDK's `rc` and handed to the linker.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `rc.exe` from the newest Windows 10/11 SDK.
fn sdk_rc() -> Option<PathBuf> {
    let bin = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(bin).ok()?.flatten().map(|e| e.path()).collect();
    versions.sort();
    versions.into_iter().rev().map(|v| v.join("x64").join("rc.exe")).find(|p| p.exists())
}

fn main() {
    println!("cargo:rerun-if-env-changed=KULISA_ICON");
    let Some(ico) = env::var_os("KULISA_ICON").filter(|v| !v.is_empty()) else { return };
    let ico = PathBuf::from(ico);
    println!("cargo:rerun-if-changed={}", ico.display());
    let windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if !windows || env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let rc = out.join("icon.rc");
    let res = out.join("icon.res");
    let path = ico.display().to_string().replace('\\', "/");
    std::fs::write(&rc, format!("1 ICON \"{path}\"\n")).expect("writing icon.rc");
    let compilers = [PathBuf::from("llvm-rc"), PathBuf::from("rc")].into_iter().chain(sdk_rc());
    for compiler in compilers {
        let ok = Command::new(&compiler).arg("/fo").arg(&res).arg(&rc).status().is_ok_and(|s| s.success());
        if ok {
            println!("cargo:rustc-link-arg-bins={}", res.display());
            return;
        }
    }
    println!(
        "cargo:warning=no resource compiler (llvm-rc or rc.exe) found; the program keeps the default icon"
    );
}
