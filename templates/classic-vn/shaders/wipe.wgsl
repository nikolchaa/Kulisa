// Wipes and image dissolves. params.p[0]: x = progress (0..1), y = mode, z = softness.
// Modes: 0 sweeps right, 1 left, 2 down, 3 up, 4 follows the mask image (darker first).
fn fragment(in: VertexOut) -> vec4<f32> {
    let p = params.p[0].x;
    let soft = max(params.p[0].z, 0.001);
    let mode = i32(params.p[0].y + 0.5);
    var t = in.uv.x;
    if mode == 1 {
        t = 1.0 - in.uv.x;
    } else if mode == 2 {
        t = in.uv.y;
    } else if mode == 3 {
        t = 1.0 - in.uv.y;
    } else if mode == 4 {
        t = textureSample(mask_tex, mask_samp, in.uv).r;
    }
    let edge = p * (1.0 + soft);
    let keep = smoothstep(edge - soft, edge, t);
    return textureSample(tex, samp, in.uv) * keep;
}
