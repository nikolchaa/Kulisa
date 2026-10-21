//! Quality tiers. The engine picks `low`, `medium` or `high` from the GPU and the device
//! (or `kulisa.toml`, `KULISA_QUALITY`, `?quality=`, or the game says), sets the render
//! scale for it, and in `auto` mode steps down when frames stay slow. Games read the tier to
//! choose their own costs (particle counts, post-processing) through `kulisa.quality`.

use kulisa_render::{GpuInfo, GpuKind};
use mlua::{IntoLua, MetaMethod, UserData, UserDataMethods, Value as LuaValue};

use super::api::{number, with};
use super::display::{Device, FormFactor};
use super::{Shared, State, emit};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Low,
    Medium,
    High,
}

pub const TIERS: &[(&str, Tier)] = &[("low", Tier::Low), ("medium", Tier::Medium), ("high", Tier::High)];

impl Tier {
    pub fn name(self) -> &'static str {
        TIERS.iter().find(|(_, t)| *t == self).map_or("medium", |(n, _)| n)
    }

    fn from_name(s: &str) -> Option<Tier> {
        TIERS.iter().find(|(n, _)| *n == s).map(|(_, t)| *t)
    }

    /// The most window pixels this tier draws (larger windows get a render scale).
    fn pixel_budget(self) -> f32 {
        match self {
            Tier::Low => 1600.0 * 900.0,
            Tier::Medium => 2560.0 * 1440.0,
            Tier::High => 3840.0 * 2160.0,
        }
    }
}

/// The tier a GPU and device are good for.
pub fn detect(gpu: &GpuInfo, device: &Device) -> Tier {
    match (gpu.kind, gpu.backend.as_str()) {
        (GpuKind::Software, _) => Tier::Low,
        // WebGL2 and GLES: old or emulated GPUs, and no compute anyway.
        (_, "gl") => Tier::Low,
        _ if device.form != FormFactor::Desktop => Tier::Medium,
        (GpuKind::Discrete, _) => Tier::High,
        // Apple silicon reports an integrated GPU.
        (GpuKind::Integrated, "metal") => Tier::High,
        _ => Tier::Medium,
    }
}

/// Frames are watched in windows this long; a slow one steps the tier down in auto mode.
const WINDOW: f32 = 3.0;
const SLOW_FPS: f32 = 40.0;
/// No judgement right after startup or a change (loading, shader compiles).
const SETTLE: f32 = 5.0;

#[derive(Debug)]
pub(crate) struct Quality {
    pub tier: Tier,
    pub detected: Tier,
    /// The tier follows detection and frame times (no one chose one).
    pub auto: bool,
    /// A render scale the game set (`nil` follows the tier).
    pub scale: Option<f32>,
    pub reported: Option<Tier>,
    settle: f32,
    window_time: f32,
    window_frames: u32,
}

impl Default for Quality {
    fn default() -> Self {
        Self {
            tier: Tier::Medium,
            detected: Tier::Medium,
            auto: true,
            scale: None,
            reported: None,
            settle: SETTLE,
            window_time: 0.0,
            window_frames: 0,
        }
    }
}

impl Quality {
    /// Detection, then `kulisa.toml`'s `[quality] tier`, then `KULISA_QUALITY` / `?quality=`.
    pub fn setup(&mut self, gpu: &GpuInfo, device: &Device, config_tier: &str) {
        self.detected = detect(gpu, device);
        let forced = kulisa_platform::quality_override().or(Some(config_tier.to_string()));
        match forced.as_deref().and_then(Tier::from_name) {
            Some(t) => {
                self.tier = t;
                self.auto = false;
            }
            None => {
                self.tier = self.detected;
                self.auto = true;
            }
        }
        log::info!(
            "quality: {} ({}; {} on {})",
            self.tier.name(),
            if self.auto { "detected" } else { "chosen" },
            gpu.name,
            gpu.backend
        );
    }

    /// Counts a frame of `dt` seconds; in auto mode, a slow window lowers the tier.
    /// Returns whether it did.
    fn watch(&mut self, dt: f32) -> bool {
        // Pauses (a hidden window, a debugger, loading) say nothing about the GPU.
        if !self.auto || self.tier == Tier::Low || dt > 0.25 {
            return false;
        }
        if self.settle > 0.0 {
            self.settle -= dt;
            return false;
        }
        self.window_time += dt;
        self.window_frames += 1;
        if self.window_time < WINDOW {
            return false;
        }
        let fps = self.window_frames as f32 / self.window_time;
        self.window_time = 0.0;
        self.window_frames = 0;
        if fps >= SLOW_FPS {
            return false;
        }
        self.tier = if self.tier == Tier::High { Tier::Medium } else { Tier::Low };
        self.settle = SETTLE;
        log::info!("quality: {fps:.0} fps, stepping down to {}", self.tier.name());
        true
    }

