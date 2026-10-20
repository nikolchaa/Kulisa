// Bloom, last step: the scene (`mask_tex`, the chain's source) plus the blurred bright parts
// (`tex`). p[0].x intensity, p[0].yzw tint of the glow.
fn fragment(in: VertexOut) -> vec4<f32> {
    let src = textureSample(mask_tex, mask_samp, in.uv);
    let glow = textureSample(tex, samp, in.uv).rgb * params.p[0].yzw * params.p[0].x;
    return vec4<f32>(src.rgb + glow * src.a + glow * (1.0 - src.a) * 0.5, max(src.a, 0.0));
}
