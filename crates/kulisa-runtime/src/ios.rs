//! iOS haptics (UIKit) and motion (CoreMotion).

use kulisa_platform::Motion;
use objc2::rc::Retained;
use objc2_core_motion::CMMotionManager;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, Ordering};

use block2::RcBlock;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, msg_send_id};
use objc2_foundation::{CGRect, MainThreadMarker, NSNotification, NSNotificationCenter};
use objc2_ui_kit::{
    UIImpactFeedbackGenerator, UIImpactFeedbackStyle, UIKeyboardFrameEndUserInfoKey,
    UIKeyboardWillChangeFrameNotification,
};

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

/// The keyboard's height in points, as bits of an `f32` (the notification may arrive on any
/// thread the system picks; reads happen on the main thread).
static KEYBOARD: AtomicU32 = AtomicU32::new(0);

/// Starts following the on-screen keyboard. Call once on the main thread at startup, so the
/// first time it appears is not missed.
pub fn observe_keyboard() {
    // SAFETY: plain Foundation/UIKit calls on the main thread; the block only reads the
    // notification it is given and stores a number.
    unsafe {
        let block = RcBlock::new(|note: NonNull<NSNotification>| {
            let note = note.as_ref();
            let info: Option<Retained<AnyObject>> = msg_send_id![note, userInfo];
            let Some(info) = info else { return };
            let value: Option<Retained<AnyObject>> =
                msg_send_id![&*info, objectForKey: UIKeyboardFrameEndUserInfoKey];
            let Some(value) = value else { return };
            let end: CGRect = msg_send![&*value, CGRectValue];
            let screen: Retained<AnyObject> = msg_send_id![class!(UIScreen), mainScreen];
            let bounds: CGRect = msg_send![&*screen, bounds];
            let scale: f64 = msg_send![&*screen, scale];
            // The frame ends below the screen when the keyboard is going away.
            let height = (bounds.size.height - end.origin.y).max(0.0);
            KEYBOARD.store(((height * scale) as f32).to_bits(), Ordering::Relaxed);
        });
        let center = NSNotificationCenter::defaultCenter();
        let token = center.addObserverForName_object_queue_usingBlock(
            Some(UIKeyboardWillChangeFrameNotification),
            None,
            None,
            &block,
        );
        // The observer lives as long as the app.
        std::mem::forget(token);
        std::mem::forget(block);
    }
}

/// The on-screen keyboard's height in window pixels (0 when hidden).
pub fn keyboard_inset() -> f32 {
    f32::from_bits(KEYBOARD.load(Ordering::Relaxed))
}
