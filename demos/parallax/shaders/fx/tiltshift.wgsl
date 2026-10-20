// Tilt-shift: the blurred image (`tex`) over the sharp source (`mask_tex`) outside a band, which
// makes a scene look like a miniature. p[0].x strength, p[0].y focus height (0..1), p[0].z band.
fn fragment(in: VertexOut) -> vec4<f32> {
    let sharp = textureSample(mask_tex, mask_samp, in.uv);
    let soft = textureSample(tex, samp, in.uv);
    let k = smoothstep(params.p[0].z * 0.5, params.p[0].z * 0.5 + 0.25, abs(in.uv.y - params.p[0].y));
    return mix(sharp, soft, clamp(k * params.p[0].x, 0.0, 1.0));
}
