//! Frame loop. Owns the window, finds the game, creates the renderer and forwards frames and
//! input to the scripted [`Game`].

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod android_sensors;
mod gamepad;
#[cfg(target_os = "ios")]
mod ios;
mod sensors;
#[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
mod watch;
#[cfg(target_arch = "wasm32")]
mod web_dev;

use std::sync::Arc;

use kulisa_core::Vec2;
#[cfg(any(target_os = "ios", target_arch = "wasm32"))]
use kulisa_lua::Orientation;
use kulisa_lua::{Device, FormFactor, Game};
use kulisa_platform::{Assets, GameConfig};
use kulisa_render::Renderer;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[cfg(target_os = "android")]
pub use winit::platform::android::activity::AndroidApp;

pub enum UserEvent {
    RendererReady(Result<Box<Renderer>, String>),
    GameLoaded(Result<Assets, String>),
    /// Web development runs: the dev server's game version, with the new pack when it changed.
    #[cfg(target_arch = "wasm32")]
    DevVersion(String, Option<Result<Assets, String>>),
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
    pads: gamepad::Pads,
    sensors: sensors::Sensors,
    /// The on-screen keyboard's height in window pixels, as last read.
    keyboard: f32,
    last_frame: Instant,
    /// The game's files, kept for restarts (live reload).
    files: Option<Assets>,
    /// The game's `kulisa.toml` settings, once its files are known.
    config: Option<GameConfig>,
    /// Development run (`--dev`, `kulisa run`): live reload and every error on screen.
    dev: bool,
    /// Set while restarting for a reload: whether the story was saved for the new game.
    restart_saved: Option<bool>,
    /// The window took the design's shape (once: restarts keep the size the player chose).
    shaped: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    watcher: Option<watch::Watcher>,
    #[cfg(target_arch = "wasm32")]
    web_dev: web_dev::Poll,
    /// The orientation the game wants locked (`landscape`/`portrait`); browsers allow it only
    /// in fullscreen, so it is applied again whenever the page gets there.
    #[cfg(target_arch = "wasm32")]
    web_lock: Option<&'static str>,
    #[cfg(target_os = "android")]
    android: AndroidApp,
}

/// The game's icon for its window, if it has a readable one.
#[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
fn window_icon(files: &Assets, path: &str) -> Option<winit::window::Icon> {
    if !files.exists(path) {
        return None;
    }
    let icon = files.read(path).and_then(|b| kulisa_render::Image::decode_icon(&b, 256));
    match icon.and_then(|(w, h, rgba)| winit::window::Icon::from_rgba(rgba, w, h).map_err(|e| e.to_string()))
    {
        Ok(icon) => Some(icon),
        Err(e) => {
            log::warn!("icon `{path}`: {e}");
            None
        }
    }
}

