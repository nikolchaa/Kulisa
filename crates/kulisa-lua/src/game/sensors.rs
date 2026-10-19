//! Motion sensors and haptics: what scripts ask for here the runtime carries out, and the
//! runtime feeds the readings back.

use kulisa_platform::Motion;
use mlua::{Lua, Table};

use super::Game;
use super::api::{number, with};

const G: f32 = 9.80665;
/// Pulses queued for the runtime at most (a script firing one per frame must not pile up).
const MAX_PENDING_PULSES: usize = 16;

#[derive(Default)]
pub(crate) struct Sensors {
    /// Scripts want readings.
    pub enabled: bool,
    pub motion: Option<Motion>,
    /// Start (`true`) or stop sensors, once.
    pub request: Option<bool>,
    /// `(strength 0..1, seconds)` pulses the runtime has yet to play.
    pub pulses: Vec<(f32, f32)>,
}

impl Game {
    /// The latest motion reading (the runtime calls this every frame while
    /// [`Game::take_sensor_request`] last said to run).
    pub fn set_motion(&mut self, motion: Option<Motion>) {
        self.state.borrow_mut().sensors.motion = motion;
    }

    /// `Some(true)` when scripts started the sensors, `Some(false)` when they stopped them.
    pub fn take_sensor_request(&mut self) -> Option<bool> {
        self.state.borrow_mut().sensors.request.take()
    }

    /// Whether the sensors should be running now.
    pub fn sensors_wanted(&self) -> bool {
        self.state.borrow().sensors.enabled
    }

    /// Haptic pulses scripts asked for since the last call: `(strength, seconds)`.
    pub fn take_haptics(&mut self) -> Vec<(f32, f32)> {
        std::mem::take(&mut self.state.borrow_mut().sensors.pulses)
    }
}

pub(super) fn install(lua: &Lua, k: &Table) -> mlua::Result<()> {
    let s = lua.create_table()?;
    s.set("available", lua.create_function(|lua, ()| with(lua, |st| Ok(st.display.device.sensors)))?)?;
    // enable(on): start or stop reading. Browsers on iOS ask the player for permission, which
    // only works when this is called while handling a tap.
    s.set(
        "enable",
        lua.create_function(|lua, on: Option<bool>| {
            let on = on.unwrap_or(true);
            with(lua, |st| {
                if st.sensors.enabled != on {
                    st.sensors.enabled = on;
                    st.sensors.request = Some(on);
                    if !on {
                        st.sensors.motion = None;
                    }
                }
                Ok(())
            })
        })?,
    )?;
    // gyro() -> x, y, z rad/s; nil until the first reading.
    s.set(
        "gyro",
        lua.create_function(|lua, ()| {
            with(lua, |st| Ok(st.sensors.motion.map(|m| m.rate).map_or((None, None, None), triple)))
        })?,
    )?;
    // gravity() -> x, y, z m/s², the direction gravity pulls in the device frame.
    s.set(
        "gravity",
        lua.create_function(|lua, ()| {
            with(lua, |st| Ok(st.sensors.motion.map(|m| m.gravity).map_or((None, None, None), triple)))
        })?,
    )?;
    // tilt() -> x, y in -1..1 (1 is 45 degrees): x is positive with the right edge down, y with
    // the top edge leaning away from you; 0, 0 when the device is held upright.
    s.set(
        "tilt",
        lua.create_function(|lua, ()| {
            with(lua, |st| {
                Ok(st.sensors.motion.map_or((None, None), |m| {
                    let k = std::f32::consts::FRAC_1_SQRT_2 * G;
                    (Some((m.gravity[0] / k).clamp(-1.0, 1.0)), Some((-m.gravity[2] / k).clamp(-1.0, 1.0)))
                }))
            })
        })?,
    )?;
    k.set("sensors", s)?;

    let h = lua.create_table()?;
    h.set("available", lua.create_function(|lua, ()| with(lua, |st| Ok(st.display.device.haptics)))?)?;
    // pulse(strength, seconds): a vibration (phones) or rumble (gamepads). Strength 0..1,
    // default 0.5; seconds default 0.05, at most 2. Platforms without haptics ignore it.
    h.set(
        "pulse",
        lua.create_function(|lua, (strength, secs): (Option<mlua::Value>, Option<mlua::Value>)| {
            let strength = match strength {
                Some(v) if !v.is_nil() => number(&v, "strength")?,
                _ => 0.5,
            };
            let secs = match secs {
                Some(v) if !v.is_nil() => number(&v, "seconds")?,
                _ => 0.05,
            };
            with(lua, |st| {
                if st.sensors.pulses.len() < MAX_PENDING_PULSES {
                    st.sensors.pulses.push((strength.clamp(0.0, 1.0), secs.clamp(0.0, 2.0)));
                }
                Ok(())
            })
        })?,
    )?;
    k.set("haptics", h)
}

fn triple(v: [f32; 3]) -> (Option<f32>, Option<f32>, Option<f32>) {
    (Some(v[0]), Some(v[1]), Some(v[2]))
}
