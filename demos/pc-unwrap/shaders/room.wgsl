// The backdrop: a wall fading into a desk, lit from the upper left, with a soft contact shadow
// under the PC. params.p[0] = (desk top y, shadow center x, shadow half width, shadow half
// height) in the sprite's pixels; params.p[1] = (width, height, _, _).

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[1].xy;
    let p = in.uv * size;
    let desk = params.p[0].x;
    // Wall: a cool dark gradient with a pool of light.
    let lamp = length((p - vec2<f32>(size.x * 0.25, size.y * 0.15)) / size.x);
    var col = mix(srgb(vec3<f32>(0.075, 0.08, 0.105)), srgb(vec3<f32>(0.16, 0.17, 0.21)), smoothstep(0.9, 0.0, lamp));
    // Desk: a warm wood-like plane with long grain and a bright front edge.
    let dd = (p.y - desk) / (size.y - desk);
    if (p.y > desk) {
        let grain = 0.5 + 0.5 * sin(p.x * 0.05 + sin(p.y * 0.9 + floor(p.x / 31.0) * 7.0) * 1.2);
        let wood = mix(srgb(vec3<f32>(0.2, 0.15, 0.12)), srgb(vec3<f32>(0.27, 0.2, 0.15)), grain * 0.4 + hash21(floor(p * vec2<f32>(0.2, 1.0))) * 0.15);
        col = mix(wood, wood * 0.55, smoothstep(0.0, 1.0, dd));
        col = col + srgb(vec3<f32>(0.5, 0.45, 0.4)) * smoothstep(0.02, 0.0, abs(dd)) * 0.18;
    }
    // Contact shadow of the case: soft and wider on the desk.
    let sc = vec2<f32>(params.p[0].y, desk + 8.0);
    let sd = (p - sc) / params.p[0].zw;
    let sh = exp(-dot(sd, sd) * 1.6);
    col = col * (1.0 - 0.72 * sh);
    return vec4<f32>(col, 1.0) * in.color;
}
