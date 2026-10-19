//! Frame loop. Owns the window, finds the game, creates the renderer and forwards frames and
//! input to the scripted [`Game`].

use std::sync::Arc;

use kulisa_core::Vec2;
#[cfg(any(target_os = "android", target_os = "ios"))]
use kulisa_lua::Orientation;
use kulisa_lua::{Device, FormFactor, Game};
use kulisa_platform::Assets;
use kulisa_render::Renderer;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[cfg(target_os = "android")]
pub use winit::platform::android::activity::AndroidApp;

pub enum UserEvent {
    RendererReady(Result<Box<Renderer>, String>),
    GameLoaded(Result<Assets, String>),
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    window: Option<Arc<Window>>,
    /// The game's files, until the renderer is ready too.
    assets: Option<Result<Assets, String>>,
    /// The renderer, until the game takes it.
    renderer: Option<Renderer>,
    creating_renderer: bool,
    game: Option<Game>,
    last_frame: Instant,
    #[cfg(target_os = "android")]
    android: AndroidApp,
}

/// The drawable size. On iOS winit's inner size is the safe area, but we draw everywhere.
fn drawable_size(w: &Window) -> winit::dpi::PhysicalSize<u32> {
    if cfg!(target_os = "ios") { w.outer_size() } else { w.inner_size() }
}

/// Form factor and platform abilities, for the game's display profile.
fn device(w: &Window) -> Device {
    let mobile = cfg!(any(target_os = "android", target_os = "ios"));
    #[cfg(target_arch = "wasm32")]
    let touch = kulisa_platform::web_touch();
    #[cfg(not(target_arch = "wasm32"))]
    let touch = mobile;
    let size = drawable_size(w);
    let logical = Vec2::new(size.width as f32, size.height as f32) / w.scale_factor() as f32;
    let detected = if mobile || cfg!(target_arch = "wasm32") {
        FormFactor::detect(touch, logical)
    } else {
        FormFactor::Desktop
    };
    let form =
        kulisa_platform::profile_override().and_then(|p| FormFactor::from_name(&p)).unwrap_or(detected);
    Device { form, touch, can_lock: mobile, can_quit: !mobile && !cfg!(target_arch = "wasm32") }
}

/// Key names as scripts see them: "space", "enter", "left", "f5", "a", ...
fn key_name(key: &Key) -> Option<String> {
    const NAMED: &[(NamedKey, &str)] = &[
        (NamedKey::Space, "space"),
        (NamedKey::Enter, "enter"),
        (NamedKey::Escape, "escape"),
        (NamedKey::Tab, "tab"),
        (NamedKey::Backspace, "backspace"),
        (NamedKey::Delete, "delete"),
        (NamedKey::ArrowLeft, "left"),
        (NamedKey::ArrowRight, "right"),
        (NamedKey::ArrowUp, "up"),
        (NamedKey::ArrowDown, "down"),
        (NamedKey::PageUp, "pageup"),
        (NamedKey::PageDown, "pagedown"),
        (NamedKey::Home, "home"),
        (NamedKey::End, "end"),
        (NamedKey::Shift, "shift"),
        (NamedKey::Control, "control"),
        (NamedKey::Alt, "alt"),
        (NamedKey::GoBack, "back"),
        (NamedKey::BrowserBack, "back"),
        (NamedKey::F1, "f1"),
        (NamedKey::F2, "f2"),
        (NamedKey::F3, "f3"),
        (NamedKey::F4, "f4"),
        (NamedKey::F5, "f5"),
        (NamedKey::F6, "f6"),
        (NamedKey::F7, "f7"),
        (NamedKey::F8, "f8"),
        (NamedKey::F9, "f9"),
        (NamedKey::F10, "f10"),
        (NamedKey::F11, "f11"),
        (NamedKey::F12, "f12"),
    ];
    match key {
        Key::Named(n) => NAMED.iter().find(|(k, _)| k == n).map(|(_, s)| s.to_string()),
        Key::Character(c) if c.as_str() == " " => Some("space".into()),
        Key::Character(c) => Some(c.to_lowercase()),
        _ => None,
    }
}

impl App {
    fn new(
        proxy: EventLoopProxy<UserEvent>,
        assets: Option<Result<Assets, String>>,
        #[cfg(target_os = "android")] android: AndroidApp,
    ) -> Self {
        Self {
            proxy,
            window: None,
            assets,
            renderer: None,
            creating_renderer: false,
            game: None,
            last_frame: Instant::now(),
            #[cfg(target_os = "android")]
            android,
        }
    }

