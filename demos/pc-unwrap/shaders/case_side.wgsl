// The side glass: a column of three big fans on a radiator with round status displays, an
// angular blue trim, a honeycomb panel at the top, seen through dark glass.
// params.p[0] = (width, height, glass sheen, _) in face pixels; uv.x runs away from the corner.

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn neon(t: f32) -> vec3<f32> {
    let a = vec3<f32>(0.72, 0.22, 1.0);
    let b = vec3<f32>(0.18, 0.42, 1.0);
    let c = vec3<f32>(0.1, 0.85, 1.0);
    return srgb(mix(mix(a, b, smoothstep(0.0, 0.55, t)), c, smoothstep(0.5, 1.0, t)));
}

fn sd_box(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_round(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 { return sd_box(p, b - vec2<f32>(r)) - r; }

fn sd_seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

fn cov(d: f32) -> f32 { return clamp(0.5 - d, 0.0, 1.0); }

fn glow(d: f32, k: f32) -> f32 { return exp(-d * d / (k * k)); }

fn line_light(d: f32, w: f32) -> f32 { return cov(d - w) + glow(d, w * 5.0) * 0.45; }

fn hex_holes(p: vec2<f32>, pitch: f32, radius: f32) -> f32 {
    var q = p / pitch;
    q.x = q.x + 0.5 * floor(q.y);
    let cell = fract(q) - vec2<f32>(0.5);
    return cov((length(cell) - radius) * pitch);
}

// A fan with a round display in the hub: a ring that fills to `value`.
fn fan_display(col: vec3<f32>, p: vec2<f32>, c: vec2<f32>, r: f32, t: f32, value: f32, tone: vec3<f32>) -> vec3<f32> {
    let q = p - c;
    let d = length(q);
    if (d > r * 1.5) { return col; }
    var o = col;
    o = mix(o, srgb(vec3<f32>(0.016, 0.018, 0.024)), cov(d - r));
    let ang = atan2(q.y, q.x);
    let blade = smoothstep(-0.1, 0.5, sin(ang * 9.0 + d * 0.07 - t * 5.0));
    let band = cov(d - r * 0.93) * (1.0 - cov(d - r * 0.42));
    o = mix(o, srgb(vec3<f32>(0.07, 0.075, 0.095)), blade * band);
    // Hub: a dark screen with a ring gauge.
    o = mix(o, srgb(vec3<f32>(0.004, 0.008, 0.02)), cov(d - r * 0.4));
    let turn = fract((ang + 1.5708) / 6.2832);
    let on = step(turn, value);
    o = o + tone * (0.25 + 0.75 * on) * cov(abs(d - r * 0.3) - 2.2) * 0.9;
    o = o + tone * glow(d, r * 0.1) * 0.25;
    o = o + tone * glow(abs(d - r * 0.4), 2.0) * 0.4;
    return o;
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let p = in.uv * size;
    let t = globals.time;
    let u = in.uv;
    let w = size.x;
    let h = size.y;

    var col = mix(srgb(vec3<f32>(0.03, 0.034, 0.05)), srgb(vec3<f32>(0.015, 0.016, 0.026)), u.y);

    // Honeycomb panel high on the far edge.
    let panel = sd_box(p - vec2<f32>(w * 0.84, h * 0.13), vec2<f32>(w * 0.15, h * 0.12));
    col = mix(col, srgb(vec3<f32>(0.03, 0.03, 0.04)), cov(panel));
    col = mix(col, srgb(vec3<f32>(0.55, 0.5, 0.75)), hex_holes(p, 15.0, 0.3) * cov(panel + 2.0) * 0.7);

    // The radiator slab behind the fans.
    let slab = sd_round(p - vec2<f32>(w * 0.44, h * 0.5), vec2<f32>(w * 0.27, h * 0.485), 8.0);
    col = mix(col, srgb(vec3<f32>(0.022, 0.024, 0.032)), cov(slab));

    // Angular blue trim: chevrons between the fans and a strip down the far edge.
    var trim = 1e9;
    trim = min(trim, sd_seg(p, vec2<f32>(w * 0.12, h * 0.03), vec2<f32>(w * 0.22, h * 0.0 + 4.0)));
    for (var i = 0; i < 2; i = i + 1) {
        let y = h * (0.335 + 0.33 * f32(i));
        trim = min(trim, sd_seg(p, vec2<f32>(w * 0.1, y - 14.0), vec2<f32>(w * 0.02, y)));
        trim = min(trim, sd_seg(p, vec2<f32>(w * 0.02, y), vec2<f32>(w * 0.1, y + 14.0)));
        trim = min(trim, sd_seg(p, vec2<f32>(w * 0.74, y - 14.0), vec2<f32>(w * 0.82, y)));
        trim = min(trim, sd_seg(p, vec2<f32>(w * 0.82, y), vec2<f32>(w * 0.74, y + 14.0)));
    }
    trim = min(trim, sd_seg(p, vec2<f32>(w * 0.74, h * 0.06), vec2<f32>(w * 0.74, h * 0.95)));
    col = col + neon(0.3 + 0.7 * u.y) * line_light(trim, 1.1) * 0.9;

    // Three big fans with displays; the gauges fill to different values.
    for (var i = 0; i < 3; i = i + 1) {
        let c = vec2<f32>(w * 0.44, h * (0.18 + 0.32 * f32(i)));
        let value = 0.3 + 0.2 * f32(i) + 0.04 * sin(t * 0.7 + f32(i));
        col = fan_display(col, p, c, h * 0.145, t * (1.0 + 0.1 * f32(i)), value, neon(0.55 + 0.2 * f32(i)));
    }

    // Vertical strip of light on the far edge.
    col = col + neon(0.2 + 0.8 * u.y) * cov(abs(p.x - w * 0.92) - 1.6) * 0.9;
    col = col + neon(0.2 + 0.8 * u.y) * glow(p.x - w * 0.92, 9.0) * 0.14;

    col = col * 0.9;
    let fr = sd_round(p - size * 0.5, size * 0.5, 6.0);
    let bezel = clamp(-fr / 8.0, 0.0, 1.0);
    col = mix(srgb(vec3<f32>(0.008, 0.008, 0.012)), col, smoothstep(0.0, 1.0, bezel));
    col = col + neon(u.x) * cov(abs(p.y - 3.5) - 1.3) * 0.95;
    col = col + neon(1.0 - u.x) * cov(abs(p.y - (h - 3.5)) - 1.3) * 0.95;
    let diag = u.x * 0.8 - u.y * 0.6;
    let sheen = smoothstep(0.08, 0.0, abs(diag - 0.1)) * 0.3 + smoothstep(0.2, 0.0, abs(diag - 0.4)) * 0.06;
    col = col + srgb(vec3<f32>(0.75, 0.85, 1.0)) * sheen * params.p[0].z * cov(fr + 8.0);
    return vec4<f32>(col * cov(fr), cov(fr)) * in.color;
}
