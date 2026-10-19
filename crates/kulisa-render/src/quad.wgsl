struct Globals {
    tint: vec4<f32>,
    // cos and sin of the rotation angle
    rot: vec2<f32>,
    scale: vec2<f32>,
    // 1.0 when the target is not sRGB and the shader must encode
    encode_srgb: f32,
}

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    // Triangle strip covering [-1, 1]^2.
    let c = vec2<f32>(f32(i & 1u) * 2.0 - 1.0, f32(i >> 1u) * 2.0 - 1.0);
    let r = vec2<f32>(c.x * g.rot.x - c.y * g.rot.y, c.x * g.rot.y + c.y * g.rot.x);
    var out: VsOut;
    out.pos = vec4<f32>(r * g.scale, 0.0, 1.0);
    out.uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    return out;
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Texture and tint are premultiplied.
    let c = textureSample(tex, samp, in.uv) * g.tint;
    if g.encode_srgb > 0.5 && c.a > 0.0 {
        // Approximate: encodes after premultiplying, exact for opaque pixels.
        return vec4<f32>(linear_to_srgb(c.rgb / c.a) * c.a, c.a);
    }
    return c;
}
