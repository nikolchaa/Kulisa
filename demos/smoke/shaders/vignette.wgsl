// Example screen effect: vignette plus film grain.
// params.p[0] = (inner radius, outer radius, grain amount, vignette strength).
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let p = params.p[0];
    let c = textureSample(tex, samp, in.uv);
    let d = distance(in.uv, vec2<f32>(0.5));
    let vignette = smoothstep(p.x, p.y, d) * p.w;
    let grain = (hash(floor(in.pixel) + fract(globals.time) * 100.0) - 0.5) * p.z;
    return vec4<f32>(max(c.rgb * (1.0 - vignette) + grain, vec3<f32>(0.0)), c.a);
}
