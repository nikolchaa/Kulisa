// ---- Kulisa shader prelude (appended after user code) ----
//
// User shaders define:
//   fn fragment(in: VertexOut) -> vec4<f32>          required, returns premultiplied linear color
//   fn vertex(in: VertexIn) -> VertexOut             optional, defaults to kulisa_vertex
// and may read: in.data (a mesh's per-vertex data), globals (time, resolution), params.p[0..4], tex/samp (the bound texture) and
// mask_tex/mask_samp (the material's mask; white when it has none).

struct Globals {
    proj: mat4x4<f32>,
    // Target size in pixels.
    resolution: vec2<f32>,
    // Seconds since start.
    time: f32,
    _pad: f32,
}

struct Params {
    p: array<vec4<f32>, 4>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var<uniform> params: Params;
@group(3) @binding(0) var mask_tex: texture_2d<f32>;
@group(3) @binding(1) var mask_samp: sampler;

struct VertexIn {
    // Target pixels.
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    // Premultiplied linear tint.
    @location(2) color: vec4<f32>,
    // A mesh's per-vertex data (zero for everything else).
    @location(3) data: vec4<f32>,
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // Target pixels, for screen-space effects.
    @location(2) pixel: vec2<f32>,
    // A mesh's per-vertex data, interpolated.
    @location(3) data: vec4<f32>,
}

fn kulisa_vertex(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = globals.proj * vec4<f32>(in.pos, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    out.pixel = in.pos;
    out.data = in.data;
    return out;
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    return KULISA_VERTEX_ENTRY(in);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return fragment(in);
}
