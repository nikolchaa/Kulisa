// The valley sky through a day: a gradient that follows the sun's height, the sun and the moon on
// an arc, stars at night, drifting clouds lit by the sun. Params: p[0].x time of day (0 midnight,
// 0.25 sunrise, 0.5 noon, 0.75 sunset), p[0].y aspect (width / height), p[0].z cloud drift,
// p[0].w cloud cover (0..1).

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
    for (var i = 0; i < 5; i++) {
        s += a * noise(p);
        p = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * p;
        a *= 0.5;
    }
    return s;
}

fn srgb(c: vec3<f32>) -> vec3<f32> {
    return pow(c, vec3<f32>(2.2));
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let phase = params.p[0].x;
    let aspect = max(params.p[0].y, 0.1);
    let drift = params.p[0].z;
    let cover = params.p[0].w;
    let uv = in.uv;
    let p = vec2<f32>(uv.x * aspect, uv.y);

    // The sun's arc: up at noon, under the horizon at night. The moon is opposite.
    let a = (phase - 0.25) * 6.2831853;
    let sun = vec2<f32>(aspect * (0.5 - 0.42 * cos(a)), 0.78 - 0.68 * sin(a));
    let moon = vec2<f32>(aspect * (0.5 + 0.42 * cos(a)), 0.78 + 0.68 * sin(a));
    let elev = sin(a); // 1 at noon, -1 at midnight

    // Palettes: night, twilight, day; zenith and horizon.
    let night_z = srgb(vec3<f32>(0.02, 0.03, 0.09));
    let night_h = srgb(vec3<f32>(0.07, 0.09, 0.2));
    let twi_z = srgb(vec3<f32>(0.18, 0.2, 0.45));
    let twi_h = srgb(vec3<f32>(1.0, 0.55, 0.35));
    let day_z = srgb(vec3<f32>(0.22, 0.47, 0.86));
    let day_h = srgb(vec3<f32>(0.7, 0.84, 0.97));
    let tw = exp(-pow(elev / 0.28, 2.0));
    let dayk = smoothstep(-0.1, 0.45, elev);
    var zen = mix(night_z, day_z, dayk);
    var hor = mix(night_h, day_h, dayk);
    zen = mix(zen, twi_z, tw * 0.6);
    hor = mix(hor, twi_h, tw * 0.85);
    let h = smoothstep(0.0, 0.85, uv.y);
    var col = mix(zen, hor, h * h);

    // Stars fade in at night.
    let nightk = 1.0 - smoothstep(-0.25, 0.05, elev);
    let cell = floor(in.pixel / 3.0);
    let sp = fract(in.pixel / 3.0) - vec2<f32>(hash(cell + 2.0), hash(cell + 5.0));
    let star = step(0.992, hash(cell)) * exp(-dot(sp, sp) * 18.0)
        * (0.6 + 0.4 * sin(globals.time * 2.3 + hash(cell) * 60.0));
    col += vec3<f32>(star) * nightk * (1.0 - h);

    // Sun disc and glow; the glow warms near the horizon.
    let ds = length(p - sun);
    let warm = mix(srgb(vec3<f32>(1.0, 0.95, 0.85)), srgb(vec3<f32>(1.0, 0.55, 0.25)), tw);
    col += warm * (exp(-ds * ds * 40.0) * 0.6 + exp(-ds * 7.0) * 0.25) * smoothstep(-0.2, 0.05, elev);
    col = mix(col, warm * 4.0, smoothstep(0.045, 0.035, ds) * smoothstep(-0.15, 0.0, elev));
    // Moon with a soft halo.
    let dm = length(p - moon);
    col += srgb(vec3<f32>(0.75, 0.82, 1.0)) * exp(-dm * 9.0) * 0.12 * nightk;
    col = mix(col, srgb(vec3<f32>(0.92, 0.94, 1.0)), smoothstep(0.03, 0.026, dm) * nightk);

    // Clouds: two layers drifting at different speeds, lit from the sun's side.
    let c1 = fbm(vec2<f32>(p.x * 2.2 + drift, p.y * 5.0));
    let c2 = fbm(vec2<f32>(p.x * 4.5 + drift * 1.7 + 9.0, p.y * 9.0 + 3.0));
    let cloud = smoothstep(0.62 - cover * 0.25, 0.85, c1 * 0.65 + c2 * 0.35) * (1.0 - smoothstep(0.55, 0.85, uv.y));
    // Lit warm at sunrise and sunset, white by day, a dim blue-grey at night (moonlit).
    let day_lit = mix(warm, srgb(vec3<f32>(1.0, 1.0, 1.0)), dayk);
    let night_lit = srgb(vec3<f32>(0.16, 0.18, 0.28));
    let lit = mix(night_lit, day_lit, smoothstep(-0.25, 0.15, elev));
    let toward_sun = clamp(1.0 - length(p - sun) * 0.8, 0.0, 1.0) * smoothstep(-0.2, 0.1, elev);
    col = mix(col, lit * (0.6 + 0.6 * toward_sun), cloud * mix(0.45, 0.85, smoothstep(-0.3, 0.1, elev)));
    return vec4<f32>(col, 1.0) * in.color.a;
}
