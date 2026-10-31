//! Steam, through its flat C API. The library (`steam_api64.dll`, `libsteam_api.so`,
//! `libsteam_api.dylib`) comes from the game developer's Steamworks SDK and sits next to the
//! program; it is loaded at startup, so games without it need nothing and the engine ships none
//! of Valve's files.

/// What the engine asks of Steam (a seam so scripts can be tested without a Steam client).
pub trait SteamApi {
    /// Called every frame.
    fn run_callbacks(&mut self);
    fn unlock(&mut self, achievement: &str) -> bool;
    fn clear(&mut self, achievement: &str) -> bool;
    fn unlocked(&self, achievement: &str) -> Option<bool>;
    fn progress(&mut self, achievement: &str, current: u32, max: u32) -> bool;
    fn stat(&self, name: &str) -> Option<f64>;
    fn set_stat(&mut self, name: &str, value: f64) -> bool;
    /// Sends changed stats and achievements to Steam.
    fn store(&mut self) -> bool;
    /// The language the player runs Steam's copy of the game in (`english`, `japanese`).
    fn language(&self) -> Option<String>;
}

#[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
pub use desktop::connect;

/// Steam exists only on desktop.
#[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
pub fn connect(_app_id: u32) -> Result<Box<dyn SteamApi>, String> {
    Err("Steam is desktop only".into())
}

#[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
mod desktop {
    use std::ffi::{CStr, CString, c_char, c_void};
    use std::path::{Path, PathBuf};

    use libloading::{Library, Symbol};

    use super::SteamApi;

    const LIBRARY: &str = if cfg!(windows) {
        "steam_api64.dll"
    } else if cfg!(target_os = "macos") {
        "libsteam_api.dylib"
    } else {
        "libsteam_api.so"
    };
    /// Overrides where the library is (what `kulisa run` sets from `[steam] sdk`).
    const LIBRARY_ENV: &str = "KULISA_STEAM_API";

    type Interface = *mut c_void;
    type Accessor = unsafe extern "C" fn() -> Interface;
    type NameBool = unsafe extern "C" fn(Interface, *const c_char) -> bool;
    type Store = unsafe extern "C" fn(Interface) -> bool;
    type GetBool = unsafe extern "C" fn(Interface, *const c_char, *mut bool) -> bool;
    type Progress = unsafe extern "C" fn(Interface, *const c_char, u32, u32) -> bool;
    type GetInt = unsafe extern "C" fn(Interface, *const c_char, *mut i32) -> bool;
    type GetFloat = unsafe extern "C" fn(Interface, *const c_char, *mut f32) -> bool;
    type SetInt = unsafe extern "C" fn(Interface, *const c_char, i32) -> bool;
    type SetFloat = unsafe extern "C" fn(Interface, *const c_char, f32) -> bool;
    type Language = unsafe extern "C" fn(Interface) -> *const c_char;

    /// The functions, copied out of the library, which `_library` keeps loaded.
    struct Steam {
        stats: Interface,
        apps: Option<(Interface, Language)>,
        run_callbacks: unsafe extern "C" fn(),
        shutdown: unsafe extern "C" fn(),
        set_achievement: NameBool,
        clear_achievement: NameBool,
        get_achievement: GetBool,
        store_stats: Store,
        indicate_progress: Progress,
        get_int: GetInt,
        get_float: GetFloat,
        set_int: SetInt,
        set_float: SetFloat,
        _library: Option<Library>,
    }

