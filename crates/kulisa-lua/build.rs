//! On `wasm32-unknown-unknown`, Luau's C++ needs libc, libc++abi, libunwind and the compiler
//! runtime from a wasi sysroot. `cargo xtask web` downloads one and sets `WASI_SYSROOT`.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=WASI_SYSROOT");
    println!("cargo:rerun-if-env-changed=WASI_CLANG_RT");

    if env::var("TARGET").as_deref() != Ok("wasm32-unknown-unknown") {
        return;
    }

    let sysroot = PathBuf::from(env::var("WASI_SYSROOT").expect(
        "WASI_SYSROOT is not set. Build the web target with `cargo xtask web`, which provides it.",
    ));
    let lib = sysroot.join("lib").join("wasm32-wasip1");
    println!("cargo:rustc-link-search=native={}", lib.join("eh").display());
    println!("cargo:rustc-link-search=native={}", lib.display());
    if let Ok(rt) = env::var("WASI_CLANG_RT") {
        println!("cargo:rustc-link-search=native={rt}");
    }
    for name in ["c++abi", "unwind", "c", "clang_rt.builtins"] {
        println!("cargo:rustc-link-lib=static={name}");
    }
}
