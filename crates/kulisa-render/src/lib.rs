//! wgpu renderer: draws the passes that `kulisa_core::extract` produces from a scene.

mod atlas;
mod image;
mod shader;

use std::borrow::Cow;
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use kulisa_core::{
    BlendMode, DrawItem, Frame, Pass, PassTarget, Rect, Scene, ShaderId, TexRef, TextureId, UVec2, Vec2,
    Vertex, Viewport,
};

pub use image::Image;

use atlas::ShelfPacker;

/// A window wgpu can draw into. It also serves as the display handle (needed by GL and
/// Wayland), so it must be cheap to clone, e.g. `Arc<winit::window::Window>`.
pub trait SurfaceTarget:
    wgpu::DisplayAndWindowHandle + std::fmt::Debug + Clone + Send + Sync + 'static
{
}
impl<T: wgpu::DisplayAndWindowHandle + std::fmt::Debug + Clone + Send + Sync + 'static> SurfaceTarget for T {}

/// Counters for the last rendered frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub passes: u32,
    pub draw_calls: u32,
    pub vertices: u32,
}

/// Format of all render targets, transients and scratch textures.
const RT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Images up to this size (each side) are packed into atlas pages.
const ATLAS_MAX: u32 = 512;
const SPRITE_SHADER: u32 = 0;
const PRESENT_SHADER: u32 = 1;
const GLOBALS_SIZE: u64 = size_of::<Globals>() as u64;
const PARAMS_SIZE: u64 = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    proj: [[f32; 4]; 4],
    resolution: [f32; 2],
    time: f32,
    _pad: f32,
}

impl Globals {
    fn new(size: UVec2, time: f32) -> Self {
        let (w, h) = (size.x.max(1) as f32, size.y.max(1) as f32);
        Self {
            proj: [
                [2.0 / w, 0.0, 0.0, 0.0],
                [0.0, -2.0 / h, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0, 1.0],
            ],
            resolution: [w, h],
            time,
            _pad: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Blend {
    Mode(BlendMode),
    /// No blending: effects and present overwrite their output.
    Replace,
}

struct GpuTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    size: UVec2,
}

enum Tex {
    Atlas { page: usize, uv: Rect, size: UVec2 },
    Own(GpuTexture),
}

struct Page {
    tex: GpuTexture,
    packer: ShelfPacker,
}

enum Output {
    Surface { surface: wgpu::Surface<'static>, config: wgpu::SurfaceConfiguration },
    Headless(GpuTexture),
}

/// Per-frame uniform slots: one globals block per pass (plus present), one params block
/// per material or effect use. Slot 0 of params is all zeros.
#[derive(Default)]
struct Uniforms {
    globals: Vec<u8>,
    params: Vec<u8>,
    item_params: Vec<Vec<u32>>,
    effect_params: Vec<Vec<u32>>,
}

/// Draws scenes with wgpu. One per window (or one headless instance for tests/thumbnails).
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    output: Output,
    /// Format of the view we draw into on the output (sRGB when the platform allows).
    out_format: wgpu::TextureFormat,
    /// The output cannot be viewed as sRGB: the screen is drawn into `screen` and encoded
    /// by a final present pass.
    encode_on_present: bool,
    screen: Option<GpuTexture>,
    size: UVec2,
    sampler: wgpu::Sampler,
    globals_layout: wgpu::BindGroupLayout,
    params_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    shaders: Vec<wgpu::ShaderModule>,
    pipelines: HashMap<(u32, Blend, wgpu::TextureFormat), wgpu::RenderPipeline>,
    textures: Vec<Tex>,
    pages: Vec<Page>,
    page_size: u32,
    white_uv: Rect,
    transients: Vec<GpuTexture>,
    scratch: Vec<GpuTexture>,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    ubuf: wgpu::Buffer,
    pbuf: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    params_bg: wgpu::BindGroup,
    align: u64,
    frame: Frame,
    stats: FrameStats,
    warned_feedback: bool,
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

fn instance_descriptor(
    backends: wgpu::Backends,
    mut desc: wgpu::InstanceDescriptor,
) -> wgpu::InstanceDescriptor {
    // WGPU_BACKEND in the environment overrides our preference.
    if std::env::var_os("WGPU_BACKEND").is_none() {
        desc.backends = backends;
    }
    // Some Android Vulkan drivers (including the emulator's) crash on debug object names.
    if cfg!(target_os = "android") {
        desc.flags.remove(wgpu::InstanceFlags::DEBUG | wgpu::InstanceFlags::VALIDATION);
    }
    desc
}

async fn create_instance(desc: wgpu::InstanceDescriptor) -> wgpu::Instance {
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

/// Drivers known to crash on valid usage; the next backend in the preference list is used.
fn is_denylisted(info: &wgpu::AdapterInfo) -> bool {
    // The Android emulator's gfxstream Vulkan driver crashes in vkQueueSubmit.
    info.backend == wgpu::Backend::Vulkan && info.name.contains("GFXStream")
}

fn blend_state(blend: Blend) -> Option<wgpu::BlendState> {
    use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation::Add as Op};
    let c = |src, dst| C { src_factor: src, dst_factor: dst, operation: Op };
    let over = c(F::One, F::OneMinusSrcAlpha);
    match blend {
        Blend::Replace => None,
        Blend::Mode(BlendMode::Alpha) => Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        Blend::Mode(BlendMode::Add) => Some(wgpu::BlendState { color: c(F::One, F::One), alpha: over }),
        Blend::Mode(BlendMode::Multiply) => {
            Some(wgpu::BlendState { color: c(F::Dst, F::OneMinusSrcAlpha), alpha: over })
        }
        Blend::Mode(BlendMode::Screen) => {
            Some(wgpu::BlendState { color: c(F::One, F::OneMinusSrc), alpha: over })
        }
    }
}

fn create_gpu_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    size: UVec2,
    render_target: bool,
) -> GpuTexture {
    let mut usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
    if render_target {
        usage |= wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: RT_FORMAT,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    });
    GpuTexture { texture, view, bind_group, size }
}

fn uniform_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buf: &wgpu::Buffer,
    size: u64,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: buf,
                offset: 0,
                size: wgpu::BufferSize::new(size),
            }),
        }],
    })
}

