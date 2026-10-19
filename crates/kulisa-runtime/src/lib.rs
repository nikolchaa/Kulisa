//! Frame loop. Owns the window, renderer and scripting, and drives them from winit events.

mod logo;

use std::sync::Arc;

use kulisa_lua::Scripting;
use kulisa_render::{QuadParams, Renderer};
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
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
    start: Instant,
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
        Self { proxy, window: None, renderer: None, creating_renderer: false, script, start: Instant::now() }
    }

    fn create_renderer(&mut self, window: Arc<Window>) {
        self.creating_renderer = true;
        let size = window.inner_size();
        let image = logo::image(256);
        let proxy = self.proxy.clone();
        let task = async move {
            let result = Renderer::new(window, (size.width, size.height), &image).await;
            let _ = proxy.send_event(UserEvent::RendererReady(result));
        };
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);
        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(task);
    }

    fn frame(&mut self) {
        let t = self.start.elapsed().as_secs_f64();
        let mut quad = QuadParams { tint: [1.0; 4], angle: 0.0, half_size: 160.0 };
        if let Some(s) = &self.script {
            match s.vm.call::<_, (f32, f32, f32, f32, f32)>(&s.module, "update", t) {
                Ok((r, g, b, a, angle)) => {
                    quad.tint = [r, g, b, a];
                    quad.angle = angle;
                }
                Err(e) => {
                    log::error!("{e}");
                    self.script = None;
                }
            }
        }
        if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
            quad.half_size *= window.scale_factor() as f32;
            window.pre_present_notify();
            renderer.render(quad);
        }
    }
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
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Android destroys the native window on suspend; the surface must go with it.
        // M0 rebuilds the whole renderer on resume.
        if cfg!(target_os = "android") {
            self.renderer = None;
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::RendererReady(result) => {
                self.creating_renderer = false;
                match result {
                    Ok(mut r) => {
                        r.set_clear_srgb(0.094, 0.094, 0.122);
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
    let event_loop = EventLoop::with_user_event()
        .with_android_app(app)
        .build()
        .expect("failed to create event loop");
    run_with(event_loop);
}
