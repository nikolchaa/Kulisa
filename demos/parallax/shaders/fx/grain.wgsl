// Film grain and a vignette. p[0] = (grain amount, grain size in pixels, vignette, unused).
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    let cell = floor(in.pixel / max(params.p[0].y, 1.0));
    let n = hash(cell + fract(globals.time * 7.31) * 113.0) - 0.5;
    let l = dot(c.rgb, vec3<f32>(0.3, 0.6, 0.1));
    // Colors are linear here: scale the grain with brightness so shadows do not sparkle, and
    // keep it strongest in the midtones, as on film.
    let amt = params.p[0].x * sqrt(max(l, 0.0)) * (0.5 + 0.5 * (1.0 - abs(l * 2.0 - 1.0)));
    let d = in.uv - 0.5;
    let vig = 1.0 - params.p[0].z * dot(d, d) * 2.2;
    return vec4<f32>(max(c.rgb * vig + n * amt * c.a, vec3<f32>(0.0)), c.a);
}
