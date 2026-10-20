// The front glass of a dual-chamber showcase case, drawn procedurally: a black interior lit by
// purple/blue/cyan neon trim, a water-cooled board on the left, a graphics card across the lower
// middle, a radiator and fans along the top, three fans on the floor.
// params.p[0] = (width, height, glass sheen, _) in face pixels.

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn hsv(h: f32, s: f32, v: f32) -> vec3<f32> {
    let k = vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0);
    let p = abs(fract(vec3<f32>(h) + k) * 6.0 - vec3<f32>(3.0));
    return v * mix(vec3<f32>(1.0), clamp(p - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0)), s);
}

// The trim color along 0..1: purple, blue, cyan.
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

// A line of light: a bright core and a soft halo.
fn line_light(d: f32, w: f32) -> f32 { return cov(d - w) + glow(d, w * 5.0) * 0.45; }

// A black fan with a glowing ring and hub.
fn fan(col: vec3<f32>, p: vec2<f32>, c: vec2<f32>, r: f32, t: f32, ring: vec3<f32>) -> vec3<f32> {
    let q = p - c;
    let d = length(q);
    if (d > r * 1.5) { return col; }
    var o = col;
    o = mix(o, srgb(vec3<f32>(0.018, 0.02, 0.026)), cov(d - r));
    let ang = atan2(q.y, q.x);
    let blade = smoothstep(-0.1, 0.5, sin(ang * 9.0 + d * 0.09 - t * 6.0));
    let band = cov(d - r * 0.86) * (1.0 - cov(d - r * 0.26));
    o = mix(o, srgb(vec3<f32>(0.075, 0.08, 0.1)), blade * band);
    o = o + ring * line_light(abs(d - r * 0.93), 1.1) * 0.85;
    o = mix(o, srgb(vec3<f32>(0.03, 0.032, 0.04)), cov(d - r * 0.26));
    o = o + ring * glow(d, r * 0.13) * 0.4;
    return o;
}

