//! Platform services. This crate is a leaf: it must not depend on other engine crates.

/// Installs the platform logger once. Safe to call more than once.
pub fn init_logging() {
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    {
        let env = env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn");
        let _ = env_logger::Builder::from_env(env).try_init();
    }

    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("kulisa"),
    );

    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Info);
    }
}
