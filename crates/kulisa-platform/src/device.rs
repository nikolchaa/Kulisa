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
