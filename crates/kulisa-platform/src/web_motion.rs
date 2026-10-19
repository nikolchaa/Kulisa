//! Web motion sensors and vibration: `devicemotion` / `deviceorientation` events and
//! `navigator.vibrate`.
//!
//! Browsers on iOS ask the player for permission, and only when asked from a tap; elsewhere the
//! events simply start. Readings are kept in thread-locals for the runtime to poll.

use std::cell::{Cell, RefCell};

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{Closure, JsValue};
use web_sys::{DeviceMotionEvent, DeviceOrientationEvent};

use crate::Motion;

thread_local! {
    static RATE: Cell<Option<[f32; 3]>> = const { Cell::new(None) };
    static GRAVITY: Cell<Option<[f32; 3]>> = const { Cell::new(None) };
    static LISTENING: Cell<bool> = const { Cell::new(false) };
    static KEEP: RefCell<Vec<Closure<dyn FnMut(web_sys::Event)>>> = const { RefCell::new(Vec::new()) };
}

fn listen() {
    if LISTENING.with(|l| l.replace(true)) {
        return;
    }
    let Some(window) = web_sys::window() else { return };
    let motion = Closure::<dyn FnMut(web_sys::Event)>::new(|e: web_sys::Event| {
        let Ok(e) = e.dyn_into::<DeviceMotionEvent>() else { return };
        if let Some(r) = e.rotation_rate()
            && let (Some(a), Some(b), Some(g)) = (r.alpha(), r.beta(), r.gamma())
        {
            // alpha turns around z, beta around x, gamma around y, in degrees per second.
            RATE.with(|c| c.set(Some([b.to_radians() as f32, g.to_radians() as f32, a.to_radians() as f32])));
        }
    });
    let orient = Closure::<dyn FnMut(web_sys::Event)>::new(|e: web_sys::Event| {
        let Ok(e) = e.dyn_into::<DeviceOrientationEvent>() else { return };
        if let (Some(beta), Some(gamma)) = (e.beta(), e.gamma()) {
            GRAVITY.with(|c| c.set(Some(Motion::gravity_from_angles(beta as f32, gamma as f32))));
        }
    });
    let _ = window.add_event_listener_with_callback("devicemotion", motion.as_ref().unchecked_ref());
    let _ = window.add_event_listener_with_callback("deviceorientation", orient.as_ref().unchecked_ref());
    KEEP.with(|k| k.borrow_mut().extend([motion, orient]));
}

/// Starts motion events. iOS shows a permission prompt, so call this from a tap.
pub fn web_motion_start() {
    let Some(window) = web_sys::window() else { return };
    let ctor =
        js_sys::Reflect::get(&window, &JsValue::from_str("DeviceMotionEvent")).unwrap_or(JsValue::UNDEFINED);
    let ask = js_sys::Reflect::get(&ctor, &JsValue::from_str("requestPermission"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    match ask {
        Some(f) => {
            // iOS: a promise of "granted" or "denied".
            let Ok(promise) = f.call0(&ctor) else { return };
            let Ok(promise) = promise.dyn_into::<js_sys::Promise>() else { return };
            wasm_bindgen_futures::spawn_local(async move {
                match wasm_bindgen_futures::JsFuture::from(promise).await {
                    Ok(v) if v.as_string().as_deref() == Some("granted") => listen(),
                    other => log::warn!("motion sensors: permission not granted ({other:?})"),
                }
            });
        }
        None => listen(),
    }
}

/// Stops using the readings (the listeners stay, they are cheap).
pub fn web_motion_stop() {
    RATE.with(|c| c.set(None));
    GRAVITY.with(|c| c.set(None));
}

/// The latest reading, once the browser has sent any.
pub fn web_motion() -> Option<Motion> {
    let rate = RATE.with(Cell::get);
    let gravity = GRAVITY.with(Cell::get);
    if rate.is_none() && gravity.is_none() {
        return None;
    }
    Some(Motion { rate: rate.unwrap_or_default(), gravity: gravity.unwrap_or([0.0, 0.0, -9.80665]) })
}

/// Vibrates for `ms` milliseconds where the browser can (Chrome on Android).
pub fn web_vibrate(ms: u32) {
    if let Some(window) = web_sys::window() {
        window.navigator().vibrate_with_duration(ms);
    }
}
