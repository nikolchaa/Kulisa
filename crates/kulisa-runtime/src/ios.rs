//! iOS haptics (UIKit) and motion (CoreMotion).

use kulisa_platform::Motion;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_core_motion::CMMotionManager;
use objc2_ui_kit::{UIImpactFeedbackGenerator, UIImpactFeedbackStyle};

const G: f64 = 9.80665;

/// A tap of haptic feedback; `strength` is 0..1.
pub fn impact(strength: f32) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    // SAFETY: on the main thread, as UIKit requires. `initWithStyle:` is deprecated in favor
    // of view-bound generators but works on every iOS we support, and needs no view.
    #[allow(deprecated)]
    unsafe {
        let generator = UIImpactFeedbackGenerator::initWithStyle(mtm.alloc(), UIImpactFeedbackStyle::Medium);
        generator.impactOccurredWithIntensity(strength.clamp(0.0, 1.0) as f64);
    }
}

/// Gyroscope and gravity through `CMMotionManager`'s fused device motion.
pub struct MotionSensors {
    manager: Retained<CMMotionManager>,
    running: bool,
}

impl MotionSensors {
    pub fn new() -> Self {
        // SAFETY: `new` on a plain NSObject subclass.
        Self { manager: unsafe { CMMotionManager::new() }, running: false }
    }

    pub fn set(&mut self, on: bool) {
        // SAFETY: the manager is a live object; these calls may be made from any thread.
        unsafe {
            if on && !self.running && self.manager.isDeviceMotionAvailable() {
                self.manager.setDeviceMotionUpdateInterval(1.0 / 60.0);
                self.manager.startDeviceMotionUpdates();
                self.running = true;
            } else if !on && self.running {
                self.manager.stopDeviceMotionUpdates();
                self.running = false;
            }
        }
    }

    pub fn poll(&mut self) -> Option<Motion> {
        if !self.running {
            return None;
        }
        // SAFETY: as above; the reading is an owned object.
        unsafe {
            let m = self.manager.deviceMotion()?;
            let (r, g) = (m.rotationRate(), m.gravity());
            Some(Motion {
                rate: [r.x as f32, r.y as f32, r.z as f32],
                gravity: [(g.x * G) as f32, (g.y * G) as f32, (g.z * G) as f32],
            })
        }
    }
}
