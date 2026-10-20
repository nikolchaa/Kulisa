// The PC seen through its tempered-glass side panel, drawn procedurally (no art assets).
// params.p[0] = (width, height, glass sheen strength, _) in pixels.
// Colors below are written as sRGB-looking values and converted to linear by srgb().

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn hsv(h: f32, s: f32, v: f32) -> vec3<f32> {
    let k = vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0);
    let p = abs(fract(vec3<f32>(h) + k) * 6.0 - vec3<f32>(3.0));
    return v * mix(vec3<f32>(1.0), clamp(p - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0)), s);
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

// Pixel coverage of a signed distance (1 px anti-aliasing).
fn cov(d: f32) -> f32 { return clamp(0.5 - d, 0.0, 1.0); }

fn glow(d: f32, k: f32) -> f32 { return exp(-d * d / (k * k)); }

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn fan(col: vec3<f32>, p: vec2<f32>, c: vec2<f32>, r: f32, t: f32, hue: f32) -> vec3<f32> {
    let q = p - c;
    let d = length(q);
    var o = col;
    o = mix(o, srgb(vec3<f32>(0.055, 0.06, 0.075)), cov(d - r));
    let ring = glow(d - r * 0.9, r * 0.05);
    o = o + hsv(hue, 0.85, 1.0) * ring * 0.95;
    let ang = atan2(q.y, q.x);
    let blade = smoothstep(0.0, 0.55, sin(ang * 7.0 + d * 0.07 - t * 5.0));
    let body = cov(d - r * 0.84) * cov(r * 0.2 - d + 0.0) ;
    let ring_mask = cov(d - r * 0.84) * (1.0 - cov(d - r * 0.2));
    o = mix(o, srgb(vec3<f32>(0.2, 0.21, 0.25)), blade * ring_mask * 0.8);
    o = mix(o, srgb(vec3<f32>(0.035, 0.035, 0.045)), cov(d - r * 0.2));
    o = o + hsv(hue + 0.08, 0.7, 1.0) * glow(d, r * 0.12) * 0.35;
    return o + vec3<f32>(body * 0.0);
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let p = in.uv * size;
    let t = globals.time;
    let u = in.uv;

    // Chassis interior: a dark vertical gradient with a faint blue bounce from the lights.
    var col = mix(srgb(vec3<f32>(0.055, 0.06, 0.08)), srgb(vec3<f32>(0.03, 0.032, 0.045)), u.y);

    // Rear wall cut-outs: faint horizontal slats.
    let slat = smoothstep(0.35, 0.5, abs(fract(p.y / 14.0) - 0.5));
    col = col + srgb(vec3<f32>(0.018, 0.02, 0.026)) * slat * 0.5;

    // PSU shroud along the bottom with vents.
    let sh = sd_round(p - vec2<f32>(size.x * 0.34, size.y * 0.915), vec2<f32>(size.x * 0.34, size.y * 0.085), 4.0);
    col = mix(col, srgb(vec3<f32>(0.1, 0.105, 0.125)), cov(sh));
    let vent = step(0.55, fract(p.x / 9.0)) * step(abs(p.y - size.y * 0.93), 11.0) * cov(sh + 6.0);
    col = mix(col, srgb(vec3<f32>(0.02, 0.02, 0.026)), vent * 0.85);
    col = col + hsv(0.60 + 0.1 * sin(t * 0.4), 0.8, 1.0) * glow(sh + 1.0, 2.5) * 0.35 * (1.0 - cov(sh + 2.0));

    // Motherboard.
    let mb_c = vec2<f32>(size.x * 0.33, size.y * 0.42);
    let mb_h = vec2<f32>(size.x * 0.295, size.y * 0.37);
    let mb = sd_round(p - mb_c, mb_h, 6.0);
    col = mix(col, srgb(vec3<f32>(0.07, 0.075, 0.1)), cov(mb));
    // Traces: a coarse grid of thin lines, brighter near a few hubs.
    let g = abs(fract(p / 11.0) - 0.5);
    let trace = smoothstep(0.46, 0.5, max(g.x, g.y)) * 0.5 + smoothstep(0.47, 0.5, min(g.x, g.y)) * 0.25;
    col = col + srgb(vec3<f32>(0.03, 0.05, 0.07)) * trace * cov(mb + 3.0);
    let rim = glow(mb, 1.6) * 0.3;
    col = col + srgb(vec3<f32>(0.2, 0.5, 0.8)) * rim * 0.12;

    // CPU water block with an RGB ring, and two tubes leaving through the top.
    let pump_c = vec2<f32>(size.x * 0.29, size.y * 0.27);
    let pr = size.y * 0.095;
    let tubes = min(
        sd_seg(p, pump_c + vec2<f32>(-pr * 0.5, -pr * 0.7), pump_c + vec2<f32>(-pr * 0.9, -size.y * 0.3)),
        sd_seg(p, pump_c + vec2<f32>(pr * 0.5, -pr * 0.7), pump_c + vec2<f32>(pr * 1.1, -size.y * 0.3))
    );
    col = mix(col, srgb(vec3<f32>(0.1, 0.1, 0.12)), cov(tubes - 5.0));
    col = col + srgb(vec3<f32>(0.4, 0.45, 0.6)) * cov(tubes - 1.0) * (1.0 - cov(tubes - 5.0)) * 0.0;
    let pd = length(p - pump_c);
    col = mix(col, srgb(vec3<f32>(0.1, 0.11, 0.14)), cov(pd - pr));
    let pang = atan2(p.y - pump_c.y, p.x - pump_c.x);
    let hue_p = fract(pang / 6.2832 + t * 0.1);
    col = col + hsv(hue_p, 0.85, 1.0) * glow(pd - pr * 0.82, pr * 0.07) * 1.0;
    col = mix(col, srgb(vec3<f32>(0.04, 0.045, 0.06)), cov(pd - pr * 0.68));
    col = col + hsv(hue_p, 0.6, 1.0) * glow(pd, pr * 0.28) * 0.18;

    // RAM sticks with RGB light bars.
    for (var i = 0; i < 4; i = i + 1) {
        let cx = size.x * (0.45 + 0.036 * f32(i));
        let rc = vec2<f32>(cx, size.y * 0.24);
        let rh = vec2<f32>(size.x * 0.0125, size.y * 0.165);
        let rd = sd_round(p - rc, rh, 2.0);
        col = mix(col, srgb(vec3<f32>(0.12, 0.125, 0.15)), cov(rd));
        let bar = sd_round(p - (rc - vec2<f32>(0.0, rh.y - 5.0)), vec2<f32>(rh.x - 3.0, 3.0), 1.5);
        let hue = fract(0.55 + 0.12 * f32(i) + t * 0.12);
        col = mix(col, hsv(hue, 0.9, 1.0), cov(bar));
        col = col + hsv(hue, 0.9, 1.0) * glow(bar, 7.0) * 0.28;
    }

    // Chipset heatsink and VRM blocks.
    let hs = sd_round(p - vec2<f32>(size.x * 0.4, size.y * 0.6), vec2<f32>(size.x * 0.07, size.y * 0.07), 3.0);
    col = mix(col, srgb(vec3<f32>(0.13, 0.135, 0.165)), cov(hs));
    let fin = smoothstep(0.4, 0.5, abs(fract(p.y / 5.0) - 0.5));
    col = col + srgb(vec3<f32>(0.05, 0.055, 0.07)) * fin * cov(hs + 2.0);
    let vrm = sd_round(p - vec2<f32>(size.x * 0.09, size.y * 0.25), vec2<f32>(size.x * 0.022, size.y * 0.15), 3.0);
    col = mix(col, srgb(vec3<f32>(0.12, 0.125, 0.15)), cov(vrm));
    col = col + srgb(vec3<f32>(0.05, 0.055, 0.07)) * smoothstep(0.4, 0.5, abs(fract(p.y / 6.0) - 0.5)) * cov(vrm + 2.0);

    // Graphics card: backplate, shroud, three fans, glowing edge.
    let gc = vec2<f32>(size.x * 0.335, size.y * 0.72);
    let gh = vec2<f32>(size.x * 0.315, size.y * 0.075);
    let gd = sd_round(p - gc, gh, 5.0);
    col = mix(col, srgb(vec3<f32>(0.075, 0.08, 0.1)), cov(gd));
    col = col + srgb(vec3<f32>(0.1, 0.11, 0.14)) * glow(gd + 1.5, 1.2) * 0.5;
    for (var k = 0; k < 3; k = k + 1) {
        let fc = vec2<f32>(size.x * (0.155 + 0.18 * f32(k)), gc.y);
        col = fan(col, p, fc, size.y * 0.062, t * (1.0 + 0.1 * f32(k)), fract(0.7 + 0.2 * f32(k) + t * 0.08));
    }
    let edge = sd_seg(p, vec2<f32>(size.x * 0.03, gc.y + gh.y - 5.0), vec2<f32>(size.x * 0.64, gc.y + gh.y - 5.0));
    col = col + hsv(fract(t * 0.1 + u.x), 0.9, 1.0) * glow(edge, 2.0) * 0.55;

    // Intake fan column on the right (three 140 mm fans).
    for (var k = 0; k < 3; k = k + 1) {
        let fc = vec2<f32>(size.x * 0.835, size.y * (0.19 + 0.31 * f32(k)));
        col = fan(col, p, fc, size.y * 0.142, -t * (0.9 + 0.1 * f32(k)), fract(0.05 + 0.18 * f32(k) + t * 0.07));
    }

    // Cables: a braided bundle from the PSU to the board.
    let cb = min(
        sd_seg(p, vec2<f32>(size.x * 0.63, size.y * 0.86), vec2<f32>(size.x * 0.61, size.y * 0.56)),
        sd_seg(p, vec2<f32>(size.x * 0.61, size.y * 0.56), vec2<f32>(size.x * 0.58, size.y * 0.33))
    );
    col = mix(col, srgb(vec3<f32>(0.03, 0.032, 0.04)), cov(cb - 5.0));
    col = col + srgb(vec3<f32>(0.08)) * smoothstep(0.42, 0.5, abs(fract((p.y + p.x * 0.4) / 4.0) - 0.5)) * cov(cb - 5.0) * 0.5;

    // Atmosphere: soft colored haze from the lights.
    col = col + hsv(fract(0.6 + t * 0.03), 0.6, 1.0) * 0.02 * (1.0 - u.y);

    // Tempered glass: dark tint, rounded frame, specular streaks, corner chamfer.
    col = col * 0.82;
    let fr = sd_round(p - size * 0.5, size * 0.5, 14.0);
    let bezel = clamp(-fr / 9.0, 0.0, 1.0);
    col = mix(srgb(vec3<f32>(0.02, 0.02, 0.025)), col, smoothstep(0.0, 1.0, bezel));
    let chamfer = glow(fr + 9.0, 1.5) * 0.35;
    col = col + srgb(vec3<f32>(0.6, 0.68, 0.8)) * chamfer * 0.25;
    let diag = u.x * 0.8 - u.y * 0.55;
    let streak = smoothstep(0.02, 0.0, abs(diag - 0.12 + 0.03 * sin(u.y * 3.0))) * 0.5
        + smoothstep(0.05, 0.0, abs(diag - 0.38)) * 0.25;
    col = col + srgb(vec3<f32>(0.75, 0.85, 1.0)) * streak * params.p[0].z * cov(fr + 10.0);
    let gloss = smoothstep(0.7, 0.0, u.y) * 0.05;
    col = col + srgb(vec3<f32>(0.8, 0.9, 1.0)) * gloss * params.p[0].z;

    return vec4<f32>(col * cov(fr), cov(fr)) * in.color;
}
