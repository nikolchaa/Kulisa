//! wgpu renderer. M0 scope: device setup with per-platform backend preference and one
//! textured quad. Batching, scene extraction and shaders arrive in M1.

use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};

/// A window wgpu can draw into. It also serves as the display handle (needed by GL and
/// Wayland), so it must be cheap to clone, e.g. `Arc<winit::window::Window>`.
pub trait SurfaceTarget:
    wgpu::DisplayAndWindowHandle + std::fmt::Debug + Clone + Send + Sync + 'static
{
}
impl<T: wgpu::DisplayAndWindowHandle + std::fmt::Debug + Clone + Send + Sync + 'static> SurfaceTarget for T {}

/// An RGBA8 image with premultiplied alpha.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Per-frame parameters for the M0 quad.
#[derive(Clone, Copy, Debug)]
pub struct QuadParams {
    /// Premultiplied RGBA tint.
    pub tint: [f32; 4],
    /// Rotation in radians.
    pub angle: f32,
    /// Half-size of the quad in pixels.
    pub half_size: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    tint: [f32; 4],
    rot: [f32; 2],
    scale: [f32; 2],
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// Format of the views we render into (always sRGB when the platform allows it).
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    clear: wgpu::Color,
}

/// Backends to try, in order. Vulkan is preferred wherever it exists.
fn backend_preference() -> Vec<wgpu::Backends> {
    use wgpu::Backends as B;
    if cfg!(target_arch = "wasm32") {
        vec![B::BROWSER_WEBGPU | B::GL]
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        vec![B::METAL]
    } else if cfg!(windows) {
        vec![B::VULKAN, B::DX12, B::GL]
    } else {
        vec![B::VULKAN, B::GL]
    }
}

async fn create_instance(backends: wgpu::Backends, display: impl SurfaceTarget) -> wgpu::Instance {
    let mut desc = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display));
    // WGPU_BACKEND in the environment overrides our preference.
    if std::env::var_os("WGPU_BACKEND").is_none() {
        desc.backends = backends;
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Falls back to WebGL2 when `navigator.gpu` is missing or unusable.
        wgpu::util::new_instance_with_webgpu_detection(desc).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        wgpu::Instance::new(desc)
    }
}

impl Renderer {
    /// Creates a renderer for `target`. `size` is the drawable size in physical pixels.
    pub async fn new(target: impl SurfaceTarget, size: (u32, u32), image: &Image) -> Result<Self, String> {
        let mut found = None;
        for backends in backend_preference() {
            let instance = create_instance(backends, target.clone()).await;
            let surface = match instance.create_surface(target.clone()) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("no surface for {backends:?}: {e}");
                    continue;
                }
            };
            let options = wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            };
            match instance.request_adapter(&options).await {
                Ok(adapter) => {
                    found = Some((instance, surface, adapter));
                    break;
                }
                Err(e) => log::warn!("no adapter for {backends:?}: {e}"),
            }
        }
        let (_instance, surface, adapter) = found.ok_or("no usable GPU adapter")?;
        let info = adapter.get_info();
        log::info!("GPU: {} ({:?}, {:?})", info.name, info.backend, info.device_type);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("kulisa"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;

        let caps = surface.get_capabilities(&adapter);
        // Render through an sRGB view so blending happens in linear space. Canvases (WebGPU)
        // only offer non-sRGB formats but allow an sRGB view of them.
        let surface_format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let format = surface_format.add_srgb_suffix();
        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            caps.alpha_modes[0]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: if format == surface_format { vec![] } else { vec![format] },
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("quad.wgsl"))),
        });

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let texture = upload_image(&device, &queue, image);
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            surface,
            config,
            format,
            pipeline,
            globals,
            bind_group,
            clear: wgpu::Color::BLACK,
        })
    }

    /// Sets the clear color from sRGB components.
    pub fn set_clear_srgb(&mut self, r: f64, g: f64, b: f64) {
        let lin = |c: f64| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        let srgb_target = self.format.is_srgb();
        let f = |c: f64| if srgb_target { lin(c) } else { c };
        self.clear = wgpu::Color { r: f(r), g: f(g), b: f(b), a: 1.0 };
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, quad: QuadParams) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                drop(t);
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            other => {
                log::debug!("skipping frame: {other:?}");
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.format),
            ..Default::default()
        });

        let (w, h) = (self.config.width as f32, self.config.height as f32);
        let globals = Globals {
            tint: quad.tint,
            rot: [quad.angle.cos(), quad.angle.sin()],
            scale: [quad.half_size * 2.0 / w, quad.half_size * 2.0 / h],
        };
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }
}

fn upload_image(device: &wgpu::Device, queue: &wgpu::Queue, image: &Image) -> wgpu::Texture {
    let size = wgpu::Extent3d { width: image.width, height: image.height, depth_or_array_layers: 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("image"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &image.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * image.width),
            rows_per_image: Some(image.height),
        },
        size,
    );
    texture
}
