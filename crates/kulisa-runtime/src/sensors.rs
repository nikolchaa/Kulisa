//! Motion sensors across platforms: the NDK on Android, CoreMotion on iOS and DeviceMotion
//! events in browsers. Desktops have none.

use kulisa_platform::Motion;

/// Whether this platform can read motion sensors at all (`touch`: a browser on a touch screen).
pub fn available(_touch: bool) -> bool {
    cfg!(any(target_os = "android", target_os = "ios")) || (cfg!(target_arch = "wasm32") && _touch)
}

pub struct Sensors {
    #[cfg(target_os = "android")]
    android: Option<crate::android_sensors::Sensors>,
    #[cfg(target_os = "ios")]
    ios: Option<crate::ios::MotionSensors>,
    on: bool,
}

impl Sensors {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "android")]
            android: None,
            #[cfg(target_os = "ios")]
            ios: None,
            on: false,
        }
    }

    pub fn set(&mut self, on: bool) {
        self.on = on;
        #[cfg(target_os = "android")]
        {
            if on && self.android.is_none() {
                self.android = crate::android_sensors::Sensors::new();
                if self.android.is_none() {
                    log::warn!("motion sensors: none available");
                }
            }
            if let Some(s) = &mut self.android {
                s.set(on);
            }
        }
        #[cfg(target_os = "ios")]
        {
            if on && self.ios.is_none() {
                self.ios = Some(crate::ios::MotionSensors::new());
            }
            if let Some(s) = &mut self.ios {
                s.set(on);
            }
        }
        #[cfg(target_arch = "wasm32")]
        if on {
            kulisa_platform::web_motion_start();
        } else {
            kulisa_platform::web_motion_stop();
        }
    }

    /// The latest reading, while the sensors are on.
    pub fn poll(&mut self) -> Option<Motion> {
        if !self.on {
            return None;
        }
        #[cfg(target_os = "android")]
        return self.android.as_mut().and_then(|s| s.poll());
        #[cfg(target_os = "ios")]
        return self.ios.as_mut().and_then(|s| s.poll());
        #[cfg(target_arch = "wasm32")]
        return kulisa_platform::web_motion();
        #[allow(unreachable_code)]
        None
    }
}
