//! Frame loop. Owns the window, renderer and scripting, and drives them from winit events.

mod logo;
mod sfx;
mod smoke;

use std::sync::Arc;

use kulisa_audio::{Audio, PlayOptions};
use kulisa_core::{ScaleMode, Vec2, Viewport};
use kulisa_lua::Scripting;
use kulisa_render::Renderer;
use kulisa_text::TextSystem;
use sfx::Sfx;
use smoke::Smoke;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, TouchPhase, WindowEvent};
use winit::keyboard::{Key, NamedKey};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

#[cfg(target_os = "android")]
pub use winit::platform::android::activity::AndroidApp;

const HELLO: &str = include_str!("scripts/hello.luau");

pub enum UserEvent {
    RendererReady(Result<Renderer, String>),
}

struct Script {
    vm: Scripting,
    module: kulisa_lua::Table,
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    creating_renderer: bool,
    script: Option<Script>,
    smoke: Option<Smoke>,
    text: TextSystem,
    /// Created at startup on native; on the web only after the first user gesture, as
    /// browsers require.
    audio: Option<Audio>,
    sfx: Option<Sfx>,
    music_on: bool,
    viewport: Viewport,
    /// Last pointer position in window pixels.
    pointer: Option<Vec2>,
    start: Instant,
    last_frame: Instant,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let script = match load_script() {
            Ok(s) => Some(s),
            Err(e) => {
                log::error!("{e}");
                None
            }
        };
        Self {
            proxy,
            window: None,
            renderer: None,
            creating_renderer: false,
            script,
            smoke: None,
            text: load_fonts(),
            audio: None,
            sfx: None,
            music_on: true,
            viewport: Viewport { design: smoke::DESIGN, mode: ScaleMode::Expand },
            pointer: None,
            start: Instant::now(),
            last_frame: Instant::now(),
        }
    }

    fn start_audio(&mut self) {
        if self.audio.is_some() {
            return;
        }
        let started = Audio::new().and_then(|mut audio| {
            let sfx = Sfx::new()?;
            audio.set_volume("music", 0.35, 0.0)?;
            audio.play("music", &sfx.pad, PlayOptions { looping: true, fade_in: 2.0, ..Default::default() })?;
            Ok((audio, sfx))
        });
        match started {
            Ok((audio, sfx)) => (self.audio, self.sfx) = (Some(audio), Some(sfx)),
            // No output device (e.g. a headless machine): keep running silently.
            Err(e) => log::warn!("audio unavailable: {e}"),
        }
    }

    /// Click, tap, space or enter.
    fn interact(&mut self) {
        self.start_audio();
        if let (Some(audio), Some(sfx)) = (&mut self.audio, &self.sfx) {
            let _ = audio.play("sound", &sfx.blip, PlayOptions { volume: 0.6, ..Default::default() });
        }
        if let (Some(r), Some(smoke)) = (&mut self.renderer, &mut self.smoke) {
            smoke.advance(r, &mut self.text);
        }
    }

    fn toggle_music(&mut self) {
        self.start_audio();
        self.music_on = !self.music_on;
        if let Some(audio) = &mut self.audio {
            let _ = audio.set_volume("music", if self.music_on { 0.35 } else { 0.0 }, 0.6);
        }
    }

    fn create_renderer(&mut self, window: Arc<Window>) {
        self.creating_renderer = true;
        let size = window.inner_size();
        let proxy = self.proxy.clone();
        let task = async move {
            let result = Renderer::new(window, (size.width, size.height)).await;
            let _ = proxy.send_event(UserEvent::RendererReady(result));
        };
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);
        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(task);
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let t = (now - self.start).as_secs_f32();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let (mut tint, mut angle) = ([1.0; 4], 0.0);
        if let Some(s) = &self.script {
            match s.vm.call::<_, (f32, f32, f32, f32, f32)>(&s.module, "update", t) {
                Ok((r, g, b, a, rot)) => (tint, angle) = ([r, g, b, a], rot),
                Err(e) => {
                    log::error!("{e}");
                    self.script = None;
                }
            }
        }
        let (Some(renderer), Some(window), Some(smoke)) = (&mut self.renderer, &self.window, &mut self.smoke)
        else {
            return;
        };
        let layout = self.viewport.layout(renderer.size());
        let pointer = self.pointer.map(|p| layout.to_pixels.inverse().transform_point2(p));
        smoke.update(renderer, &mut self.text, layout.scale, t, dt, tint, angle, pointer);
        window.pre_present_notify();
        renderer.render(&smoke.scene, &self.viewport, t);
    }
}

fn load_fonts() -> TextSystem {
    let mut text = TextSystem::new();
    text.load_font(include_bytes!("../assets/fonts/Inter-Regular.ttf").to_vec());
    text.load_font(include_bytes!("../assets/fonts/Inter-Bold.ttf").to_vec());
    text
}

fn load_script() -> Result<Script, kulisa_lua::Error> {
    let vm = Scripting::new()?;
    log::info!("{} ready", vm.luau_version());
    let module = vm.run("hello.luau", HELLO)?;
    Ok(Script { vm, module })
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
        if self.renderer.is_none() && !self.creating_renderer {
            self.create_renderer(window);
        }
        if !cfg!(target_arch = "wasm32") {
            self.start_audio();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Android destroys the native window on suspend; the surface must go with it.
        // M0 rebuilds the whole renderer on resume.
        if cfg!(target_os = "android") {
            self.renderer = None;
            self.smoke = None;
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::RendererReady(result) => {
                self.creating_renderer = false;
                match result {
                    Ok(mut r) => {
                        // Glyph textures belonged to the previous renderer, if any.
                        self.text.clear_glyph_cache();
                        let scale = self.viewport.layout(r.size()).scale;
                        match Smoke::build(&mut r, &mut self.text, scale) {
                            Ok(s) => self.smoke = Some(s),
                            Err(e) => log::error!("{e}"),
                        }
                        if let Some(w) = &self.window {
                            let size = w.inner_size();
                            r.resize(size.width, size.height);
                            w.request_redraw();
                        }
                        self.renderer = Some(r);
                    }
                    Err(e) => log::error!("renderer: {e}"),
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Some(Vec2::new(position.x as f32, position.y as f32));
            }
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            WindowEvent::Touch(touch) => {
                self.pointer = Some(Vec2::new(touch.location.x as f32, touch.location.y as f32));
                if touch.phase == TouchPhase::Started {
                    self.interact();
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => self.interact(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat => {
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Space | NamedKey::Enter) => self.interact(),
                    Key::Character(c) if c.eq_ignore_ascii_case("m") => self.toggle_music(),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.frame();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn run_with(event_loop: EventLoop<UserEvent>) {
    let app = App::new(event_loop.create_proxy());
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

/// Runs the engine on desktop, iOS and web.
#[cfg(not(target_os = "android"))]
pub fn run() {
    kulisa_platform::init_logging();
    let event_loop = EventLoop::with_user_event().build().expect("failed to create event loop");
    run_with(event_loop);
}

/// Runs the engine inside an Android `NativeActivity`.
#[cfg(target_os = "android")]
pub fn run_android(app: AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    kulisa_platform::init_logging();
    let event_loop =
        EventLoop::with_user_event().with_android_app(app).build().expect("failed to create event loop");
    run_with(event_loop);
}
