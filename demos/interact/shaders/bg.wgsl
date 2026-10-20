// The backdrop: a deep indigo-to-teal gradient with a few soft glows and a faint dot grid.
// params.p[0] = (width, height, _, _) of the sprite in pixels.

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let p = in.uv * size;
    let t = globals.time;
    var col = mix(srgb(vec3<f32>(0.07, 0.06, 0.16)), srgb(vec3<f32>(0.05, 0.14, 0.2)), smoothstep(0.0, 1.0, in.uv.x * 0.6 + in.uv.y * 0.5));
    // Soft moving glows.
    let g1 = exp(-pow(length((p - vec2<f32>(size.x * 0.2, size.y * 0.3)) / 700.0), 2.0));
    let g2 = exp(-pow(length((p - vec2<f32>(size.x * 0.8 + 40.0 * sin(t * 0.2), size.y * 0.7)) / 800.0), 2.0));
    col = col + srgb(vec3<f32>(0.35, 0.2, 0.7)) * g1 * 0.22 + srgb(vec3<f32>(0.1, 0.6, 0.7)) * g2 * 0.18;
    // A faint dot grid.
    let cell = fract(p / 32.0) - vec2<f32>(0.5);
    col = col + srgb(vec3<f32>(0.6, 0.6, 0.9)) * smoothstep(0.1, 0.0, length(cell) - 0.04) * 0.06;
    // Vignette.
    let v = length(in.uv - vec2<f32>(0.5));
    col = col * (1.0 - smoothstep(0.35, 0.9, v) * 0.5);
    return vec4<f32>(col, 1.0) * in.color;
}