    fn library_path() -> Result<PathBuf, String> {
        if let Some(p) = std::env::var_os(LIBRARY_ENV) {
            return Ok(p.into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let dir = exe.parent().unwrap_or(Path::new("."));
        // Next to the program only: not the working directory, where anyone could plant one.
        Ok(dir.join(LIBRARY))
    }

    /// A function from the library, by its C name.
    unsafe fn symbol<T: Copy>(lib: &Library, name: &str) -> Result<T, String> {
        let found: Symbol<T> = unsafe { lib.get(name.as_bytes()) }.map_err(|e| format!("{name}: {e}"))?;
        Ok(*found)
    }

    /// The first of the interface's accessors this version of the library exports.
    unsafe fn interface(lib: &Library, name: &str) -> Result<Interface, String> {
        for version in (1..=20).rev() {
            let accessor = format!("SteamAPI_{name}_v{version:03}");
            if let Ok(get) = unsafe { symbol::<Accessor>(lib, &accessor) } {
                let found = unsafe { get() };
                return if found.is_null() { Err(format!("{accessor} returned nothing")) } else { Ok(found) };
            }
        }
        Err(format!("this Steam library has no {name} interface"))
    }

    /// Loads the library and connects to the Steam client as `app_id`.
    pub fn connect(app_id: u32) -> Result<Box<dyn SteamApi>, String> {
        let path = library_path()?;
        if !path.exists() {
            return Err(format!("{} is not there", path.display()));
        }
        // SAFETY: the Steam library's initializers are safe to run, and every signature below
        // follows its `steam_api_flat.h`.
        unsafe {
            let lib = Library::new(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            // How a game started outside Steam names itself; Steam sets it when it starts one.
            // Nothing else reads the environment yet: audio and scripts start after this.
            if std::env::var_os("SteamAppId").is_none() {
                std::env::set_var("SteamAppId", app_id.to_string());
            }
            init(&lib)?;
            let shutdown: unsafe extern "C" fn() = symbol(&lib, "SteamAPI_Shutdown")?;
            let connected = (|| -> Result<Steam, String> {
                let stats = interface(&lib, "SteamUserStats")?;
                // Older libraries need this; newer ones removed it (the client syncs first).
                if let Ok(request) = symbol::<Store>(&lib, "SteamAPI_ISteamUserStats_RequestCurrentStats") {
                    request(stats);
                }
                let apps = interface(&lib, "SteamApps").ok().and_then(|apps| {
                    symbol::<Language>(&lib, "SteamAPI_ISteamApps_GetCurrentGameLanguage")
                        .ok()
                        .map(|f| (apps, f))
                });
                Ok(Steam {
                    stats,
                    apps,
                    run_callbacks: symbol(&lib, "SteamAPI_RunCallbacks")?,
                    shutdown,
                    set_achievement: symbol(&lib, "SteamAPI_ISteamUserStats_SetAchievement")?,
                    clear_achievement: symbol(&lib, "SteamAPI_ISteamUserStats_ClearAchievement")?,
                    get_achievement: symbol(&lib, "SteamAPI_ISteamUserStats_GetAchievement")?,
                    store_stats: symbol(&lib, "SteamAPI_ISteamUserStats_StoreStats")?,
                    indicate_progress: symbol(&lib, "SteamAPI_ISteamUserStats_IndicateAchievementProgress")?,
                    get_int: symbol(&lib, "SteamAPI_ISteamUserStats_GetStatInt32")?,
                    get_float: symbol(&lib, "SteamAPI_ISteamUserStats_GetStatFloat")?,
                    set_int: symbol(&lib, "SteamAPI_ISteamUserStats_SetStatInt32")?,
                    set_float: symbol(&lib, "SteamAPI_ISteamUserStats_SetStatFloat")?,
                    _library: None,
                })
            })();
            match connected {
                Ok(mut steam) => {
                    steam._library = Some(lib);
                    Ok(Box::new(steam))
                }
                Err(e) => {
                    shutdown();
                    Err(e)
                }
            }
        }
    }

    /// `SteamAPI_InitFlat` (SDK 1.59 and later), else the older `SteamAPI_Init`.
    unsafe fn init(lib: &Library) -> Result<(), String> {
        type InitFlat = unsafe extern "C" fn(*mut c_char) -> i32;
        if let Ok(init) = unsafe { symbol::<InitFlat>(lib, "SteamAPI_InitFlat") } {
            let mut message = [0 as c_char; 1024];
            let result = unsafe { init(message.as_mut_ptr()) };
            if result == 0 {
                return Ok(());
            }
            let text = unsafe { CStr::from_ptr(message.as_ptr()) }.to_string_lossy();
            return Err(format!("Steam did not start ({result}): {text}"));
        }
        let init: unsafe extern "C" fn() -> bool = unsafe { symbol(lib, "SteamAPI_Init") }?;
        if unsafe { init() } { Ok(()) } else { Err("Steam did not start; is the client running?".into()) }
    }

    /// Names with a NUL byte cannot exist in Steam.
    fn c(name: &str) -> Option<CString> {
        CString::new(name).ok()
    }

    // SAFETY for the calls below: the functions come from the library `_library` keeps loaded,
    // with the signatures of `steam_api_flat.h`, on the one thread the game runs on.
    impl SteamApi for Steam {
        fn run_callbacks(&mut self) {
            unsafe { (self.run_callbacks)() }
        }

        fn unlock(&mut self, achievement: &str) -> bool {
            c(achievement).is_some_and(|n| unsafe { (self.set_achievement)(self.stats, n.as_ptr()) })
        }

        fn clear(&mut self, achievement: &str) -> bool {
            c(achievement).is_some_and(|n| unsafe { (self.clear_achievement)(self.stats, n.as_ptr()) })
        }

        fn unlocked(&self, achievement: &str) -> Option<bool> {
            let name = c(achievement)?;
            let mut achieved = false;
            unsafe { (self.get_achievement)(self.stats, name.as_ptr(), &mut achieved) }.then_some(achieved)
        }

        fn progress(&mut self, achievement: &str, current: u32, max: u32) -> bool {
            c(achievement)
                .is_some_and(|n| unsafe { (self.indicate_progress)(self.stats, n.as_ptr(), current, max) })
        }

        fn stat(&self, name: &str) -> Option<f64> {
            let name = c(name)?;
            let (mut i, mut f) = (0i32, 0f32);
            // A stat has one type in Steamworks; the call for the other type fails.
            if unsafe { (self.get_int)(self.stats, name.as_ptr(), &mut i) } {
                Some(i as f64)
            } else if unsafe { (self.get_float)(self.stats, name.as_ptr(), &mut f) } {
                Some(f as f64)
            } else {
                None
            }
        }

        fn set_stat(&mut self, name: &str, value: f64) -> bool {
            let Some(name) = c(name) else { return false };
            let whole = value.fract() == 0.0 && value.abs() <= i32::MAX as f64;
            (whole && unsafe { (self.set_int)(self.stats, name.as_ptr(), value as i32) })
                || unsafe { (self.set_float)(self.stats, name.as_ptr(), value as f32) }
        }

        fn store(&mut self) -> bool {
            unsafe { (self.store_stats)(self.stats) }
        }

        fn language(&self) -> Option<String> {
            let (apps, get) = self.apps?;
            let text = unsafe { get(apps) };
            if text.is_null() {
                return None;
            }
            Some(unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned())
        }
    }

    impl Drop for Steam {
        fn drop(&mut self) {
            unsafe { (self.shutdown)() }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_missing_library_is_an_error_not_a_crash() {
            // The test program has no Steam library next to it.
            let e = connect(480).err().expect("no library here");
            assert!(e.contains("is not there"), "{e}");
        }
    }
}
