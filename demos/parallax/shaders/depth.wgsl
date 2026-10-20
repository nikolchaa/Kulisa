// Depth-map 2.5D: one picture (`tex`) and its depth map (`mask_tex`, white = near) shifted per pixel
// by how near it is, so a still image turns like a small 3D scene when the view moves. The shader
// marches from the nearest depth to the farthest until the depth map says the surface is there
// (steep parallax mapping), then refines between the last two steps.
// Params: p[0].xy view offset in uv units at depth 0 (pointer or tilt), p[0].z focus depth (does
// not move), p[0].w zoom (above 1 hides the edges that get pulled in).

fn depth_at(uv: vec2<f32>) -> f32 {
    return textureSampleLevel(mask_tex, mask_samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).r;
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let view = params.p[0].xy;
    let focus = params.p[0].z;
    let zoom = max(params.p[0].w, 1.0);
    let uv0 = (in.uv - 0.5) / zoom + 0.5;
    let steps = 40;
    var d = 1.0;
    var prev_d = 1.0;
    var prev_gap = 0.0;
    var uv = uv0;
    var hit = false;
    for (var i = 0; i <= steps; i++) {
        d = 1.0 - f32(i) / f32(steps);
        uv = uv0 - view * (d - focus);
        let gap = depth_at(uv) - d;
        if (gap >= 0.0) {
            // Between the previous step (in front of the surface) and this one: interpolate.
            if (i > 0) {
                let t = prev_gap / (prev_gap - gap);
                let dd = mix(prev_d, d, t);
                uv = uv0 - view * (dd - focus);
            }
            hit = true;
            break;
        }
        prev_d = d;
        prev_gap = gap;
    }
    if (!hit) {
        uv = uv0 - view * (0.0 - focus);
    }
    let c = textureSampleLevel(tex, samp, clamp(uv, vec2<f32>(0.001), vec2<f32>(0.999)), 0.0);
    return c * in.color;
}