/// The game's settings for the window; errors are reported by the game itself.
fn read_config(assets: &Assets) -> Option<GameConfig> {
    let source = assets.read_string(kulisa_platform::CONFIG_FILE).ok();
    GameConfig::parse(source.as_deref()).ok()
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
    Device {
        form,
        touch,
        can_lock: mobile,
        can_quit: !mobile && !cfg!(target_arch = "wasm32"),
        sensors: sensors::available(touch),
        haptics: mobile || cfg!(target_arch = "wasm32"),
    }
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
        dev: bool,
        #[cfg(target_os = "android")] android: AndroidApp,
    ) -> Self {
        #[cfg(target_os = "ios")]
        ios::observe_keyboard();
        let config = assets.as_ref().and_then(|a| a.as_ref().ok()).and_then(read_config);
        Self {
            proxy,
            window: None,
            assets,
            renderer: None,
            creating_renderer: false,
            game: None,
            pads: gamepad::Pads::new(),
            sensors: sensors::Sensors::new(),
            keyboard: 0.0,
            last_frame: Instant::now(),
            files: None,
            config,
            dev,
            restart_saved: None,
            shaped: false,
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            watcher: None,
            #[cfg(target_arch = "wasm32")]
            web_dev: Default::default(),
            #[cfg(target_arch = "wasm32")]
            web_lock: None,
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
        if let Some(insets) = android::insets(&self.android) {
            return insets;
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
        // Sensors and vibration are asked for while handling input, which browsers require
        // (iOS asks permission for motion only in reply to a tap, `vibrate` only after one).
        if let Some(on) = game.take_sensor_request() {
            self.sensors.set(on);
        }
        for (strength, secs) in game.take_haptics() {
            #[cfg(target_os = "android")]
            android::vibrate(&self.android, strength, secs);
            #[cfg(target_os = "ios")]
            ios::impact(strength);
            #[cfg(target_arch = "wasm32")]
            kulisa_platform::web_vibrate((secs * 1000.0) as u32);
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            self.pads.rumble(strength, secs);
            let _ = (strength, secs);
        }
        let r = game.take_requests();
        if let Some(on) = game.take_ime_request() {
            window.set_ime_allowed(on);
            // Touch browsers show a soft keyboard only for a focused input element.
            #[cfg(target_arch = "wasm32")]
            if kulisa_platform::web_touch() {
                kulisa_platform::web_text_focus(on);
            }
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        if let Some(o) = r.orientation {
            #[cfg(target_os = "android")]
            android::set_orientation(&self.android, o);
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
        #[cfg(target_arch = "wasm32")]
        if let Some(o) = r.orientation {
            self.web_lock = match o {
                Orientation::Landscape => Some("landscape"),
                Orientation::Portrait => Some("portrait"),
                _ => None,
            };
            if let Some(lock) = self.web_lock {
                kulisa_platform::web_lock_orientation(lock);
            }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
        if let Some(aspect) = r.window_aspect
            && window.fullscreen().is_none()
            && !std::mem::replace(&mut self.shaped, true)
            && self.config.as_ref().is_none_or(|c| c.window.is_none())
        {
            let aspect = aspect as f64;
            let (w, h) = if aspect >= 1.0 { (1280.0, 1280.0 / aspect) } else { (900.0 * aspect, 900.0) };
            let fit = window.current_monitor().map_or(1.0, |m| {
                let s = m.size().to_logical::<f64>(m.scale_factor());
                (s.width * 0.85 / w).min(s.height * 0.85 / h).min(1.0)
            });
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(w * fit, h * fit));
        }
        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
        if let Some((w, h)) = r.window_size
            && window.fullscreen().is_none()
        {
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(w as f64, h as f64));
        }
        if let Some(name) = r.cursor {
            use winit::window::CursorIcon as C;
            window.set_cursor_visible(name != "hidden");
            let icon = match name {
                "pointer" => C::Pointer,
                "text" => C::Text,
                "move" => C::Move,
                "grab" => C::Grab,
                "grabbing" => C::Grabbing,
                "crosshair" => C::Crosshair,
                "wait" => C::Wait,
                "progress" => C::Progress,
                "help" => C::Help,
                "not_allowed" => C::NotAllowed,
                "ew_resize" => C::EwResize,
                "ns_resize" => C::NsResize,
                "nwse_resize" => C::NwseResize,
                "nesw_resize" => C::NeswResize,
                "zoom_in" => C::ZoomIn,
                "zoom_out" => C::ZoomOut,
                _ => C::Default,
            };
            window.set_cursor(icon);
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
        // A game that cannot be found still gets a window that says so.
        let (assets, missing) = match self.assets.take().unwrap() {
            Ok(a) => (a, None),
            Err(e) => (Assets::empty(), Some(e)),
        };
        self.files = Some(assets.clone());
        #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
        if self.dev
            && self.watcher.is_none()
            && let Assets::Dir(dir) = &assets
        {
            log::info!("live reload: watching {}", dir.display());
            self.watcher = Some(watch::Watcher::new(dir.clone()));
        }
        let mut game = Game::start(assets, renderer, device, self.dev);
        if let Some(e) = missing {
            game.clear_errors();
            game.show_error(&e);
        }
        let config = game.config();
        game.set_safe_area(self.safe_area());
        if let Some(w) = &self.window {
            w.set_title(&config.title);
            // Desktop windows (title bar, taskbar). Phones and the web take theirs from the
            // package `kulisa build` makes.
            #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
            if let Some(files) = &self.files {
                w.set_window_icon(window_icon(files, &config.icon));
            }
            let size = drawable_size(w);
            game.resize(size.width, size.height);
            w.request_redraw();
        }
        self.config = Some(config);
        if let Some(saved) = self.restart_saved.take() {
            game.restarted(saved);
        }
        self.last_frame = Instant::now();
        self.game = Some(game);
    }

    /// Development runs: applies changed files, or restarts the game when they need it
    /// (or the player pressed Shift+R).
    fn live_reload(&mut self, changed: Vec<String>) {
        let Some(game) = &mut self.game else { return };
        let forced = game.take_reload_request();
        if !forced && changed.is_empty() {
            return;
        }
        if !forced && !game.reload(&changed) {
            return;
        }
        log::info!("live reload: restarting the game");
        let mut game = self.game.take().unwrap();
        self.restart_saved = Some(game.save_for_restart());
        drop(game);
        self.assets = self.files.take().map(Ok);
        if let Some(w) = self.window.clone() {
            self.create_renderer(w);
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
                let config = self.config.clone().unwrap_or_else(|| GameConfig::parse(None).unwrap());
                let attrs = Window::default_attributes().with_title(&config.title);
                // On the web the page's CSS sizes the canvas.
                #[cfg(not(target_arch = "wasm32"))]
                let attrs = {
                    let (w, h) = config.window.unwrap_or((1280.0, 720.0));
                    let full = config.fullscreen && !cfg!(any(target_os = "android", target_os = "ios"));
                    attrs
                        .with_inner_size(winit::dpi::LogicalSize::new(w, h))
                        .with_fullscreen(full.then_some(winit::window::Fullscreen::Borderless(None)))
                };
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
            #[cfg(target_arch = "wasm32")]
            UserEvent::DevVersion(version, pack) => {
                let changed = self.web_dev.arrived(version, pack, self.files.as_ref());
                if let Some((assets, changed)) = changed {
                    if let Some(game) = &mut self.game {
                        game.set_assets(assets.clone());
                    }
                    self.files = Some(assets);
                    self.live_reload(changed);
                }
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
                // The page just went fullscreen (or rotated in it): lock the orientation, which
                // browsers only allow now. A lock that holds already changes nothing.
                #[cfg(target_arch = "wasm32")]
                if let Some(lock) = self.web_lock.filter(|_| kulisa_platform::web_fullscreen()) {
                    kulisa_platform::web_lock_orientation(lock);
                }
                if let Some(game) = &mut self.game {
                    game.set_safe_area(safe);
                    // The page enters fullscreen on a phone's first tap, and players leave it
                    // with the back gesture or Esc, without the game asking.
                    #[cfg(target_arch = "wasm32")]
                    game.set_fullscreen(kulisa_platform::web_fullscreen());
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
                // Pointer 0 is the mouse; each finger gets its own id from 1.
                let id = (touch.id as u32).wrapping_add(1).max(1);
                game.pointer_moved_id(id, Some(Vec2::new(touch.location.x as f32, touch.location.y as f32)));
                match touch.phase {
                    TouchPhase::Started => game.pressed_id(id, "touch"),
                    TouchPhase::Ended | TouchPhase::Cancelled => game.released_id(id, "touch"),
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
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let name = key_name(&event.logical_key);
                let Some(game) = self.input() else { return };
                if event.repeat {
                    // Held keys repeat for text fields (backspace, arrows, letters).
                    if let Some(name) = &name {
                        game.key_repeat(name);
                    }
                } else if let Some(name) = &name {
                    game.key(name, pressed);
                }
                // Committed text: what the key typed, without control characters (Enter,
                // Backspace and Ctrl combinations are keys, not text).
                if pressed
                    && let Some(text) = &event.text
                    && !text.chars().any(char::is_control)
                {
                    game.text_input(text);
                }
            }
            WindowEvent::Ime(ime) => {
                let Some(game) = self.input() else { return };
                match ime {
                    Ime::Commit(text) => game.text_input(&text),
                    Ime::Preedit(text, cursor) => game.ime_preedit(&text, cursor),
                    Ime::Enabled | Ime::Disabled => {}
                }
            }
            WindowEvent::RedrawRequested => {
                #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
                let changed = self.watcher.as_mut().map(|w| w.poll()).unwrap_or_default();
                #[cfg(any(target_os = "android", target_os = "ios", target_arch = "wasm32"))]
                let changed = Vec::new();
                #[cfg(target_arch = "wasm32")]
                if self.dev {
                    self.web_dev.tick(&self.proxy);
                }
                self.live_reload(changed);
                let Some(game) = &mut self.game else { return };
                self.pads.poll(game);
                // The keyboard's height, read only while it can be up (a JNI call on Android).
                if game.wants_text() || self.keyboard > 0.0 {
                    #[cfg(target_os = "android")]
                    {
                        self.keyboard = android::keyboard_inset(&self.android);
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        self.keyboard = kulisa_platform::web_keyboard_inset();
                    }
                    #[cfg(target_os = "ios")]
                    {
                        self.keyboard = ios::keyboard_inset();
                    }
                }
                game.set_keyboard_inset(self.keyboard);
                #[cfg(target_arch = "wasm32")]
                for e in kulisa_platform::web_text_events() {
                    match e {
                        kulisa_platform::TextEvent::Text(t) => game.text_input(&t),
                        kulisa_platform::TextEvent::Key(k) => {
                            game.key(k, true);
                            game.key(k, false);
                        }
                    }
                }
                let now = Instant::now();
                game.frame((now - self.last_frame).as_secs_f32());
                if game.sensors_wanted() {
                    game.set_motion(self.sensors.poll());
                }
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

fn run_with(
    event_loop: EventLoop<UserEvent>,
    assets: Option<Result<Assets, String>>,
    dev: bool,
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
    let app = App::new(proxy, assets, dev, android);
    #[cfg(not(target_os = "android"))]
    let app = App::new(proxy, assets, dev);
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

/// The game folder or `.kpak`: the first argument that is not an option, `KULISA_GAME`, or
/// `game/` or `game.kpak` next to the executable (an iOS app bundle) or in the working
/// directory.
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
    let arg = std::env::args_os().skip(1).find(|a| !a.to_string_lossy().starts_with("--"));
    if let Some(p) = arg.or_else(|| std::env::var_os("KULISA_GAME")) {
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
    // `--dev` (what `kulisa run` passes) or KULISA_DEV=1; `?dev=1` on the web.
    let dev = kulisa_platform::dev_override() || std::env::args().skip(1).any(|a| a == "--dev");
    run_with(event_loop, assets, dev);
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
    run_with(event_loop, Some(assets), false, app);
}
