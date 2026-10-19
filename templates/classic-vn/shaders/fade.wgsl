// Fade through a color: the old screen fades to the color, then the new one fades in.
// params.p[0]: x = progress (0..1), yzw = the color (linear RGB).
fn fragment(in: VertexOut) -> vec4<f32> {
    let p = params.p[0].x;
    let color = vec4<f32>(params.p[0].yzw, 1.0);
    if p < 0.5 {
        return mix(textureSample(tex, samp, in.uv), color, p * 2.0);
    }
    return color * (1.0 - (p - 0.5) * 2.0);
}
