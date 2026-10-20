// A CRT monitor: curved glass, scanlines, an RGB shadow mask, color fringes and a dark bezel.
// p[0] = (curvature, scanline strength, mask strength, chromatic aberration).
fn fragment(in: VertexOut) -> vec4<f32> {
    let res = globals.resolution;
    var uv = in.uv * 2.0 - 1.0;
    let curve = params.p[0].x;
    uv *= 1.0 + curve * dot(uv.yx, uv.yx) * 0.5;
    uv = uv * 0.5 + 0.5;
    // Outside the curved glass is black. Sample first and mask after: texture sampling must stay
    // in uniform control flow (WebGPU rejects the shader otherwise).
    let inside = step(0.0, uv.x) * step(0.0, uv.y) * step(uv.x, 1.0) * step(uv.y, 1.0);
    let ca = params.p[0].w / res.x * (uv.x - 0.5) * 2.0;
    let r = textureSample(tex, samp, uv + vec2<f32>(ca, 0.0)).r;
    let g = textureSample(tex, samp, uv).g;
    let b = textureSample(tex, samp, uv - vec2<f32>(ca, 0.0)).b;
    var col = vec3<f32>(r, g, b);
    let line = 0.5 + 0.5 * cos(uv.y * res.y * 3.14159 / 1.5);
    col *= 1.0 - params.p[0].y * (1.0 - line) * 0.6;
    let m = fract(in.pixel.x / 3.0);
    let mask = vec3<f32>(step(m, 0.333), step(0.333, m) * step(m, 0.666), step(0.666, m));
    col *= mix(vec3<f32>(1.0), mask * 2.2 + 0.25, params.p[0].z * 0.35);
    let v = uv * (1.0 - uv.yx);
    col *= pow(clamp(v.x * v.y * 18.0, 0.0, 1.0), 0.25);
    return vec4<f32>(col * 1.1 * inside, 1.0);
}
