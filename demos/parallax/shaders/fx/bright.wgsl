// Bloom, step 1: keeps what is brighter than a threshold (soft knee). p[0].x threshold, p[0].y knee.
fn fragment(in: VertexOut) -> vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    let l = max(c.r, max(c.g, c.b));
    let t = params.p[0].x;
    let k = max(params.p[0].y, 0.0001);
    let soft = clamp(l - t + k, 0.0, 2.0 * k);
    let w = max(soft * soft / (4.0 * k), l - t) / max(l, 0.0001);
    return vec4<f32>(c.rgb * max(w, 0.0), 1.0);
}