/// Grows `buf` to hold `needed` bytes; returns true if it was recreated.
fn ensure_capacity(device: &wgpu::Device, buf: &mut wgpu::Buffer, needed: u64) -> bool {
    if needed <= buf.size() {
        return false;
    }
    *buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: needed.next_power_of_two().max(256),
        usage: buf.usage(),
        mapped_at_creation: false,
    });
    true
}

fn ortho_quad(size: UVec2, frame: &mut Frame) -> std::ops::Range<u32> {
    let (w, h) = (size.x as f32, size.y as f32);
    let v0 = frame.vertices.len() as u32;
    let i0 = frame.indices.len() as u32;
    for (pos, uv) in
        [([0.0, 0.0], [0.0, 0.0]), ([w, 0.0], [1.0, 0.0]), ([w, h], [1.0, 1.0]), ([0.0, h], [0.0, 1.0])]
    {
        frame.vertices.push(Vertex { pos, uv, color: [255; 4] });
    }
    frame.indices.extend([0, 1, 2, 0, 2, 3].map(|i| v0 + i));
    i0..i0 + 6
}

impl Renderer {
    /// Creates a renderer drawing into `target`. `size` is the drawable size in pixels.
    pub async fn new(target: impl SurfaceTarget, size: (u32, u32)) -> Result<Self, String> {
        for backends in backend_preference() {
            let desc = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(target.clone()));
            let instance = create_instance(instance_descriptor(backends, desc)).await;
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
                Ok(a) if is_denylisted(&a.get_info()) => {
                    log::warn!("skipping unreliable driver: {}", a.get_info().name)
                }
                Ok(adapter) => return Self::init(adapter, Some(surface), UVec2::new(size.0, size.1)).await,
                Err(e) => log::warn!("no adapter for {backends:?}: {e}"),
            }
        }
        Err("no usable GPU adapter".into())
    }

    /// Creates a renderer that draws into an offscreen texture (tests, thumbnails). Falls back
    /// to a software adapter when no GPU is available.
    pub async fn headless(width: u32, height: u32) -> Result<Self, String> {
        for force_fallback_adapter in [false, true] {
            for backends in backend_preference() {
                let desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
                let instance = create_instance(instance_descriptor(backends, desc)).await;
                let options = wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter,
                    apply_limit_buckets: false,
                };
                if let Ok(adapter) = instance.request_adapter(&options).await
                    && !is_denylisted(&adapter.get_info())
                {
                    return Self::init(adapter, None, UVec2::new(width, height)).await;
                }
            }
        }
        Err("no usable GPU adapter".into())
    }

    async fn init(
        adapter: wgpu::Adapter,
        surface: Option<wgpu::Surface<'static>>,
        size: UVec2,
    ) -> Result<Self, String> {
        let info = adapter.get_info();
        log::info!("GPU: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
        let limits = adapter.limits();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("kulisa"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(limits.clone()),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let size = size.max(UVec2::ONE);
        // Uniform slots must be a multiple of the *device's* (requested) alignment.
        let device_align = device.limits().min_uniform_buffer_offset_alignment as u64;
        let align = GLOBALS_SIZE.max(PARAMS_SIZE).div_ceil(device_align) * device_align;

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform_layout = |label, size| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(size),
                    },
                    count: None,
                }],
            })
        };
        let globals_layout = uniform_layout("globals", GLOBALS_SIZE);
        let params_layout = uniform_layout("params", PARAMS_SIZE);
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kulisa"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout), Some(&params_layout)],
            ..Default::default()
        });

        let (output, out_format, encode_on_present) = match surface {
            Some(surface) => {
                let caps = surface.get_capabilities(&adapter);
                let surface_format =
                    caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
                let view_formats_ok = adapter
                    .get_downlevel_capabilities()
                    .flags
                    .contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS);
                let (out_format, encode) = if surface_format.is_srgb() {
                    (surface_format, false)
                } else if view_formats_ok {
                    (surface_format.add_srgb_suffix(), false)
                } else {
                    (surface_format, true)
                };
                let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
                    wgpu::CompositeAlphaMode::Opaque
                } else {
                    caps.alpha_modes[0]
                };
                let config = wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: surface_format,
                    width: size.x,
                    height: size.y,
                    present_mode: wgpu::PresentMode::AutoVsync,
                    desired_maximum_frame_latency: 2,
                    alpha_mode,
                    view_formats: if out_format != surface_format { vec![out_format] } else { vec![] },
                    color_space: wgpu::SurfaceColorSpace::Auto,
                };
                surface.configure(&device, &config);
                (Output::Surface { surface, config }, out_format, encode)
            }
            None => (
                Output::Headless(create_gpu_texture(&device, &texture_layout, &sampler, size, true)),
                RT_FORMAT,
                false,
            ),
        };

        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let ubuf = buffer("globals", 16 * align, wgpu::BufferUsages::UNIFORM);
        let pbuf = buffer("params", 64 * align, wgpu::BufferUsages::UNIFORM);
        let globals_bg = uniform_bind_group(&device, &globals_layout, &ubuf, GLOBALS_SIZE);
        let params_bg = uniform_bind_group(&device, &params_layout, &pbuf, PARAMS_SIZE);
        let vbuf = buffer("vertices", 1 << 16, wgpu::BufferUsages::VERTEX);
        let ibuf = buffer("indices", 1 << 16, wgpu::BufferUsages::INDEX);

        let mut r = Self {
            device,
            queue,
            output,
            out_format,
            encode_on_present,
            screen: None,
            size,
            sampler,
            globals_layout,
            params_layout,
            texture_layout,
            pipeline_layout,
            shaders: Vec::new(),
            pipelines: HashMap::new(),
            textures: Vec::new(),
            pages: Vec::new(),
            page_size: limits.max_texture_dimension_2d.min(2048),
            white_uv: Rect::UNIT,
            transients: Vec::new(),
            scratch: Vec::new(),
            vbuf,
            ibuf,
            ubuf,
            pbuf,
            globals_bg,
            params_bg,
            align,
            frame: Frame::default(),
            stats: FrameStats::default(),
            warned_feedback: false,
        };
        let sprite = r.module("sprite", shader::build("sprite", shader::SPRITE)?);
        let present = r.module("present", shader::build("present", shader::PRESENT_ENCODE)?);
        r.shaders.extend([sprite, present]);
        // Page 0 holds a white texel for solid-color draws.
        r.add_page();
        let (_, uv) = r.pack(&Image::solid(1, 1, [255; 4])).expect("an empty page fits a texel");
        let center = (uv.min + uv.max) * 0.5;
        r.white_uv = Rect { min: center, max: center };
        Ok(r)
    }

    fn module(&self, label: &str, source: String) -> wgpu::ShaderModule {
        self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
        })
    }

    fn new_texture(&self, size: UVec2, render_target: bool) -> GpuTexture {
        create_gpu_texture(
            &self.device,
            &self.texture_layout,
            &self.sampler,
            size.max(UVec2::ONE),
            render_target,
        )
    }

    /// Drawable size in pixels.
    pub fn size(&self) -> UVec2 {
        self.size
    }

    /// Counters for the last rendered frame.
    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || UVec2::new(width, height) == self.size {
            return;
        }
        let old = self.size;
        self.size = UVec2::new(width, height);
        let new_headless =
            matches!(self.output, Output::Headless(_)).then(|| self.new_texture(self.size, true));
        match &mut self.output {
            Output::Surface { surface, config } => {
                config.width = width;
                config.height = height;
                surface.configure(&self.device, config);
            }
            Output::Headless(tex) => *tex = new_headless.unwrap(),
        }
        // Screen-sized textures are recreated on demand.
        self.transients.clear();
        self.screen = None;
        self.scratch.retain(|t| t.size != old);
    }

    /// Uploads an image. Small images share atlas pages, which keeps them batchable.
    pub fn create_texture(&mut self, image: &Image) -> TextureId {
        let size = UVec2::new(image.width, image.height);
        let id = TextureId(self.textures.len() as u32);
        if (1..=ATLAS_MAX).contains(&image.width) && (1..=ATLAS_MAX).contains(&image.height) {
            let packed = self.pack(image).or_else(|| {
                self.add_page();
                self.pack(image)
            });
            if let Some((page, uv)) = packed {
                self.textures.push(Tex::Atlas { page, uv, size });
                return id;
            }
        }
        let tex = self.new_texture(size, false);
        self.upload(&tex.texture, 0, 0, image);
        self.textures.push(Tex::Own(tex));
        id
    }

    /// Creates a render target for a [`kulisa_core::Target`] node; sprites can show it.
    pub fn create_target(&mut self, width: u32, height: u32) -> TextureId {
        let tex = self.new_texture(UVec2::new(width, height), true);
        self.textures.push(Tex::Own(tex));
        TextureId(self.textures.len() as u32 - 1)
    }

    /// Size of a texture in pixels.
    pub fn texture_size(&self, id: TextureId) -> Option<UVec2> {
        match self.textures.get(id.0 as usize)? {
            Tex::Atlas { size, .. } => Some(*size),
            Tex::Own(t) => Some(t.size),
        }
    }

    /// Compiles a user shader. `name` (e.g. its file path) prefixes error messages.
    pub fn create_shader(&mut self, name: &str, source: &str) -> Result<ShaderId, String> {
        let full = shader::build(name, source)?;
        self.shaders.push(self.module(name, full));
        Ok(ShaderId(self.shaders.len() as u32 - 1))
    }

    /// Replaces a user shader's source (live reload). On error the old shader stays.
    pub fn reload_shader(&mut self, id: ShaderId, name: &str, source: &str) -> Result<(), String> {
        if id.0 <= PRESENT_SHADER || id.0 as usize >= self.shaders.len() {
            return Err(format!("{name}: not a user shader"));
        }
        let full = shader::build(name, source)?;
        self.shaders[id.0 as usize] = self.module(name, full);
        self.pipelines.retain(|(s, _, _), _| *s != id.0);
        Ok(())
    }

    fn add_page(&mut self) {
        let tex = self.new_texture(UVec2::splat(self.page_size), false);
        self.pages.push(Page { tex, packer: ShelfPacker::new(self.page_size) });
    }

    /// Packs `image` (with a 1px extruded border) into the newest page that fits.
    fn pack(&mut self, image: &Image) -> Option<(usize, Rect)> {
        let padded = image.extruded(1);
        let index = self.pages.len().checked_sub(1)?;
        let (x, y) = self.pages[index].packer.alloc(padded.width, padded.height)?;
        self.upload(&self.pages[index].tex.texture, x, y, &padded);
        let s = self.page_size as f32;
        let min = Vec2::new((x + 1) as f32, (y + 1) as f32) / s;
        let max = min + Vec2::new(image.width as f32, image.height as f32) / s;
        Some((index, Rect { min, max }))
    }

    fn upload(&self, texture: &wgpu::Texture, x: u32, y: u32, image: &Image) {
        if image.width == 0 || image.height == 0 {
            return;
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * image.width),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d { width: image.width, height: image.height, depth_or_array_layers: 1 },
        );
    }

    fn pipeline(&mut self, shader: u32, blend: Blend, format: wgpu::TextureFormat) {
        let key = (shader, blend, format);
        if self.pipelines.contains_key(&key) {
            return;
        }
        let module = &self.shaders[shader as usize];
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: blend_state(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key, pipeline);
    }

    fn shader_index(&self, id: ShaderId) -> u32 {
        if (id.0 as usize) < self.shaders.len() { id.0 } else { SPRITE_SHADER }
    }

    /// Format of the texture a pass writes into.
    fn target_format(&self, target: PassTarget) -> wgpu::TextureFormat {
        match target {
            PassTarget::Surface if !self.encode_on_present => self.out_format,
            _ => RT_FORMAT,
        }
    }

    /// Remaps sprite UVs into atlas pages and the white texel.
    fn remap_uvs(&self, frame: &mut Frame) {
        for pass in &frame.passes {
            for item in &pass.items {
                let uv = match item.texture {
                    TexRef::Texture(id) => match self.textures.get(id.0 as usize) {
                        Some(Tex::Atlas { uv, .. }) => *uv,
                        Some(Tex::Own(_)) => continue,
                        None => self.white_uv,
                    },
                    TexRef::White => self.white_uv,
                    TexRef::Transient(_) => continue,
                };
                for v in &mut frame.vertices[item.vertices.start as usize..item.vertices.end as usize] {
                    v.uv = (uv.min + uv.size() * Vec2::from(v.uv)).to_array();
                }
            }
        }
    }

    fn uniforms(&self, frame: &Frame, time: f32) -> Uniforms {
        let a = self.align as usize;
        let mut u = Uniforms::default();
        for pass in &frame.passes {
            let at = u.globals.len();
            u.globals.resize(at + a, 0);
            u.globals[at..at + GLOBALS_SIZE as usize]
                .copy_from_slice(bytemuck::bytes_of(&Globals::new(pass.size, time)));
        }
        // Present pass globals.
        let at = u.globals.len();
        u.globals.resize(at + a, 0);
        u.globals[at..at + GLOBALS_SIZE as usize]
            .copy_from_slice(bytemuck::bytes_of(&Globals::new(self.size, time)));

        u.params.resize(a, 0); // slot 0: zeros
        let mut push = |params: &[f32; 16]| {
            let at = u.params.len();
            u.params.resize(at + a, 0);
            u.params[at..at + PARAMS_SIZE as usize].copy_from_slice(bytemuck::cast_slice(params));
            (at / a) as u32
        };
        for pass in &frame.passes {
            u.item_params
                .push(pass.items.iter().map(|i| i.material.map_or(0, |m| push(&m.params))).collect());
            u.effect_params.push(pass.effects.iter().map(|e| push(&e.params)).collect());
        }
        u
    }

    /// Returns scratch texture indices `[a, b]` of the given size, creating them if needed.
    fn scratch_pair(&mut self, size: UVec2) -> [usize; 2] {
        let mut found: Vec<usize> =
            self.scratch.iter().enumerate().filter(|(_, t)| t.size == size).map(|(i, _)| i).take(2).collect();
        while found.len() < 2 {
            self.scratch.push(self.new_texture(size, true));
            found.push(self.scratch.len() - 1);
        }
        [found[0], found[1]]
    }

    fn texture_bind_group(&self, t: TexRef) -> &wgpu::BindGroup {
        match t {
            TexRef::Texture(id) => match self.textures.get(id.0 as usize) {
                Some(Tex::Atlas { page, .. }) => &self.pages[*page].tex.bind_group,
                Some(Tex::Own(t)) => &t.bind_group,
                None => &self.pages[0].tex.bind_group,
            },
            TexRef::White => &self.pages[0].tex.bind_group,
            TexRef::Transient(k) => &self.transients[k as usize].bind_group,
        }
    }

    /// Renders `scene` laid out by `viewport` and presents it. `time` is in seconds.
    pub fn render(&mut self, scene: &Scene, viewport: &Viewport, time: f32) {
        let mut frame = std::mem::take(&mut self.frame);
        kulisa_core::extract(scene, &viewport.layout(self.size), self.size, &mut frame);
        self.remap_uvs(&mut frame);

        // A full-target quad for every pass with effects, plus one for present.
        let effect_quads: Vec<_> = frame
            .passes
            .clone()
            .iter()
            .map(|p| (!p.effects.is_empty()).then(|| ortho_quad(p.size, &mut frame)))
            .collect();
        let present_quad = ortho_quad(self.size, &mut frame);
        let uniforms = self.uniforms(&frame, time);

        // Resources: transients, scratch, buffers, pipelines.
        while self.transients.len() < frame.transients as usize {
            self.transients.push(self.new_texture(self.size, true));
        }
        if self.encode_on_present && self.screen.is_none() {
            self.screen = Some(self.new_texture(self.size, true));
        }
        let mut pairs = Vec::with_capacity(frame.passes.len());
        for pass in &frame.passes {
            pairs.push((!pass.effects.is_empty()).then(|| self.scratch_pair(pass.size)));
        }
        let vbytes: &[u8] = bytemuck::cast_slice(&frame.vertices);
        let ibytes: &[u8] = bytemuck::cast_slice(&frame.indices);
        ensure_capacity(&self.device, &mut self.vbuf, vbytes.len() as u64);
        ensure_capacity(&self.device, &mut self.ibuf, ibytes.len() as u64);
        if ensure_capacity(&self.device, &mut self.ubuf, uniforms.globals.len() as u64) {
            self.globals_bg =
                uniform_bind_group(&self.device, &self.globals_layout, &self.ubuf, GLOBALS_SIZE);
        }
        if ensure_capacity(&self.device, &mut self.pbuf, uniforms.params.len() as u64) {
            self.params_bg = uniform_bind_group(&self.device, &self.params_layout, &self.pbuf, PARAMS_SIZE);
        }
        self.queue.write_buffer(&self.vbuf, 0, vbytes);
        self.queue.write_buffer(&self.ibuf, 0, ibytes);
        self.queue.write_buffer(&self.ubuf, 0, &uniforms.globals);
        self.queue.write_buffer(&self.pbuf, 0, &uniforms.params);
        for pass in &frame.passes {
            let fmt = self.target_format(pass.target);
            let item_fmt = if pass.effects.is_empty() { fmt } else { RT_FORMAT };
            for item in &pass.items {
                let shader = item.material.map_or(SPRITE_SHADER, |m| self.shader_index(m.shader));
                self.pipeline(shader, Blend::Mode(item.blend), item_fmt);
            }
            for (k, e) in pass.effects.iter().enumerate() {
                let f = if k + 1 == pass.effects.len() { fmt } else { RT_FORMAT };
                self.pipeline(self.shader_index(e.shader), Blend::Replace, f);
            }
        }
        if self.encode_on_present {
            self.pipeline(PRESENT_SHADER, Blend::Replace, self.out_format);
        }

        // Acquire the output.
        let surface_tex = match &self.output {
            Output::Surface { surface, config } => match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t) => Some(t),
                wgpu::CurrentSurfaceTexture::Suboptimal(_)
                | wgpu::CurrentSurfaceTexture::Outdated
                | wgpu::CurrentSurfaceTexture::Lost => {
                    surface.configure(&self.device, config);
                    self.frame = frame;
                    return;
                }
                other => {
                    log::debug!("skipping frame: {other:?}");
                    self.frame = frame;
                    return;
                }
            },
            Output::Headless(_) => None,
        };
        let surface_view = surface_tex.as_ref().map(|t| {
            t.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(self.out_format),
                ..Default::default()
            })
        });
        let screen_view = match (&self.screen, &surface_view, &self.output) {
            (Some(s), _, _) => &s.view,
            (None, Some(v), _) => v,
            (None, None, Output::Headless(t)) => &t.view,
            _ => unreachable!("a surface output always has a view"),
        };

        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut stats = FrameStats { vertices: frame.vertices.len() as u32, ..Default::default() };
        for (i, pass) in frame.passes.iter().enumerate() {
            let target_view = match pass.target {
                PassTarget::Surface => screen_view,
                PassTarget::Transient(k) => &self.transients[k as usize].view,
                PassTarget::Texture(id) => match self.textures.get(id.0 as usize) {
                    Some(Tex::Own(t)) => &t.view,
                    _ => {
                        log::warn!("pass target {id:?} is not a render target");
                        continue;
                    }
                },
            };
            let globals = (i as u64 * self.align) as u32;
            let fmt = self.target_format(pass.target);
            match pairs[i] {
                None => self.draw_items(
                    &mut encoder,
                    target_view,
                    fmt,
                    pass,
                    globals,
                    &uniforms.item_params[i],
                    &mut stats,
                ),
                Some([a, b]) => {
                    let (sa, sb) = (&self.scratch[a], &self.scratch[b]);
                    self.draw_items(
                        &mut encoder,
                        &sa.view,
                        RT_FORMAT,
                        pass,
                        globals,
                        &uniforms.item_params[i],
                        &mut stats,
                    );
                    let quad = effect_quads[i].clone().unwrap();
                    for (k, effect) in pass.effects.iter().enumerate() {
                        let last = k + 1 == pass.effects.len();
                        let (src, dst) = if k % 2 == 0 { (sa, &sb.view) } else { (sb, &sa.view) };
                        let (dst, f) = if last { (target_view, fmt) } else { (dst, RT_FORMAT) };
                        let shader = self.shader_index(effect.shader);
                        let params = uniforms.effect_params[i][k];
                        self.draw_fullscreen(
                            &mut encoder,
                            dst,
                            f,
                            shader,
                            &src.bind_group,
                            globals,
                            params,
                            quad.clone(),
                        );
                        stats.draw_calls += 1;
                        stats.passes += 1;
                    }
                }
            }
        }
        if let (Some(screen), Some(view)) = (&self.screen, &surface_view) {
            let globals = (frame.passes.len() as u64 * self.align) as u32;
            self.draw_fullscreen(
                &mut encoder,
                view,
                self.out_format,
                PRESENT_SHADER,
                &screen.bind_group,
                globals,
                0,
                present_quad,
            );
            stats.draw_calls += 1;
            stats.passes += 1;
        }
        self.queue.submit([encoder.finish()]);
        if let Some(t) = surface_tex {
            self.queue.present(t);
        }
        self.stats = stats;
        self.frame = frame;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_items(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        pass: &Pass,
        globals: u32,
        params: &[u32],
        stats: &mut FrameStats,
    ) {
        let load = match pass.clear {
            Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                r: c.r as f64,
                g: c.g as f64,
                b: c.b as f64,
                a: c.a as f64,
            }),
            None => wgpu::LoadOp::Load,
        };
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        stats.passes += 1;
        if pass.items.is_empty() {
            return;
        }
        rp.set_vertex_buffer(0, self.vbuf.slice(..));
        rp.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
        rp.set_bind_group(0, &self.globals_bg, &[globals]);

        // Second-level batching: consecutive items whose GPU state matches (e.g. different
        // textures on the same atlas page) and whose indices are contiguous become one call.
        let mut pending: Option<(&DrawItem, std::ops::Range<u32>, u32)> = None;
        let mut last_state = None;
        let own_target = match pass.target {
            PassTarget::Texture(id) => Some(TexRef::Texture(id)),
            PassTarget::Transient(k) => Some(TexRef::Transient(k)),
            PassTarget::Surface => None,
        };
        let mut flush = |rp: &mut wgpu::RenderPass,
                         p: &mut Option<(&DrawItem, std::ops::Range<u32>, u32)>| {
            if let Some((_, range, _)) = p.take() {
                rp.draw_indexed(range, 0, 0..1);
                stats.draw_calls += 1;
            }
        };
        for (item, &param) in pass.items.iter().zip(params) {
            if own_target == Some(item.texture) {
                if !self.warned_feedback {
                    log::warn!("a render target cannot draw itself; skipping");
                }
                continue;
            }
            if let Some([_, _, w, h]) = item.clip
                && (w == 0 || h == 0)
            {
                continue;
            }
            let shader = item.material.map_or(SPRITE_SHADER, |m| self.shader_index(m.shader));
            let bg = self.texture_bind_group(item.texture);
            let state = (shader, item.blend, bg as *const _, param, item.clip);
            if let Some((_, range, _)) = &mut pending
                && last_state == Some(state)
                && range.end == item.indices.start
            {
                range.end = item.indices.end;
                continue;
            }
            flush(&mut rp, &mut pending);
            if last_state != Some(state) {
                rp.set_pipeline(&self.pipelines[&(shader, Blend::Mode(item.blend), format)]);
                rp.set_bind_group(1, bg, &[]);
                rp.set_bind_group(2, &self.params_bg, &[param * self.align as u32]);
                let [x, y, w, h] = item.clip.unwrap_or([0, 0, pass.size.x, pass.size.y]);
                rp.set_scissor_rect(x, y, w, h);
                last_state = Some(state);
            }
            pending = Some((item, item.indices.clone(), param));
        }
        flush(&mut rp, &mut pending);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_fullscreen(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        shader: u32,
        input: &wgpu::BindGroup,
        globals: u32,
        param: u32,
        quad: std::ops::Range<u32>,
    ) {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rp.set_pipeline(&self.pipelines[&(shader, Blend::Replace, format)]);
        rp.set_vertex_buffer(0, self.vbuf.slice(..));
        rp.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
        rp.set_bind_group(0, &self.globals_bg, &[globals]);
        rp.set_bind_group(1, input, &[]);
        rp.set_bind_group(2, &self.params_bg, &[param * self.align as u32]);
        rp.draw_indexed(quad, 0, 0..1);
    }

    /// Reads back the headless output (premultiplied sRGB RGBA8). Native only.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_pixels(&self) -> Option<Image> {
        let Output::Headless(tex) = &self.output else { return None };
        let (w, h) = (tex.size.x, tex.size.y);
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &tex.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);
        buf.map_async(wgpu::MapMode::Read, .., |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = buf.get_mapped_range(..).ok()?;
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            pixels.extend_from_slice(&data[y * row as usize..][..(w * 4) as usize]);
        }
        Some(Image { width: w, height: h, pixels })
    }
}