fn hex_holes(p: vec2<f32>, pitch: f32, radius: f32) -> f32 {
    var q = p / pitch;
    q.x = q.x + 0.5 * floor(q.y);
    let cell = fract(q) - vec2<f32>(0.5);
    return cov((length(cell) - radius) * pitch);
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let p = in.uv * size;
    let t = globals.time;
    let u = in.uv;
    let w = size.x;
    let h = size.y;

    // Interior: near-black with a cool bounce, lighter towards the floor.
    var col = mix(srgb(vec3<f32>(0.035, 0.038, 0.055)), srgb(vec3<f32>(0.02, 0.022, 0.035)), u.y);

    // The rear panel on the left: a honeycomb of holes with light behind it.
    let panel = sd_box(p - vec2<f32>(w * 0.085, h * 0.5), vec2<f32>(w * 0.085, h * 0.34));
    col = mix(col, srgb(vec3<f32>(0.03, 0.03, 0.04)), cov(panel));
    let holes = hex_holes(p, 17.0, 0.3) * cov(panel + 2.0);
    col = mix(col, srgb(vec3<f32>(0.55, 0.5, 0.75)), holes * 0.8);

    // The radiator across the top, with three fans.
    let rad = sd_round(p - vec2<f32>(w * 0.5, h * 0.085), vec2<f32>(w * 0.36, h * 0.05), 5.0);
    col = mix(col, srgb(vec3<f32>(0.025, 0.027, 0.035)), cov(rad));
    col = col + neon(0.1 + u.x * 0.3) * line_light(abs(rad), 1.0) * 0.8;
    for (var i = 0; i < 3; i = i + 1) {
        let c = vec2<f32>(w * (0.28 + 0.22 * f32(i)), h * 0.085);
        col = fan(col, p, c, h * 0.042, t * (1.0 + 0.1 * f32(i)), neon(0.15 + 0.2 * f32(i)));
    }

    // The fan on the top left, in front of the honeycomb.
    col = fan(col, p, vec2<f32>(w * 0.1, h * 0.2), h * 0.115, t * 1.1, neon(0.05));
    let frame_l = sd_round(p - vec2<f32>(w * 0.1, h * 0.2), vec2<f32>(h * 0.125, h * 0.125), 10.0);
    col = col + neon(0.0) * line_light(abs(frame_l), 0.9) * 0.55;

    // The motherboard: dark PCB with faint traces, a VRM heatsink and a chipset block.
    let mb = sd_round(p - vec2<f32>(w * 0.53, h * 0.45), vec2<f32>(w * 0.33, h * 0.29), 5.0);
    col = mix(col, srgb(vec3<f32>(0.03, 0.034, 0.05)), cov(mb));
    let g = abs(fract(p / 13.0) - 0.5);
    col = col + srgb(vec3<f32>(0.02, 0.025, 0.05)) * smoothstep(0.46, 0.5, max(g.x, g.y)) * cov(mb + 2.0);
    let vrm = sd_round(p - vec2<f32>(w * 0.31, h * 0.25), vec2<f32>(w * 0.1, h * 0.065), 4.0);
    col = mix(col, srgb(vec3<f32>(0.045, 0.05, 0.065)), cov(vrm));
    col = col + srgb(vec3<f32>(0.04, 0.045, 0.06)) * smoothstep(0.3, 0.5, abs(fract(p.x / 7.0) - 0.5)) * cov(vrm + 2.0);
    col = col + neon(0.3) * line_light(abs(vrm), 0.8) * 0.45;

    // Water block: a glowing cube with a round display.
    let pump_c = vec2<f32>(w * 0.45, h * 0.385);
    let pump = sd_round(p - pump_c, vec2<f32>(h * 0.085, h * 0.085), 6.0);
    col = mix(col, srgb(vec3<f32>(0.02, 0.022, 0.03)), cov(pump));
    col = col + neon(0.0) * line_light(abs(pump), 1.0) * 0.7;
    let pd = length(p - pump_c);
    col = mix(col, srgb(vec3<f32>(0.005, 0.01, 0.02)), cov(pd - h * 0.06));
    let ang = atan2(p.y - pump_c.y, p.x - pump_c.x);
    let arc = step(0.0, sin(ang * 0.5 + 0.8 + t * 0.3)) ;
    col = col + neon(0.8) * line_light(abs(pd - h * 0.05), 1.2) * (0.35 + 0.65 * arc);
    col = col + neon(0.7) * glow(pd, h * 0.03) * 0.5;

    // RAM: four sticks with light bars.
    for (var i = 0; i < 4; i = i + 1) {
        let c = vec2<f32>(w * (0.585 + 0.026 * f32(i)), h * 0.33);
        let ram = sd_round(p - c, vec2<f32>(w * 0.009, h * 0.13), 2.0);
        col = mix(col, srgb(vec3<f32>(0.04, 0.042, 0.055)), cov(ram));
        let bar = sd_round(p - c, vec2<f32>(w * 0.0035, h * 0.12), 1.5);
        let hue = fract(0.78 + 0.05 * f32(i) + 0.05 * sin(t * 0.6 + p.y * 0.03));
        col = mix(col, hsv(hue, 0.55, 1.0), cov(bar) * 0.85);
        col = col + hsv(hue, 0.7, 1.0) * glow(bar, 6.0) * 0.14;
    }

    // Two braided tubes from the block up to the radiator.
    let tubes = min(
        sd_seg(p, pump_c + vec2<f32>(-h * 0.04, -h * 0.085), vec2<f32>(w * 0.36, h * 0.17)),
        sd_seg(p, vec2<f32>(w * 0.36, h * 0.17), vec2<f32>(w * 0.33, h * 0.125))
    );
    let tubes2 = min(
        sd_seg(p, pump_c + vec2<f32>(h * 0.04, -h * 0.085), vec2<f32>(w * 0.5, h * 0.15)),
        sd_seg(p, vec2<f32>(w * 0.5, h * 0.15), vec2<f32>(w * 0.5, h * 0.125))
    );
    let tube = min(tubes, tubes2);
    col = mix(col, srgb(vec3<f32>(0.012, 0.012, 0.016)), cov(tube - 6.0));
    col = col + srgb(vec3<f32>(0.2, 0.22, 0.3)) * smoothstep(0.35, 0.5, abs(fract((p.x * 0.7 + p.y) / 4.0) - 0.5)) * cov(tube - 6.0) * 0.12;
    col = col + srgb(vec3<f32>(0.5, 0.55, 0.8)) * cov(abs(tube - 3.0) - 0.8) * cov(tube - 6.0) * 0.1;

    // The graphics card: a long black body with three fans, a backplate, and a glowing end block.
    let gc = vec2<f32>(w * 0.5, h * 0.625);
    let gpu = sd_round(p - gc, vec2<f32>(w * 0.33, h * 0.075), 7.0);
    col = mix(col, srgb(vec3<f32>(0.015, 0.016, 0.02)), cov(gpu));
    col = col + srgb(vec3<f32>(0.1, 0.11, 0.15)) * glow(gpu + 2.0, 1.4) * 0.5;
    for (var i = 0; i < 3; i = i + 1) {
        let c = vec2<f32>(w * (0.3 + 0.2 * f32(i)), gc.y);
        col = fan(col, p, c, h * 0.062, t * (1.0 + 0.15 * f32(i)), neon(0.55 + 0.15 * f32(i)));
    }
    let edge = abs(p.y - (gc.y + h * 0.075)) - 0.8;
    col = col + neon(u.x) * (cov(edge) + glow(p.y - (gc.y + h * 0.075), 4.0) * 0.4) * cov(gpu + 6.0) * 0.9;
    let end = sd_round(p - vec2<f32>(w * 0.835, h * 0.625), vec2<f32>(w * 0.032, h * 0.082), 6.0);
    col = mix(col, srgb(vec3<f32>(0.4, 0.2, 0.75)), cov(end) * 0.9);
    let rib = smoothstep(0.3, 0.5, abs(fract(p.y / 6.0) - 0.5));
    col = col + srgb(vec3<f32>(0.6, 0.4, 1.0)) * rib * cov(end + 3.0) * 0.4;
    col = col + srgb(vec3<f32>(0.6, 0.3, 1.0)) * glow(end, 12.0) * 0.18;

    // The floor of the case: three fans in a trimmed tray.
    let tray = sd_round(p - vec2<f32>(w * 0.49, h * 0.855), vec2<f32>(w * 0.37, h * 0.085), 6.0);
    col = mix(col, srgb(vec3<f32>(0.02, 0.022, 0.03)), cov(tray));
    col = col + neon(0.55 + 0.4 * u.x) * line_light(abs(tray), 1.0) * 0.9;
    for (var i = 0; i < 3; i = i + 1) {
        let c = vec2<f32>(w * (0.27 + 0.22 * f32(i)), h * 0.855);
        col = fan(col, p, c, h * 0.068, -t * (1.0 + 0.1 * f32(i)), neon(0.6 + 0.15 * f32(i)));
    }

    // Soft glow washing over everything from the trims.
    col = col + neon(u.x) * 0.03 * smoothstep(0.7, 1.0, u.y);
    col = col + neon(0.1) * 0.02 * smoothstep(0.3, 0.0, u.y);

    // Tempered glass: a dark tint, the bezel, and the neon strips running along the top and bottom.
    col = col * 0.9;
    let fr = sd_round(p - size * 0.5, size * 0.5, 6.0);
    let bezel = clamp(-fr / 8.0, 0.0, 1.0);
    col = mix(srgb(vec3<f32>(0.008, 0.008, 0.012)), col, smoothstep(0.0, 1.0, bezel));
    col = col + neon(u.x) * cov(abs(p.y - 3.5) - 1.3) * 0.95;
    col = col + neon(u.x) * glow(p.y - 3.5, 7.0) * 0.14;
    col = col + neon(1.0 - u.x) * cov(abs(p.y - (h - 3.5)) - 1.3) * 0.95;
    col = col + neon(1.0 - u.x) * glow(p.y - (h - 3.5), 7.0) * 0.18;
    // Reflections: a tall soft window and a thin sheen.
    let diag = u.x * 0.9 - u.y * 0.5;
    let sheen = smoothstep(0.06, 0.0, abs(diag - 0.18)) * 0.35 + smoothstep(0.18, 0.0, abs(diag - 0.5)) * 0.08;
    col = col + srgb(vec3<f32>(0.75, 0.85, 1.0)) * sheen * params.p[0].z * cov(fr + 8.0);
    return vec4<f32>(col * cov(fr), cov(fr)) * in.color;
}
