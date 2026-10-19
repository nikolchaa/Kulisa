//! Per-target entry points. Desktop and iOS use `main.rs`; Android and web load this library.

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: kulisa_runtime::AndroidApp) {
    kulisa_runtime::run_android(app);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn web_start() {
    kulisa_runtime::run();
}
