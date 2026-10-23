//! Device facts the runtime needs for display profiles: overrides for testing, and the web's
//! touch support, safe-area insets and URL options.

/// Window insets in pixels (left, top, right, bottom) from `KULISA_SAFE_AREA="l,t,r,b"`
/// (or `?safe=l,t,r,b` on the web), to try notch layouts on a desktop.
pub fn safe_area_override() -> Option<[f32; 4]> {
    let s = option("KULISA_SAFE_AREA", "safe")?;
    let v: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match *v.as_slice() {
        [l, t, r, b] => Some([l, t, r, b]),
        [a] => Some([a; 4]),
        _ => None,
    }
}

/// A forced form factor (`desktop`, `tablet`, `phone`) from `KULISA_PROFILE` (or
/// `?profile=` on the web).
pub fn profile_override() -> Option<String> {
    option("KULISA_PROFILE", "profile")
}

/// A development run (live reload, every error on screen) from `KULISA_DEV=1` (or `?dev=1`).
pub fn dev_override() -> bool {
    option("KULISA_DEV", "dev").is_some_and(|v| v != "0")
}

/// A forced quality tier (`low`, `medium`, `high`) from `KULISA_QUALITY` (or `?quality=`).
pub fn quality_override() -> Option<String> {
    option("KULISA_QUALITY", "quality")
}

#[cfg(not(target_arch = "wasm32"))]
fn option(env: &str, _query: &str) -> Option<String> {
    std::env::var(env).ok().filter(|s| !s.is_empty())
}

#[cfg(target_arch = "wasm32")]
fn option(_env: &str, query: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    search.trim_start_matches('?').split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == query).then(|| v.replace("%2C", ",").replace("%2c", ","))
    })
}

/// Whether the browser has a touch screen.
#[cfg(target_arch = "wasm32")]
pub fn web_touch() -> bool {
    web_sys::window().is_some_and(|w| w.navigator().max_touch_points() > 0)
}

/// Whether something on the page is fullscreen: the game's canvas, or the whole page (the
/// player's first tap on a phone asks for that).
#[cfg(target_arch = "wasm32")]
pub fn web_fullscreen() -> bool {
    web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.fullscreen_element().is_some())
}

/// Locks the screen to `"landscape"` or `"portrait"`. Browsers allow it only while the page is
/// fullscreen (Chrome on Android; Safari never), so callers try again when it becomes
/// fullscreen; refusals are expected and ignored.
#[cfg(target_arch = "wasm32")]
pub fn web_lock_orientation(orientation: &str) {
    use web_sys::OrientationLockType as Lock;
    let Some(screen) = web_sys::window().and_then(|w| w.screen().ok()) else { return };
    let lock = if orientation == "portrait" { Lock::Portrait } else { Lock::Landscape };
    if let Ok(promise) = screen.orientation().lock(lock) {
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        });
    }
}

/// The page's safe-area insets (CSS `env(safe-area-inset-*)`) in device pixels.
#[cfg(target_arch = "wasm32")]
pub fn web_safe_area() -> [f32; 4] {
    use wasm_bindgen::JsCast;
    let get = || -> Option<[f32; 4]> {
        let window = web_sys::window()?;
        let doc = window.document()?;
        let probe = match doc.get_element_by_id("kulisa-safe-area") {
            Some(e) => e,
            None => {
                let e = doc.create_element("div").ok()?;
                e.set_id("kulisa-safe-area");
                let style = "position:fixed;visibility:hidden;pointer-events:none;left:0;top:0;\
                    padding-left:env(safe-area-inset-left,0px);padding-top:env(safe-area-inset-top,0px);\
                    padding-right:env(safe-area-inset-right,0px);padding-bottom:env(safe-area-inset-bottom,0px)";
                e.set_attribute("style", style).ok()?;
                doc.body()?.append_child(&e).ok()?;
                e
            }
        };
        let css = window.get_computed_style(probe.dyn_ref::<web_sys::Element>()?).ok()??;
        let px = |name: &str| {
            css.get_property_value(name)
                .ok()
                .and_then(|v| v.trim_end_matches("px").parse::<f32>().ok())
                .unwrap_or(0.0)
        };
        let dpr = window.device_pixel_ratio() as f32;
        Some(
            [px("padding-left"), px("padding-top"), px("padding-right"), px("padding-bottom")]
                .map(|v| v * dpr),
        )
    };
    get().unwrap_or([0.0; 4])
}

/// A reading of the motion sensors, in the device frame: x to the right of the screen, y
/// up the screen, z out of it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    /// Rotation speed around the x, y and z axes in radians per second.
    pub rate: [f32; 3],
    /// The direction gravity pulls, in m/s² (about 9.8 long): a device lying face up reads
    /// `[0, 0, -9.8]`, one held upright `[0, -9.8, 0]`.
    pub gravity: [f32; 3],
}

impl Motion {
    /// Gravity from the W3C `deviceorientation` angles (degrees): `beta` is the front-back
    /// tilt (0 flat, 90 upright), `gamma` the left-right tilt (positive: right side down).
    pub fn gravity_from_angles(beta: f32, gamma: f32) -> [f32; 3] {
        const G: f32 = 9.80665;
        let (b, g) = (beta.to_radians(), gamma.to_radians());
        [G * g.sin() * b.cos(), -G * b.sin(), -G * g.cos() * b.cos()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_follows_the_w3c_angles() {
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3);
        let g = 9.80665;
        assert!(near(Motion::gravity_from_angles(0.0, 0.0), [0.0, 0.0, -g]), "flat, face up");
        assert!(near(Motion::gravity_from_angles(90.0, 0.0), [0.0, -g, 0.0]), "upright");
        assert!(near(Motion::gravity_from_angles(0.0, 90.0), [g, 0.0, 0.0]), "right side down");
    }
}
