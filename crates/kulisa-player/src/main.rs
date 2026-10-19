#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
fn main() {
    kulisa_runtime::run();
}

// Android and web enter through the library; keep a stub so the bin target still builds.
#[cfg(any(target_os = "android", target_arch = "wasm32"))]
fn main() {}
