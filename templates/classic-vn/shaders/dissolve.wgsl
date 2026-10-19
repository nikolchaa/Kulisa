// Crossfade from the old screen to the new one. params.p[0].x = progress (0..1).
fn fragment(in: VertexOut) -> vec4<f32> {
    return textureSample(tex, samp, in.uv) * (1.0 - params.p[0].x);
}
