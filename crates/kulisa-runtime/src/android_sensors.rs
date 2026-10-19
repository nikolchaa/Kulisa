//! Android motion sensors through the NDK: the gyroscope and the accelerometer (low-pass
//! filtered into a gravity direction).

use std::ptr;

use kulisa_platform::Motion;
use ndk_sys::{
    ALooper_forThread, ALooper_prepare, ASensorEvent, ASensorEventQueue, ASensorEventQueue_disableSensor,
    ASensorEventQueue_enableSensor, ASensorEventQueue_getEvents, ASensorEventQueue_setEventRate,
    ASensorManager, ASensorManager_createEventQueue, ASensorManager_destroyEventQueue,
    ASensorManager_getDefaultSensor, ASensorManager_getInstanceForPackage, ASensorRef,
};

const TYPE_ACCELEROMETER: i32 = 1;
const TYPE_GYROSCOPE: i32 = 4;
/// Microseconds between readings (60 Hz).
const PERIOD_US: i32 = 16_667;
/// Weight of each new accelerometer reading in the gravity estimate.
const SMOOTHING: f32 = 0.2;

pub struct Sensors {
    manager: *mut ASensorManager,
    queue: *mut ASensorEventQueue,
    gyro: ASensorRef,
    accel: ASensorRef,
    motion: Motion,
    seen: bool,
    on: bool,
}

impl Sensors {
    pub fn new() -> Option<Sensors> {
        // SAFETY: plain NDK calls; the queue lives as long as this struct. The activity's main
        // thread already has a looper (android-activity runs one), which the queue needs.
        unsafe {
            let manager = ASensorManager_getInstanceForPackage(c"dev.kulisa.player".as_ptr());
            if manager.is_null() {
                return None;
            }
            let mut looper = ALooper_forThread();
            if looper.is_null() {
                looper = ALooper_prepare(0);
            }
            let queue = ASensorManager_createEventQueue(manager, looper, 0x4b53, None, ptr::null_mut());
            if queue.is_null() {
                return None;
            }
            let gyro = ASensorManager_getDefaultSensor(manager, TYPE_GYROSCOPE);
            let accel = ASensorManager_getDefaultSensor(manager, TYPE_ACCELEROMETER);
            if gyro.is_null() && accel.is_null() {
                ASensorManager_destroyEventQueue(manager, queue);
                return None;
            }
            Some(Sensors {
                manager,
                queue,
                gyro,
                accel,
                motion: Motion { rate: [0.0; 3], gravity: [0.0, 0.0, -9.80665] },
                seen: false,
                on: false,
            })
        }
    }

    pub fn set(&mut self, on: bool) {
        if on == self.on {
            return;
        }
        self.on = on;
        // SAFETY: the queue and sensors are valid for the life of `self`.
        unsafe {
            for s in [self.gyro, self.accel] {
                if s.is_null() {
                    continue;
                }
                if on {
                    ASensorEventQueue_enableSensor(self.queue, s);
                    ASensorEventQueue_setEventRate(self.queue, s, PERIOD_US);
                } else {
                    ASensorEventQueue_disableSensor(self.queue, s);
                }
            }
        }
        if !on {
            self.seen = false;
        }
    }

    /// Reads what arrived since the last call and returns the current estimate.
    pub fn poll(&mut self) -> Option<Motion> {
        if !self.on {
            return None;
        }
        // SAFETY: `events` is plain data the NDK fills; vector members are floats for these
        // sensor types.
        unsafe {
            let mut events: [ASensorEvent; 16] = std::mem::zeroed();
            loop {
                let n = ASensorEventQueue_getEvents(self.queue, events.as_mut_ptr(), events.len());
                if n <= 0 {
                    break;
                }
                for e in &events[..n as usize] {
                    let v = e.__bindgen_anon_1.__bindgen_anon_1.vector.__bindgen_anon_1.v;
                    match e.type_ {
                        TYPE_GYROSCOPE => self.motion.rate = v,
                        TYPE_ACCELEROMETER => {
                            // The accelerometer reads opposite to gravity (+9.8 on z when flat).
                            for (g, a) in self.motion.gravity.iter_mut().zip(v) {
                                *g += (-a - *g) * SMOOTHING;
                            }
                        }
                        _ => continue,
                    }
                    self.seen = true;
                }
            }
        }
        self.seen.then_some(self.motion)
    }
}

impl Drop for Sensors {
    fn drop(&mut self) {
        self.set(false);
        // SAFETY: the queue was made by this manager and is not used again.
        unsafe { ASensorManager_destroyEventQueue(self.manager, self.queue) };
    }
}
