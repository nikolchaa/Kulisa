// The case's top panel: a magnetic dust filter, and the front I/O strip (USB ports, jacks, power
// button). uv.x runs left to right, uv.y from the front edge to the back. params.p[0] = (width,
// depth, _, _) in face pixels, so shapes stay round on a foreshortened face.

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

fn cov(d: f32) -> f32 { return clamp(0.5 - d, 0.0, 1.0); }

fn glow(d: f32, k: f32) -> f32 { return exp(-d * d / (k * k)); }

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let p = in.uv * size;
    let t = globals.time;

    // Brushed graphite: fine streaks along the width.
    let streaks = fract(sin(floor(p.y * 1.7) * 91.7) * 4375.5) * 0.04;
    var col = srgb(vec3<f32>(0.105, 0.11, 0.13)) + vec3<f32>(streaks * 0.05);

    // Dust filter: a recessed panel perforated with a hex pattern.
    let fc = vec2<f32>(size.x * 0.5, size.y * 0.58);
    let fh = vec2<f32>(size.x * 0.46, size.y * 0.33);
    let fd = sd_round(p - fc, fh, 8.0);
    col = mix(col, srgb(vec3<f32>(0.045, 0.048, 0.06)), cov(fd));
    let pitch = 11.0;
    var q = p / pitch;
    q.x = q.x + 0.5 * floor(q.y);
    let cell = fract(q) - vec2<f32>(0.5);
    let hole = cov((length(cell * vec2<f32>(1.0, 1.0)) - 0.26) * pitch);
    col = mix(col, srgb(vec3<f32>(0.01, 0.011, 0.015)), hole * cov(fd + 4.0) * 0.9);
    col = col + srgb(vec3<f32>(0.3, 0.34, 0.42)) * glow(fd - 0.5, 1.2) * 0.2;

    // Front I/O strip: a darker bar with ports.
    let sc = vec2<f32>(size.x * 0.5, size.y * 0.1);
    let sh = vec2<f32>(size.x * 0.46, size.y * 0.062);
    let sd = sd_round(p - sc, sh, 6.0);
    col = mix(col, srgb(vec3<f32>(0.04, 0.042, 0.052)), cov(sd));
    for (var i = 0; i < 4; i = i + 1) {
        let c = vec2<f32>(size.x * (0.2 + 0.065 * f32(i)), sc.y);
        let port = sd_round(p - c, vec2<f32>(15.0, 6.5), 1.5);
        col = mix(col, srgb(vec3<f32>(0.01)), cov(port));
        let tongue = sd_box(p - c - vec2<f32>(0.0, 1.5), vec2<f32>(11.0, 1.7));
        let blue = select(srgb(vec3<f32>(0.1, 0.45, 0.95)), srgb(vec3<f32>(0.9, 0.92, 0.95)), i > 1);
        col = mix(col, blue, cov(tongue));
    }
    for (var j = 0; j < 2; j = j + 1) {
        let c = vec2<f32>(size.x * (0.52 + 0.04 * f32(j)), sc.y);
        let jack = length(p - c);
        col = mix(col, srgb(vec3<f32>(0.01)), cov(jack - 6.0));
        col = col + srgb(vec3<f32>(0.5, 0.55, 0.65)) * glow(jack - 7.5, 0.8) * 0.35;
        col = mix(col, srgb(vec3<f32>(0.15, 0.16, 0.2)), cov(jack - 2.5) * 0.0);
    }
    // Power button with a breathing ring.
    let pc = vec2<f32>(size.x * 0.8, sc.y);
    let pd = length(p - pc);
    col = mix(col, srgb(vec3<f32>(0.14, 0.15, 0.18)), cov(pd - 12.0));
    col = mix(col, srgb(vec3<f32>(0.05, 0.055, 0.07)), cov(pd - 9.0));
    let breathe = 0.65 + 0.35 * sin(t * 1.6);
    col = col + hsv(0.58, 0.8, 1.0) * glow(pd - 10.5, 1.4) * breathe;

    // Chamfered front and side edges.
    let e = min(min(p.x, size.x - p.x), p.y);
    col = col + srgb(vec3<f32>(0.7, 0.78, 0.95)) * glow(e - 1.0, 1.5) * 0.18;

    // A soft light gradient across the surface.
    col = col + srgb(vec3<f32>(0.8, 0.88, 1.0)) * smoothstep(0.7, 0.0, in.uv.y) * 0.02;
    return vec4<f32>(col, 1.0) * in.color;
}