    /// Window insets in pixels: notches, status and navigation bars.
    fn safe_area(&self) -> [f32; 4] {
        if let Some(o) = kulisa_platform::safe_area_override() {
            return o;
        }
        #[cfg(target_os = "android")]
        if let Some(w) = &self.window {
            let r = self.android.content_rect();
            let s = w.inner_size();
            let (l, t) = (r.left.max(0) as f32, r.top.max(0) as f32);
            let (rr, b) =
                ((s.width as i32 - r.right).max(0) as f32, (s.height as i32 - r.bottom).max(0) as f32);
            // An empty rect means "not reported yet".
            if r.right > r.left && r.bottom > r.top {
                return [l, t, rr, b];
            }
        }
        #[cfg(target_os = "ios")]
        if let Some(w) = &self.window
            && let Ok(p) = w.inner_position()
        {
            let (inner, outer) = (w.inner_size(), w.outer_size());
            let (l, t) = (p.x.max(0) as f32, p.y.max(0) as f32);
            let r = (outer.width as f32 - l - inner.width as f32).max(0.0);
            let b = (outer.height as f32 - t - inner.height as f32).max(0.0);
            return [l, t, r, b];
        }
        #[cfg(target_arch = "wasm32")]
        return kulisa_platform::web_safe_area();
        #[allow(unreachable_code)]
        [0.0; 4]
    }

