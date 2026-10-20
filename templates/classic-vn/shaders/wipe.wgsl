// Wipes and dissolves. params.p[0]: x = progress (0..1), y = mode, z = softness.
// Modes: 0 sweeps right, 1 left, 2 down, 3 up, 4 follows the mask image (darker first; give the
// transition a `mask`), 5 a swirl from the middle, computed per pixel (smooth at any size).
fn fragment(in: VertexOut) -> vec4<f32> {
    let p = params.p[0].x;
    let soft = max(params.p[0].z, 0.001);
    let mode = i32(params.p[0].y + 0.5);
    // Sampled up front: texture reads must stay in uniform control flow.
    let old = textureSample(tex, samp, in.uv);
    let masked = textureSample(mask_tex, mask_samp, in.uv).r;
    var t = in.uv.x;
    if mode == 1 {
        t = 1.0 - in.uv.x;
    } else if mode == 2 {
        t = in.uv.y;
    } else if mode == 3 {
        t = 1.0 - in.uv.y;
    } else if mode == 4 {
        t = masked;
    } else if mode == 5 {
        // Round on screen whatever its shape: distances in pixels, scaled to the farthest corner.
        let res = globals.resolution;
        let d = (in.uv - 0.5) * res;
        let r = length(d) / (0.5 * length(res));
        let a = atan2(d.y, d.x) / 6.2831853;
        // Three arms winding outwards; the sine keeps the field smooth all the way round.
        let arms = 0.5 + 0.5 * sin(6.2831853 * (3.0 * a + 1.6 * r));
        t = clamp(r * 0.75 + arms * 0.25, 0.0, 1.0);
    }
    let edge = p * (1.0 + soft);
    let keep = smoothstep(edge - soft, edge, t);
    return old * keep;
}
