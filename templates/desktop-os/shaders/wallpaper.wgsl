// The desktop wallpaper: a night sky in three colors with slow aurora curtains, stars, a glow on
// the horizon and a little grain. Params: p[0].xyz deep color, p[1].xyz middle color, p[2].xyz
// glow color (all linear), p[0].w flow speed, p[1].w aspect (width / height), p[2].w seed.

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x),
               mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

fn fbm(p0: vec2<f32>) -> f32 {
    var p = p0;
    var s = 0.0;
    var a = 0.5;
    for (var i = 0; i < 4; i++) {
        s += a * noise(p);
        p = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * p;
        a *= 0.5;
    }
    return s;
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let deep = params.p[0].xyz;
    let mid = params.p[1].xyz;
    let glow = params.p[2].xyz;
    let speed = params.p[0].w;
    let aspect = max(params.p[1].w, 0.1);
    let t = globals.time * speed + params.p[2].w * 10.0;
    let uv = in.uv;
    let x = uv.x * aspect;

    // Sky: deep at the top, the middle color towards the horizon, with slow cloudy variation.
    let n = fbm(vec2<f32>(x * 1.2 + t, uv.y * 2.0 - t * 0.5));
    var col = mix(deep * 0.5, mid * 0.55, smoothstep(0.1, 0.95, uv.y + (n - 0.5) * 0.3));

    // Stars: one sub-pixel point per 4x4-pixel cell at most, twinkling, fading lower down.
    let cell = floor(in.pixel / 4.0);
    let sp = (vec2<f32>(hash(cell + 3.1), hash(cell + 9.7)) * 3.0 + 0.5) - (in.pixel - cell * 4.0);
    let star = step(0.985, hash(cell)) * exp(-dot(sp, sp) * 1.6)
        * (0.55 + 0.45 * sin(globals.time * 1.7 + hash(cell + 7.0) * 40.0));
    col += vec3<f32>(star) * (1.0 - smoothstep(0.05, 0.55, uv.y)) * 0.9;

    // Aurora curtains: bright wavering edges hanging down into vertical rays.
    for (var i = 0; i < 3; i++) {
        let fi = f32(i);
        let edge = 0.16 + fi * 0.08
            + 0.06 * sin(x * (1.1 + fi * 0.4) + t * (2.0 + fi) + fi * 2.1)
            + 0.07 * (fbm(vec2<f32>(x * 0.9 + fi * 3.0, t * 0.8)) - 0.5);
        let d = uv.y - edge;
        let hang = select(exp(-d * d / 0.0004), exp(-d / (0.07 + 0.04 * fi)), d > 0.0);
        let r = fbm(vec2<f32>(x * 16.0 + fi * 5.0 + sin(uv.y * 3.0 + t) * 0.6, t * 1.2));
        let rays = 0.15 + 1.1 * r * r;
        let shade = mix(glow, mid * 2.4 + glow * 0.3, fi / 2.0);
        col += shade * hang * rays * (0.75 - fi * 0.18);
    }

    // Horizon glow behind two mountain ridges (the far one hazy, the near one dark).
    col += glow * 0.25 * exp(-pow((uv.y - 0.86) / 0.16, 2.0));
    let far = 0.74 - 0.07 * fbm(vec2<f32>(x * 1.6 + 11.0, 0.5)) - 0.04 * sin(x * 0.7 + 1.0);
    let near = 0.83 - 0.09 * fbm(vec2<f32>(x * 2.4 + 3.0, 1.7)) - 0.03 * sin(x * 1.3);
    let px = 1.5 / globals.resolution.y;
    col = mix(col, mix(deep, mid, 0.5) * 0.45 + glow * 0.04, smoothstep(far - px, far + px, uv.y));
    let rim = exp(-max(uv.y - near, 0.0) * 260.0) * 0.35;
    col = mix(col, deep * 0.16 + glow * rim, smoothstep(near - px, near + px, uv.y));
    let v = uv - vec2<f32>(0.5, 0.45);
    col *= 1.0 - 0.7 * dot(v, v);
    // Soft filmic curve and grain.
    col = col / (1.0 + col * 0.35);
    col += (hash(in.pixel + fract(globals.time) * 61.0) - 0.5) * 0.008;
    return vec4<f32>(max(col, vec3<f32>(0.0)), 1.0) * in.color.a;
}
