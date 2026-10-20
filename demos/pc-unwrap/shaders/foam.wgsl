// A block of expanded polystyrene packing foam: white beads, rounded edges, and a notch where the
// corner of the case sits. params.p[0] = (width, height, notch width, notch height);
// params.p[1].x = 0 when the notch is at the bottom right, 1 at the bottom left.

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn sd_box(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn cov(d: f32) -> f32 { return clamp(0.5 - d, 0.0, 1.0); }

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    var p = in.uv * size - size * 0.5;
    if (params.p[1].x > 0.5) { p.x = -p.x; }
    // Body: a rounded box minus the notch in the bottom-right corner.
    let body = sd_box(p, size * 0.5 - vec2<f32>(12.0)) - 12.0;
    let nsize = params.p[0].zw;
    let notch = sd_box(p - (size * 0.5 - nsize * 0.5), nsize * 0.5 + vec2<f32>(6.0)) - 6.0;
    let d = max(body, -notch);
    // Light from the upper left; the beads give a fine grain.
    let cell = floor(in.uv * size / 3.0);
    let beads = hash21(cell) * 0.5 + hash21(floor(in.uv * size / 9.0)) * 0.5;
    var col = vec3<f32>(0.9 + 0.08 * beads);
    col = col * (1.0 - 0.18 * smoothstep(-0.4, 0.5, p.y / size.y) );
    col = col * (0.82 + 0.18 * smoothstep(0.0, 22.0, -d));
    col = col + vec3<f32>(0.06) * smoothstep(5.0, 0.0, abs(d + 3.0)) * step(0.0, -(p.x + p.y));
    col = srgb(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)));
    let shadow = (1.0 - smoothstep(-4.0, 14.0, d)) * 0.3;
    let a = cov(d);
    return vec4<f32>(col * a, a + shadow * (1.0 - a)) * in.color;
}
