//! Android activity calls through JNI: orientation lock and system-bar insets.

use std::ffi::{CStr, c_void};

use jni_sys::{JNI_OK, JNIEnv, JavaVM, jclass, jint, jmethodID, jobject, jvalue};
use kulisa_lua::Orientation;
use winit::platform::android::activity::AndroidApp;

/// A JNI environment for the current thread. Every call checks for and clears exceptions,
/// so a missing method (older Android) just returns `None`.
struct Env {
    env: *mut JNIEnv,
}

impl Env {
    fn attach(app: &AndroidApp) -> Option<Env> {
        let vm = app.vm_as_ptr() as *mut JavaVM;
        let mut env: *mut JNIEnv = std::ptr::null_mut();
        // SAFETY: android-activity hands out the process's valid JavaVM pointer.
        let ok = unsafe {
            let attach = (**vm).AttachCurrentThread?;
            attach(vm, &mut env as *mut _ as *mut *mut c_void, std::ptr::null_mut())
        };
        (ok == JNI_OK && !env.is_null()).then_some(Env { env })
    }

    /// Clears a pending exception; returns whether there was one.
    fn failed(&self) -> bool {
        // SAFETY: `env` is attached to this thread.
        unsafe {
            let e = &**self.env;
            if (e.ExceptionCheck.unwrap())(self.env) != 0 {
                (e.ExceptionClear.unwrap())(self.env);
                return true;
            }
        }
        false
    }

    fn method(&self, obj: jobject, name: &CStr, sig: &CStr) -> Option<(jclass, jmethodID)> {
        // SAFETY: `obj` is a live reference; the class reference is freed by the caller.
        unsafe {
            let e = &**self.env;
            let class = (e.GetObjectClass?)(self.env, obj);
            let m = (e.GetMethodID?)(self.env, class, name.as_ptr(), sig.as_ptr());
            if self.failed() || m.is_null() {
                (e.DeleteLocalRef?)(self.env, class);
                return None;
            }
            Some((class, m))
        }
    }

    fn drop_ref(&self, r: jobject) {
        if !r.is_null() {
            // SAFETY: `r` is a local reference made on this thread.
            unsafe { ((**self.env).DeleteLocalRef.unwrap())(self.env, r) }
        }
    }

    fn call_object(&self, obj: jobject, name: &CStr, sig: &CStr, args: &[jvalue]) -> Option<jobject> {
        let (class, m) = self.method(obj, name, sig)?;
        // SAFETY: `m` belongs to `obj`'s class and `args` match `sig`.
        let r = unsafe { ((**self.env).CallObjectMethodA?)(self.env, obj, m, args.as_ptr()) };
        self.drop_ref(class);
        (!self.failed() && !r.is_null()).then_some(r)
    }

    fn call_int(&self, obj: jobject, name: &CStr) -> Option<jint> {
        let (class, m) = self.method(obj, name, c"()I")?;
        // SAFETY: as above, for a method taking no arguments.
        let r = unsafe { ((**self.env).CallIntMethodA?)(self.env, obj, m, std::ptr::null()) };
        self.drop_ref(class);
        (!self.failed()).then_some(r)
    }

    fn call_void(&self, obj: jobject, name: &CStr, sig: &CStr, args: &[jvalue]) -> Option<()> {
        let (class, m) = self.method(obj, name, sig)?;
        // SAFETY: as above.
        unsafe { ((**self.env).CallVoidMethodA?)(self.env, obj, m, args.as_ptr()) };
        self.drop_ref(class);
        (!self.failed()).then_some(())
    }

    fn static_int(&self, class: &CStr, name: &CStr) -> Option<jint> {
        // SAFETY: class and method names are valid C strings; references are freed.
        unsafe {
            let e = &**self.env;
            let c = (e.FindClass?)(self.env, class.as_ptr());
            if self.failed() || c.is_null() {
                return None;
            }
            let m = (e.GetStaticMethodID?)(self.env, c, name.as_ptr(), c"()I".as_ptr());
            if self.failed() || m.is_null() {
                self.drop_ref(c);
                return None;
            }
            let r = (e.CallStaticIntMethodA?)(self.env, c, m, std::ptr::null());
            self.drop_ref(c);
            (!self.failed()).then_some(r)
        }
    }

    fn int_field(&self, obj: jobject, name: &CStr) -> Option<jint> {
        // SAFETY: `obj` is live; the field is a public int.
        unsafe {
            let e = &**self.env;
            let class = (e.GetObjectClass?)(self.env, obj);
            let f = (e.GetFieldID?)(self.env, class, name.as_ptr(), c"I".as_ptr());
            self.drop_ref(class);
            if self.failed() || f.is_null() {
                return None;
            }
            let v = (e.GetIntField?)(self.env, obj, f);
            (!self.failed()).then_some(v)
        }
    }
}

/// Locks the activity's orientation (`setRequestedOrientation`).
pub fn set_orientation(app: &AndroidApp, o: Orientation) {
    // ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE / SENSOR_PORTRAIT / FULL_SENSOR.
    let value = match o {
        Orientation::Landscape => 6,
        Orientation::Portrait => 7,
        _ => 10,
    };
    let Some(env) = Env::attach(app) else { return log::warn!("orientation: cannot attach to the JVM") };
    let activity = app.activity_as_ptr() as jobject;
    if env.call_void(activity, c"setRequestedOrientation", c"(I)V", &[jvalue { i: value }]).is_none() {
        log::warn!("orientation: setRequestedOrientation failed");
    }
}

/// The window's system-bar and display-cutout insets in pixels (left, top, right, bottom).
/// Android 15 draws apps edge to edge, under the status and navigation bars.
pub fn insets(app: &AndroidApp) -> Option<[f32; 4]> {
    let env = Env::attach(app)?;
    let activity = app.activity_as_ptr() as jobject;
    let window = env.call_object(activity, c"getWindow", c"()Landroid/view/Window;", &[])?;
    let view = env.call_object(window, c"getDecorView", c"()Landroid/view/View;", &[]);
    env.drop_ref(window);
    let view = view?;
    let root = env.call_object(view, c"getRootWindowInsets", c"()Landroid/view/WindowInsets;", &[]);
    env.drop_ref(view);
    let root = root?;
    // API 30+: insets by type; older: the system window insets.
    let types = env
        .static_int(c"android/view/WindowInsets$Type", c"systemBars")
        .zip(env.static_int(c"android/view/WindowInsets$Type", c"displayCutout"))
        .map(|(a, b)| a | b);
    let result = match types.and_then(|t| {
        env.call_object(root, c"getInsets", c"(I)Landroid/graphics/Insets;", &[jvalue { i: t }])
    }) {
        Some(ins) => {
            let v = [c"left", c"top", c"right", c"bottom"].map(|n| env.int_field(ins, n).unwrap_or(0) as f32);
            env.drop_ref(ins);
            Some(v)
        }
        None => {
            let names = [
                c"getSystemWindowInsetLeft",
                c"getSystemWindowInsetTop",
                c"getSystemWindowInsetRight",
                c"getSystemWindowInsetBottom",
            ];
            Some(names.map(|n| env.call_int(root, n).unwrap_or(0) as f32))
        }
    };
    env.drop_ref(root);
    result
}
