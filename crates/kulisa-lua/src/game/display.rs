//! Display profiles: one design by default, with optional phone, tablet and desktop
//! overrides of the design resolution, scale mode, aspect range and orientation. Also the
//! requests the runtime carries out for scripts (orientation lock, window shape,
//! fullscreen, quit).

use kulisa_core::{ScaleMode, Vec2, Viewport, parse_aspect};
use mlua::{Table, Value as LuaValue};

use super::State;
use super::api::number;

/// The kind of device, from the OS and screen size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormFactor {
    #[default]
    Desktop,
    Tablet,
    Phone,
}

impl FormFactor {
    pub fn name(self) -> &'static str {
        match self {
            FormFactor::Desktop => "desktop",
            FormFactor::Tablet => "tablet",
            FormFactor::Phone => "phone",
        }
    }

    pub fn from_name(s: &str) -> Option<FormFactor> {
        match s {
            "desktop" => Some(FormFactor::Desktop),
            "tablet" => Some(FormFactor::Tablet),
            "phone" => Some(FormFactor::Phone),
            _ => None,
        }
    }

    /// Phones have a touch screen whose smaller side is under 600 logical pixels; tablets
    /// are larger touch screens (up to 1400 on the long side, so touch laptops stay desktops).
    pub fn detect(touch: bool, logical: Vec2) -> FormFactor {
        let (short, long) = (logical.x.min(logical.y), logical.x.max(logical.y));
        match (touch, short < 600.0, long <= 1400.0) {
            (true, true, _) => FormFactor::Phone,
            (true, false, true) => FormFactor::Tablet,
            _ => FormFactor::Desktop,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Landscape,
    Portrait,
    /// Follows the design aspect.
    Auto,
    /// The device may rotate; scripts get `display` events.
    Any,
}

impl Orientation {
    fn from_name(s: &str) -> Option<Orientation> {
        match s {
            "landscape" => Some(Orientation::Landscape),
            "portrait" => Some(Orientation::Portrait),
            "auto" => Some(Orientation::Auto),
            "any" => Some(Orientation::Any),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Orientation::Landscape => "landscape",
            Orientation::Portrait => "portrait",
            Orientation::Auto => "auto",
            Orientation::Any => "any",
        }
    }
}

/// What the runtime knows about the device at startup.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Device {
    pub form: FormFactor,
    pub touch: bool,
    /// The platform can lock the orientation (Android, iOS).
    pub can_lock: bool,
    /// The game can close itself (desktop).
    pub can_quit: bool,
    /// Motion sensors (gyroscope, accelerometer) can be read.
    pub sensors: bool,
    /// The platform can vibrate (or has a gamepad to rumble).
    pub haptics: bool,
}

/// Things scripts asked for that the runtime does (see [`crate::Game::take_requests`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Requests {
    /// Lock to `Landscape` or `Portrait`, or unlock (`Any`).
    pub orientation: Option<Orientation>,
    /// Desktop: reshape the window to this aspect (width / height) at startup.
    pub window_aspect: Option<f32>,
    pub fullscreen: Option<bool>,
    pub quit: bool,
}

/// One profile's settings; unset fields come from the base.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Profile {
    design: Option<Vec2>,
    mode: Option<ScaleMode>,
    aspect: Option<Option<[f32; 2]>>,
    orientation: Option<Orientation>,
}

