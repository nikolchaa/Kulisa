// A mountain ridge drawn across the sprite: sharp ridged-noise peaks, a lit rim, snow on the high
// parts, and haze towards its foot. Params:
//   p[0] = (baseline 0..1 of the sprite height, peak height 0..1, frequency, seed)
//   p[1] = (top color rgb (linear), snow amount 0..1)
//   p[2] = (foot color rgb (linear), rim light 0..1)
//   p[3] = (world x of the sprite's left edge, sprite width, rim color warmth, unused)

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise1(x: f32, seed: f32) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(hash(vec2<f32>(i, seed)), hash(vec2<f32>(i + 1.0, seed)), u);
}

// Ridged noise: sharp peaks where the noise crosses the middle.
fn ridged(x0: f32, seed: f32) -> f32 {
    var x = x0;
    var s = 0.0;
    var a = 0.55;
    for (var i = 0; i < 5; i++) {
        let n = 1.0 - abs(noise1(x, seed + f32(i) * 7.0) * 2.0 - 1.0);
        s += a * n * n;
        x *= 2.1;
        a *= 0.48;
    }
    return s;
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let base = params.p[0].x;
    let peak = params.p[0].y;
    let freq = params.p[0].z;
    let seed = params.p[0].w;
    let top = params.p[1].xyz;
    let snow = params.p[1].w;
    let foot = params.p[2].xyz;
    let rim = params.p[2].w;
    let wx = params.p[3].x + in.uv.x * params.p[3].y;

    // Sharpened, never clipped: peaks stay pointed.
    let r = pow(ridged(wx * freq, seed), 1.3) * 1.6;
    let h = base - peak * r;
    let d = in.uv.y - h;
    let aa = max(fwidth(in.uv.y) * 1.5, 0.0005);
    let cover = smoothstep(-aa, aa, d);
    if (cover <= 0.0) {
        return vec4<f32>(0.0);
    }
    let depth = clamp(d / max(1.0 - h, 0.001), 0.0, 1.0);
    var col = mix(top, foot, smoothstep(0.0, 0.7, depth));
    // Snow where the ridge is high, on the upper part of the slope.
    let high = smoothstep(0.5, 0.9, r);
    col = mix(col, vec3<f32>(0.85, 0.88, 0.95) * (top * 2.0 + 0.2), snow * high * (1.0 - smoothstep(0.0, 0.08, d)));
    // Rim light along the crest.
    col += mix(vec3<f32>(0.9, 0.95, 1.0), vec3<f32>(1.0, 0.7, 0.45), params.p[3].z) * rim * exp(-d / max(aa * 4.0, 0.004)) * 0.6;
    return vec4<f32>(col * cover, cover) * in.color;
}
