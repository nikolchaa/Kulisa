//! Android motion sensors through the NDK: the gyroscope and the accelerometer (low-pass
//! filtered into a gravity direction).
//!
//! The NDK delivers sensor events through an `ALooper`. The activity's own looper would log
//! every event as spurious, so a small thread with its own looper owns the queue and shares
//! the latest reading.

use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use kulisa_platform::Motion;
use ndk_sys::{
    ALooper_pollOnce, ALooper_prepare, ASensorEvent, ASensorEventQueue_disableSensor,
    ASensorEventQueue_enableSensor, ASensorEventQueue_getEvents, ASensorEventQueue_setEventRate,
    ASensorManager_createEventQueue, ASensorManager_destroyEventQueue, ASensorManager_getDefaultSensor,
    ASensorManager_getInstanceForPackage,
};

const TYPE_ACCELEROMETER: i32 = 1;
const TYPE_GYROSCOPE: i32 = 4;
/// Microseconds between readings (60 Hz).
const PERIOD_US: i32 = 16_667;
/// Weight of each new accelerometer reading in the gravity estimate.
const SMOOTHING: f32 = 0.2;
/// ALOOPER_PREPARE_ALLOW_NON_CALLBACKS.
const PREPARE_ALLOW_NON_CALLBACKS: i32 = 1;

#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    motion: Mutex<Option<Motion>>,
}

pub struct Sensors {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

fn run(shared: Arc<Shared>) {
    // SAFETY: plain NDK calls on this thread; the queue and looper belong to it alone and
    // are released before it returns.
    unsafe {
        let manager = ASensorManager_getInstanceForPackage(c"dev.kulisa.player".as_ptr());
        if manager.is_null() {
            return;
        }
        let looper = ALooper_prepare(PREPARE_ALLOW_NON_CALLBACKS);
        let queue = ASensorManager_createEventQueue(manager, looper, 1, None, ptr::null_mut());
        if queue.is_null() {
            return log::warn!("motion sensors: no event queue");
        }
        let sensors: Vec<_> = [TYPE_GYROSCOPE, TYPE_ACCELEROMETER]
            .into_iter()
            .map(|t| ASensorManager_getDefaultSensor(manager, t))
            .filter(|s| !s.is_null())
            .collect();
        if sensors.is_empty() {
            log::warn!("motion sensors: none available");
        }
        for &s in &sensors {
            ASensorEventQueue_enableSensor(queue, s);
            ASensorEventQueue_setEventRate(queue, s, PERIOD_US);
        }
        let mut motion = Motion { rate: [0.0; 3], gravity: [0.0, 0.0, -9.80665] };
        let mut events: [ASensorEvent; 16] = std::mem::zeroed();
        while !shared.stop.load(Ordering::Relaxed) {
            // Wait for events, waking now and then to notice a stop request.
            ALooper_pollOnce(100, ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
            let mut seen = false;
            loop {
                let n = ASensorEventQueue_getEvents(queue, events.as_mut_ptr(), events.len());
                if n <= 0 {
                    break;
                }
                for e in &events[..n as usize] {
                    // SAFETY: both sensor types report a 3-vector.
                    let v = e.__bindgen_anon_1.__bindgen_anon_1.vector.__bindgen_anon_1.v;
                    match e.type_ {
                        TYPE_GYROSCOPE => motion.rate = v,
                        TYPE_ACCELEROMETER => {
                            // The accelerometer reads opposite to gravity (+9.8 on z when flat).
                            for (g, a) in motion.gravity.iter_mut().zip(v) {
                                *g += (-a - *g) * SMOOTHING;
                            }
                        }
                        _ => continue,
                    }
                    seen = true;
                }
            }
            if seen && let Ok(mut m) = shared.motion.lock() {
                *m = Some(motion);
            }
        }
        for &s in &sensors {
            ASensorEventQueue_disableSensor(queue, s);
        }
        ASensorManager_destroyEventQueue(manager, queue);
    }
}

impl Sensors {
    pub fn new() -> Option<Sensors> {
        Some(Sensors { shared: Arc::new(Shared::default()), thread: None })
    }

    pub fn set(&mut self, on: bool) {
        if on && self.thread.is_none() {
            self.shared.stop.store(false, Ordering::Relaxed);
            let shared = self.shared.clone();
            self.thread =
                std::thread::Builder::new().name("kulisa-sensors".into()).spawn(move || run(shared)).ok();
        } else if !on && let Some(t) = self.thread.take() {
            self.shared.stop.store(true, Ordering::Relaxed);
            let _ = t.join();
            if let Ok(mut m) = self.shared.motion.lock() {
                *m = None;
            }
        }
    }

    /// The latest reading, once the sensors have produced one.
    pub fn poll(&mut self) -> Option<Motion> {
        self.thread.as_ref()?;
        self.shared.motion.lock().ok().and_then(|m| *m)
    }
}

impl Drop for Sensors {
    fn drop(&mut self) {
        self.set(false);
    }
}