#[derive(Default)]
pub(crate) struct DisplayState {
    pub device: Device,
    pub requests: Requests,
    /// The wanted orientation, resolved (`Landscape`, `Portrait` or `Any`).
    pub wanted: Option<Orientation>,
    pub fullscreen: bool,
    /// What the last `display` event reported.
    pub reported: Option<(FormFactor, &'static str, bool)>,
    /// The window was already shaped (only at startup).
    pub shaped: bool,
}

fn profile(t: &Table, modes: &[(&str, ScaleMode)]) -> mlua::Result<Profile> {
    let mut p = Profile::default();
    let w = t.get::<LuaValue>("width")?;
    let h = t.get::<LuaValue>("height")?;
    if !w.is_nil() || !h.is_nil() {
        p.design = Some(Vec2::new(number(&w, "width")?, number(&h, "height")?));
    }
    if let Some(m) = t.get::<Option<String>>("scale")? {
        p.mode = Some(modes.iter().find(|(n, _)| *n == m).map(|(_, v)| *v).ok_or_else(|| {
            mlua::Error::runtime(format!("`scale` must be \"expand\", \"fit\" or \"fill\", not `{m}`"))
        })?);
    }
    match t.get::<LuaValue>("aspect")? {
        LuaValue::Nil => {}
        LuaValue::Boolean(false) => p.aspect = Some(None),
        LuaValue::Table(a) => {
            let one = |v: LuaValue| -> mlua::Result<f32> {
                match &v {
                    LuaValue::String(s) => parse_aspect(&s.to_string_lossy()),
                    v => number(v, "aspect").ok().filter(|n| *n > 0.0),
                }
                .ok_or_else(|| mlua::Error::runtime("`aspect` entries must be like \"4:3\" or 1.333"))
            };
            let min = one(a.get::<LuaValue>(1)?)?;
            let max = one(a.get::<LuaValue>(2)?)?;
            p.aspect = Some(Some([min.min(max), max.max(min)]));
        }
        _ => return Err(mlua::Error::runtime("`aspect` must be { min, max }, like { \"4:3\", \"21:9\" }")),
    }
    if let Some(o) = t.get::<Option<String>>("orientation")? {
        p.orientation = Some(Orientation::from_name(&o).ok_or_else(|| {
            mlua::Error::runtime("`orientation` must be \"landscape\", \"portrait\", \"auto\" or \"any\"")
        })?);
    }
    Ok(p)
}

/// `kulisa.display{ width, height, scale, aspect, orientation, phone = {...}, ... }`.
pub(crate) fn configure(st: &mut State, t: &Table, modes: &[(&str, ScaleMode)]) -> mlua::Result<()> {
    let mut p = profile(t, modes)?;
    let form = st.display.device.form;
    if let LuaValue::Table(o) = t.get::<LuaValue>(form.name())? {
        let over = profile(&o, modes)?;
        p.design = over.design.or(p.design);
        p.mode = over.mode.or(p.mode);
        p.aspect = over.aspect.or(p.aspect);
        p.orientation = over.orientation.or(p.orientation);
    }
    for name in ["phone", "tablet", "desktop"] {
        if let LuaValue::Table(o) = t.get::<LuaValue>(name)? {
            profile(&o, modes)?; // report mistakes in every profile, not just the active one
        }
    }
    let design = p.design.unwrap_or(Vec2::new(1920.0, 1080.0));
    let mut viewport = Viewport::new(design.x, design.y, p.mode.unwrap_or(ScaleMode::Expand));
    viewport.aspect = p.aspect.flatten();
    apply(st, viewport, p.orientation.unwrap_or(Orientation::Auto));
    Ok(())
}

/// Uses `viewport` and asks the runtime for the orientation (and, on desktop at startup,
/// a window of that shape).
pub(crate) fn apply(st: &mut State, viewport: Viewport, orientation: Orientation) {
    st.viewport = viewport;
    st.scene.camera = kulisa_core::Camera::new(viewport.design * 0.5);
    let wanted = match orientation {
        Orientation::Auto if viewport.design.y > viewport.design.x => Orientation::Portrait,
        Orientation::Auto => Orientation::Landscape,
        o => o,
    };
    st.display.wanted = Some(wanted);
    st.display.requests.orientation = Some(wanted);
    if st.display.device.form == FormFactor::Desktop && !st.display.shaped {
        st.display.shaped = true;
        st.display.requests.window_aspect = Some(viewport.design.x / viewport.design.y);
    }
}

/// The window's orientation now.
pub(crate) fn orientation(st: &State) -> &'static str {
    let s = st.renderer.size();
    if s.y > s.x { "portrait" } else { "landscape" }
}

/// Whether the window's orientation differs from the wanted one where the platform cannot
/// lock it (touch screens on the web): the game should ask the player to rotate.
pub(crate) fn mismatch(st: &State) -> bool {
    let d = &st.display;
    if d.device.can_lock || !d.device.touch {
        return false;
    }
    match d.wanted {
        Some(Orientation::Landscape) => orientation(st) == "portrait",
        Some(Orientation::Portrait) => orientation(st) == "landscape",
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_factors() {
        assert_eq!(FormFactor::detect(true, Vec2::new(412.0, 915.0)), FormFactor::Phone);
        assert_eq!(FormFactor::detect(true, Vec2::new(820.0, 1180.0)), FormFactor::Tablet);
        assert_eq!(FormFactor::detect(true, Vec2::new(1920.0, 1080.0)), FormFactor::Desktop);
        assert_eq!(FormFactor::detect(false, Vec2::new(400.0, 800.0)), FormFactor::Desktop);
    }
}
