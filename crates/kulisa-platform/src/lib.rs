//! Platform services. This crate is a leaf: it must not depend on other engine crates.

mod assets;
mod config;
mod device;
mod storage;
#[cfg(target_arch = "wasm32")]
mod web_motion;
#[cfg(target_arch = "wasm32")]
mod web_text;

#[cfg(not(target_arch = "wasm32"))]
pub use assets::build_pack;
#[cfg(target_arch = "wasm32")]
pub use assets::fetch;
pub use assets::{Assets, Pack, is_game_file, normalize};
pub use config::{CONFIG_FILE, Config, ConfigValue, GameConfig, ORIENTATIONS, QUALITY_TIERS, valid_id};
pub use device::{Motion, dev_override, profile_override, quality_override, safe_area_override};
#[cfg(target_arch = "wasm32")]
pub use device::{web_safe_area, web_touch};
pub use storage::Storage;
#[cfg(not(target_arch = "wasm32"))]
pub use storage::data_dir;
#[cfg(target_os = "android")]
pub use storage::set_android_data_dir;
#[cfg(target_arch = "wasm32")]
pub use web_motion::{web_motion, web_motion_start, web_motion_stop, web_vibrate};
#[cfg(target_arch = "wasm32")]
pub use web_text::{TextEvent, web_keyboard_inset, web_text_events, web_text_focus};

/// Installs the platform logger once. Safe to call more than once.
pub fn init_logging() {
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    {
        let env = env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn");
        let _ = env_logger::Builder::from_env(env).try_init();
    }

    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("kulisa"),
    );

    #[cfg(target_os = "ios")]
    {
        let _ = oslog::OsLogger::new("dev.kulisa.player").level_filter(log::LevelFilter::Info).init();
    }

    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Info);
    }
}
