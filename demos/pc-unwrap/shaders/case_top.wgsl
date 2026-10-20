// The case's top panel: black steel with a field of fine round perforations, a neon line along the
// front edge and the I/O cluster (two USB ports, a jack, the power button) at the right front.
// uv.x runs left to right along the front glass, uv.y from the front edge to the back.
// params.p[0] = (width, depth, _, _) in face pixels, so shapes stay round on a foreshortened face.

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

fn cov(d: f32) -> f32 { return clamp(0.5 - d, 0.0, 1.0); }

fn glow(d: f32, k: f32) -> f32 { return exp(-d * d / (k * k)); }

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let p = in.uv * size;
    let t = globals.time;
    var col = srgb(vec3<f32>(0.035, 0.037, 0.045));
    // Brushed streaks along the length.
    col = col + srgb(vec3<f32>(0.05)) * fract(sin(floor(p.y * 1.3) * 91.7) * 4375.5) * 0.12;

    // The perforated field.
    let field = sd_round(p - size * vec2<f32>(0.5, 0.55), size * vec2<f32>(0.47, 0.4), 6.0);
    var q = p / 7.0;
    q.x = q.x + 0.5 * floor(q.y);
    let cell = fract(q) - vec2<f32>(0.5);
    let hole = cov((length(cell) - 0.27) * 7.0) * cov(field + 3.0);
    col = mix(col, srgb(vec3<f32>(0.004, 0.005, 0.01)), hole);
    col = col + srgb(vec3<f32>(0.4, 0.45, 0.6)) * cov(abs(field) - 0.6) * 0.18;

    // Light line along the front edge.
    col = col + neon(in.uv.x) * (cov(abs(p.y - 6.0) - 1.4) + glow(p.y - 6.0, 6.0) * 0.25) * 0.9;

    // I/O cluster, right front.
    for (var i = 0; i < 2; i = i + 1) {
        let c = vec2<f32>(size.x * (0.83 + 0.04 * f32(i)), 30.0);
        let port = sd_round(p - c, vec2<f32>(11.0, 5.5), 1.5);
        col = mix(col, srgb(vec3<f32>(0.004)), cov(port));
        col = mix(col, srgb(vec3<f32>(0.1, 0.45, 0.95)), cov(sd_box(p - c - vec2<f32>(0.0, 1.5), vec2<f32>(8.0, 1.4))));
    }
    let jack = length(p - vec2<f32>(size.x * 0.915, 30.0));
    col = mix(col, srgb(vec3<f32>(0.004)), cov(jack - 4.5));
    col = col + srgb(vec3<f32>(0.5, 0.55, 0.65)) * glow(jack - 5.5, 0.7) * 0.4;
    let pc = vec2<f32>(size.x * 0.955, 30.0);
    let pd = length(p - pc);
    col = mix(col, srgb(vec3<f32>(0.1, 0.11, 0.14)), cov(pd - 10.0));
    col = mix(col, srgb(vec3<f32>(0.03, 0.035, 0.05)), cov(pd - 7.5));
    col = col + neon(0.7) * glow(pd - 8.8, 1.2) * (0.65 + 0.35 * sin(t * 1.6));

    // A long soft reflection.
    col = col + srgb(vec3<f32>(0.8, 0.88, 1.0)) * smoothstep(0.75, 0.0, in.uv.y) * 0.03;
    return vec4<f32>(col, 1.0) * in.color;
}