    fn set_tier(&mut self, tier: Option<Tier>) {
        self.auto = tier.is_none();
        self.tier = tier.unwrap_or(self.detected);
        self.settle = SETTLE;
        self.window_time = 0.0;
        self.window_frames = 0;
    }
}

/// The render scale for the window now.
fn render_scale(st: &State) -> f32 {
    if let Some(s) = st.quality.scale {
        return s;
    }
    let size = st.renderer.size().as_vec2();
    (st.quality.tier.pixel_budget() / (size.x * size.y).max(1.0)).sqrt().min(1.0)
}

/// Per frame: watch frame times, keep the render scale, and report tier changes.
pub(crate) fn update(shared: &Shared, dt: f32) {
    let changed = {
        let mut st = shared.borrow_mut();
        let st = &mut *st;
        if !st.renderer.is_headless() {
            st.quality.watch(dt);
        }
        let scale = render_scale(st);
        if (st.renderer.render_scale() - scale).abs() > 1e-3 {
            st.renderer.set_render_scale(scale);
        }
        let tier = st.quality.tier;
        (st.quality.reported.replace(tier) != Some(tier)).then_some(tier)
    };
    // The first report is the startup tier, not a change.
    if let Some(tier) = changed
        && shared.borrow().time > 0.0
    {
        emit(shared, "quality", tier.name());
    }
}

/// `kulisa.quality`.
pub(crate) struct QualityRef;

impl UserData for QualityRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |lua, _, key: String| {
            with(lua, |st| match key.as_str() {
                "tier" => st.quality.tier.name().into_lua(lua),
                "detected" => st.quality.detected.name().into_lua(lua),
                "auto" => st.quality.auto.into_lua(lua),
                "render_scale" => (render_scale(st) as f64).into_lua(lua),
                "gpu" => st.renderer.gpu().name.as_str().into_lua(lua),
                "backend" => st.renderer.gpu().backend.as_str().into_lua(lua),
                _ => Err(mlua::Error::runtime(format!("kulisa.quality has no property `{key}`"))),
            })
        });
        m.add_meta_method(MetaMethod::NewIndex, |lua, _, (key, v): (String, LuaValue)| {
            with(lua, |st| {
                match key.as_str() {
                    "tier" => {
                        let name = v.as_string().map(|s| s.to_string_lossy()).unwrap_or_default();
                        let tier = match name.as_str() {
                            "auto" => None,
                            n => Some(Tier::from_name(n).ok_or_else(|| {
                                mlua::Error::runtime(
                                    "`tier` must be \"auto\", \"low\", \"medium\" or \"high\"",
                                )
                            })?),
                        };
                        st.quality.set_tier(tier);
                    }
                    "render_scale" => {
                        st.quality.scale = match v {
                            LuaValue::Nil => None,
                            v => Some(number(&v, "render_scale")?.clamp(0.25, 1.0)),
                        }
                    }
                    _ => return Err(mlua::Error::runtime(format!("`quality.{key}` cannot be set"))),
                }
                Ok(())
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(kind: GpuKind, backend: &str) -> GpuInfo {
        GpuInfo { name: "test".into(), backend: backend.into(), kind }
    }

    #[test]
    fn detects_tiers() {
        let desktop = Device::default();
        let phone = Device { form: FormFactor::Phone, touch: true, ..Default::default() };
        assert_eq!(detect(&gpu(GpuKind::Discrete, "vulkan"), &desktop), Tier::High);
        assert_eq!(detect(&gpu(GpuKind::Integrated, "dx12"), &desktop), Tier::Medium);
        assert_eq!(detect(&gpu(GpuKind::Integrated, "metal"), &desktop), Tier::High);
        assert_eq!(detect(&gpu(GpuKind::Other, "browserwebgpu"), &desktop), Tier::Medium);
        assert_eq!(detect(&gpu(GpuKind::Other, "gl"), &desktop), Tier::Low);
        assert_eq!(detect(&gpu(GpuKind::Software, "vulkan"), &desktop), Tier::Low);
        assert_eq!(detect(&gpu(GpuKind::Integrated, "vulkan"), &phone), Tier::Medium);
    }

    #[test]
    fn slow_frames_step_down_in_auto_mode() {
        let mut q = Quality { tier: Tier::High, ..Default::default() };
        // Settling, then a fast window: no change.
        for _ in 0..((SETTLE + WINDOW) * 60.0) as usize + 10 {
            assert!(!q.watch(1.0 / 60.0));
        }
        // A slow window (30 fps): one step down, then settle again.
        let stepped = (0..(WINDOW * 30.0) as usize + 2).any(|_| q.watch(1.0 / 30.0));
        assert!(stepped);
        assert_eq!(q.tier, Tier::Medium);
        // Pauses do not count, and a chosen tier never moves.
        q.set_tier(Some(Tier::High));
        assert!(!(0..2000).any(|_| q.watch(1.0 / 20.0)));
        assert!(!q.watch(5.0));
    }
}