    /// Does what scripts asked for: orientation lock, window shape, fullscreen, quit.
    fn apply_requests(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(game), Some(window)) = (&mut self.game, &self.window) else { return };
        let r = game.take_requests();
        #[cfg(any(target_os = "android", target_os = "ios"))]
        if let Some(o) = r.orientation {
            #[cfg(target_os = "android")]
            android_orientation(&self.android, o);
            #[cfg(target_os = "ios")]
            {
                use winit::platform::ios::{ValidOrientations, WindowExtIOS};
                window.set_valid_orientations(match o {
                    Orientation::Landscape => ValidOrientations::Landscape,
                    Orientation::Portrait => ValidOrientations::Portrait,
                    _ => ValidOrientations::LandscapeAndPortrait,
                });
            }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
        if let Some(aspect) = r.window_aspect
            && window.fullscreen().is_none()
        {
            let aspect = aspect as f64;
            let (w, h) = if aspect >= 1.0 { (1280.0, 1280.0 / aspect) } else { (900.0 * aspect, 900.0) };
            let fit = window.current_monitor().map_or(1.0, |m| {
                let s = m.size().to_logical::<f64>(m.scale_factor());
                (s.width * 0.85 / w).min(s.height * 0.85 / h).min(1.0)
            });
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(w * fit, h * fit));
        }
        if let Some(on) = r.fullscreen {
            window.set_fullscreen(on.then_some(winit::window::Fullscreen::Borderless(None)));
            game.set_fullscreen(on);
        }
        if r.quit && !cfg!(any(target_os = "android", target_os = "ios", target_arch = "wasm32")) {
            event_loop.exit();
        }
    }

    fn create_renderer(&mut self, window: Arc<Window>) {
        self.creating_renderer = true;
        let size = drawable_size(&window);
        let proxy = self.proxy.clone();
        let task = async move {
            let result = Renderer::new(window, (size.width, size.height)).await.map(Box::new);
            let _ = proxy.send_event(UserEvent::RendererReady(result));
        };
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);
        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(task);
    }

    /// Starts the game once both its files and the renderer are ready.
    fn start_game(&mut self) {
        if self.assets.is_none() || self.renderer.is_none() {
            return;
        }
        let renderer = self.renderer.take().unwrap();
        let device = self.window.as_deref().map(device).unwrap_or_default();
        let started =
            self.assets.take().unwrap().and_then(|assets| Game::with_device(assets, renderer, device));
        match started {
            Ok(mut game) => {
                game.set_safe_area(self.safe_area());
                if let Some(w) = &self.window {
                    let size = drawable_size(w);
                    game.resize(size.width, size.height);
                    w.request_redraw();
                }
                self.last_frame = Instant::now();
                self.game = Some(game);
            }
            Err(e) => log::error!("the game failed to start:\n{e}"),
        }
    }

    /// Browsers allow audio only after a user gesture.
    fn input(&mut self) -> Option<&mut Game> {
        let game = self.game.as_mut()?;
        if cfg!(target_arch = "wasm32") {
            game.start_audio();
        }
        Some(game)
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = match &self.window {
            Some(w) => w.clone(),
            None => {
                let attrs = Window::default_attributes().with_title("Kulisa");
                // On the web the page's CSS sizes the canvas.
                #[cfg(not(target_arch = "wasm32"))]
                let attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
                #[cfg(target_arch = "wasm32")]
                let attrs = {
                    use winit::platform::web::WindowAttributesExtWebSys;
                    attrs.with_append(true)
                };
                let w = Arc::new(event_loop.create_window(attrs).expect("failed to create window"));
                self.window = Some(w.clone());
                w
            }
        };
        // Android recreates the native window after a suspend; the GPU device stays.
        let size = drawable_size(&window);
        let resumed = match (&mut self.game, &mut self.renderer) {
            (Some(game), _) => Some(game.resume(window.clone(), size.width, size.height)),
            (None, Some(r)) if r.is_suspended() => Some(r.resume(window.clone(), size.width, size.height)),
            _ => None,
        };
        match resumed {
            Some(Err(e)) => log::error!("resuming the renderer: {e}"),
            Some(Ok(())) => window.request_redraw(),
            None if self.renderer.is_none() && !self.creating_renderer => self.create_renderer(window),
            None => {}
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Android destroys the native window on suspend; the surface must go with it.
        if cfg!(target_os = "android") {
            if let Some(game) = &mut self.game {
                game.suspend();
            } else if let Some(r) = &mut self.renderer {
                r.suspend();
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::RendererReady(Ok(mut r)) => {
                self.creating_renderer = false;
                if let Some(w) = &self.window {
                    let size = drawable_size(w);
                    r.resize(size.width, size.height);
                }
                self.renderer = Some(*r);
                self.start_game();
            }
            UserEvent::RendererReady(Err(e)) => {
                self.creating_renderer = false;
                log::error!("renderer: {e}");
            }
            UserEvent::GameLoaded(assets) => {
                self.assets = Some(assets);
                self.start_game();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let input = !matches!(event, WindowEvent::RedrawRequested | WindowEvent::Resized(_));
        self.handle(event_loop, event);
        // Browsers allow fullscreen only while handling the input that asked for it.
        if input {
            self.apply_requests(event_loop);
        }
    }
}

impl App {
    fn handle(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                let size = self.window.as_deref().map(drawable_size).unwrap_or_default();
                let safe = self.safe_area();
                if let Some(game) = &mut self.game {
                    game.set_safe_area(safe);
                    game.resize(size.width, size.height);
                } else if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(game) = &mut self.game {
                    game.pointer_moved(Some(Vec2::new(position.x as f32, position.y as f32)));
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if let Some(game) = &mut self.game {
                    game.pointer_moved(None);
                }
            }
            WindowEvent::Touch(touch) => {
                let Some(game) = self.input() else { return };
                game.pointer_moved(Some(Vec2::new(touch.location.x as f32, touch.location.y as f32)));
                match touch.phase {
                    TouchPhase::Started => game.pressed("touch"),
                    TouchPhase::Ended | TouchPhase::Cancelled => game.released("touch"),
                    TouchPhase::Moved => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let name = match button {
                    MouseButton::Left => "left",
                    MouseButton::Right => "right",
                    MouseButton::Middle => "middle",
                    _ => return,
                };
                let Some(game) = self.input() else { return };
                match state {
                    ElementState::Pressed => game.pressed(name),
                    ElementState::Released => game.released(name),
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if let Some(game) = &mut self.game {
                    game.wheel(lines);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if !event.repeat => {
                let Some(name) = key_name(&event.logical_key) else { return };
                let Some(game) = self.input() else { return };
                game.key(&name, event.state == ElementState::Pressed);
            }
            WindowEvent::RedrawRequested => {
                let Some(game) = &mut self.game else { return };
                let now = Instant::now();
                game.frame((now - self.last_frame).as_secs_f32());
                self.last_frame = now;
                if let Some(w) = &self.window {
                    w.pre_present_notify();
                }
                game.render();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
                self.apply_requests(event_loop);
            }
            _ => {}
        }
    }
}

/// Locks the activity's orientation (`setRequestedOrientation`) through JNI.
#[cfg(target_os = "android")]
fn android_orientation(app: &AndroidApp, o: Orientation) {
    use jni_sys::{JNI_OK, JNIEnv, JavaVM, jvalue};
    // ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE / SENSOR_PORTRAIT / FULL_SENSOR.
    let value = match o {
        Orientation::Landscape => 6,
        Orientation::Portrait => 7,
        _ => 10,
    };
    let vm = app.vm_as_ptr() as *mut JavaVM;
    let activity = app.activity_as_ptr() as jni_sys::jobject;
    // SAFETY: android-activity hands out valid VM and activity pointers for the app's
    // lifetime; every JNI function is checked before use and exceptions are cleared.
    unsafe {
        let mut env: *mut JNIEnv = std::ptr::null_mut();
        let Some(attach) = (**vm).AttachCurrentThread else { return };
        if attach(vm, &mut env as *mut _ as *mut *mut std::ffi::c_void, std::ptr::null_mut()) != JNI_OK {
            return log::warn!("orientation: cannot attach to the JVM");
        }
        let e = &**env;
        let (Some(get_class), Some(get_method), Some(call), Some(check), Some(clear), Some(delete)) = (
            e.GetObjectClass,
            e.GetMethodID,
            e.CallVoidMethodA,
            e.ExceptionCheck,
            e.ExceptionClear,
            e.DeleteLocalRef,
        ) else {
            return;
        };
        let class = get_class(env, activity);
        let method = get_method(env, class, c"setRequestedOrientation".as_ptr(), c"(I)V".as_ptr());
        if !method.is_null() {
            call(env, activity, method, &jvalue { i: value });
        }
        if check(env) != 0 {
            clear(env);
            log::warn!("orientation: setRequestedOrientation failed");
        }
        delete(env, class);
    }
}

fn run_with(
    event_loop: EventLoop<UserEvent>,
    assets: Option<Result<Assets, String>>,
    #[cfg(target_os = "android")] android: AndroidApp,
) {
    let proxy = event_loop.create_proxy();
    #[cfg(target_arch = "wasm32")]
    {
        let proxy = proxy.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let assets = kulisa_platform::fetch("game.kpak").await.and_then(Assets::pack);
            let _ = proxy.send_event(UserEvent::GameLoaded(assets));
        });
    }
    #[cfg(target_os = "android")]
    let app = App::new(proxy, assets, android);
    #[cfg(not(target_os = "android"))]
    let app = App::new(proxy, assets);
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut app = app;
        if let Err(e) = event_loop.run_app(&mut app) {
            log::error!("event loop: {e}");
        }
    }
}

/// The game folder or `.kpak`: the first argument, `KULISA_GAME`, or `game/` or `game.kpak`
/// next to the executable (an iOS app bundle) or in the working directory.
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
fn find_game() -> Result<Assets, String> {
    use std::path::PathBuf;
    let open = |p: PathBuf| {
        if p.is_dir() {
            Ok(Assets::dir(p))
        } else {
            Assets::pack(std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?)
        }
    };
    if let Some(p) = std::env::args_os().nth(1).or_else(|| std::env::var_os("KULISA_GAME")) {
        return open(p.into());
    }
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from));
    for dir in exe_dir.into_iter().chain([PathBuf::from(".")]) {
        for name in ["game", "game.kpak"] {
            if dir.join(name).exists() {
                return open(dir.join(name));
            }
        }
    }
    Err("no game found; pass a game folder or .kpak, like `kulisa-player demos/smoke`".into())
}

/// Runs the engine on desktop, iOS and web.
#[cfg(not(target_os = "android"))]
pub fn run() {
    kulisa_platform::init_logging();
    let event_loop = EventLoop::with_user_event().build().expect("failed to create event loop");
    // The web fetches the game while the renderer starts.
    #[cfg(target_arch = "wasm32")]
    let assets = None;
    #[cfg(not(target_arch = "wasm32"))]
    let assets = Some(find_game());
    run_with(event_loop, assets);
}

/// Runs the engine inside an Android `NativeActivity`, with the game in the APK's
/// `assets/game.kpak`.
#[cfg(target_os = "android")]
pub fn run_android(app: AndroidApp) {
    use std::io::Read;
    use winit::platform::android::EventLoopBuilderExtAndroid;
    kulisa_platform::init_logging();
    if let Some(dir) = app.internal_data_path() {
        kulisa_platform::set_android_data_dir(dir);
    }
    let assets = app
        .asset_manager()
        .open(c"game.kpak")
        .ok_or_else(|| "the APK has no assets/game.kpak".to_string())
        .and_then(|mut a| {
            let mut bytes = Vec::new();
            a.read_to_end(&mut bytes).map_err(|e| format!("reading game.kpak: {e}"))?;
            Assets::pack(bytes)
        });
    let event_loop = EventLoop::with_user_event()
        .with_android_app(app.clone())
        .build()
        .expect("failed to create event loop");
    run_with(event_loop, Some(assets), app);
}
