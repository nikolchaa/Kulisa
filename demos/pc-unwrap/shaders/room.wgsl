// The backdrop: a dark wall with out-of-focus lights fading into a glossy black desk that picks up
// the neon from under the case, plus a soft contact shadow. params.p[0] = (desk top y, shadow
// center x, shadow half width, shadow half height) in the sprite's pixels; params.p[1] = (width,
// height, _, _); params.p[2].x = the shadow's and the glow's center y.

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[1].xy;
    let p = in.uv * size;
    let desk = params.p[0].x;
    let cx = params.p[0].y;
    // Wall: deep blue-violet, brighter behind the case.
    let halo = length((p - vec2<f32>(cx, desk - 260.0)) / vec2<f32>(900.0, 520.0));
    var col = mix(srgb(vec3<f32>(0.035, 0.03, 0.07)), srgb(vec3<f32>(0.09, 0.07, 0.19)), smoothstep(1.0, 0.0, halo));
    // Bokeh: a few big soft discs of colored light far behind.
    for (var i = 0; i < 7; i = i + 1) {
        let fi = f32(i);
        let c = vec2<f32>(size.x * (0.1 + 0.13 * fi + 0.03 * sin(fi * 5.0)), desk - 120.0 - 220.0 * hash21(vec2<f32>(fi, 3.0)));
        let r = 40.0 + 55.0 * hash21(vec2<f32>(fi, 7.0));
        let d = length(p - c) / r;
        let hue = mix(vec3<f32>(0.6, 0.25, 1.0), vec3<f32>(0.15, 0.6, 1.0), hash21(vec2<f32>(fi, 1.0)));
        col = col + srgb(hue) * smoothstep(1.0, 0.8, d) * 0.035 + srgb(hue) * smoothstep(1.3, 0.0, d) * 0.012;
    }
    // Desk: glossy black, with the wall's glow reflected and a bright front edge.
    if (p.y > desk) {
        let dd = (p.y - desk) / (size.y - desk);
        var d_col = srgb(vec3<f32>(0.018, 0.018, 0.026));
        d_col = d_col + srgb(vec3<f32>(0.08, 0.07, 0.16)) * smoothstep(1.0, 0.0, halo * 1.3) * (1.0 - dd) * 0.8;
        d_col = d_col + srgb(vec3<f32>(0.04)) * (0.5 + 0.5 * sin(p.x * 0.03 + sin(p.y * 0.4) * 0.8)) * 0.05;
        col = d_col;
        col = col + srgb(vec3<f32>(0.5, 0.5, 0.7)) * smoothstep(0.012, 0.0, abs(dd)) * 0.08;
        // Neon from under the case.
        let g = (p.x - cx) / params.p[0].z;
        let gy = (p.y - params.p[2].x) / (params.p[0].w * 2.2);
        let hue = mix(vec3<f32>(0.7, 0.22, 1.0), vec3<f32>(0.15, 0.55, 1.0), smoothstep(-0.8, 0.8, g));
        col = col + srgb(hue) * exp(-gy * gy * 1.2) * exp(-g * g * 1.3) * 0.2;
    }
    // Contact shadow of the case.
    let sc = vec2<f32>(cx, params.p[2].x);
    let sd = (p - sc) / params.p[0].zw;
    col = col * (1.0 - 0.65 * exp(-dot(sd, sd) * 1.8));
    return vec4<f32>(col, 1.0) * in.color;
}
