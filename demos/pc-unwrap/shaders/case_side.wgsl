// The case's right side panel, seen at an angle: dark metal with a vent grille.
// params.p[0] = (width, height, _, _) in face pixels; uv.x runs front to back.

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn cov(d: f32) -> f32 { return clamp(0.5 - d, 0.0, 1.0); }

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let p = in.uv * size;
    var col = mix(srgb(vec3<f32>(0.085, 0.09, 0.11)), srgb(vec3<f32>(0.04, 0.042, 0.052)), in.uv.x);
    // Vent slots near the bottom.
    let slot = step(0.5, fract(p.x / 10.0)) * cov(abs(p.y - size.y * 0.88) - 22.0);
    col = mix(col, srgb(vec3<f32>(0.012, 0.013, 0.018)), slot * 0.9);
    col = col + srgb(vec3<f32>(0.5, 0.55, 0.7)) * smoothstep(0.03, 0.0, in.uv.x) * 0.12;
    return vec4<f32>(col, 1.0) * in.color;
}
