// Corrugated cardboard for the box: a warm brown with fine flutes, paper fibres and darker edges.
// params.p[0] = (width, height, shade, flutes along v (1) or u (0)) in pixels; p[1].x = interior
// darkness (0..1: the inside of the box is shadowed).

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let p = in.uv * size;
    let shade = params.p[0].z;
    var col = mix(srgb(vec3<f32>(0.78, 0.62, 0.43)), srgb(vec3<f32>(0.66, 0.5, 0.33)), in.uv.y * 0.6);
    // Flutes: very faint stripes across the board.
    let fl = select(p.x, p.y, params.p[0].w > 0.5);
    col = col * (0.96 + 0.04 * sin(fl * 1.7));
    // Paper fibres and speckle.
    let n = hash21(floor(p * vec2<f32>(0.9, 0.6))) * 0.5 + hash21(floor(p * 2.7)) * 0.5;
    col = col * (0.95 + 0.1 * n);
    // Slow stains.
    col = col * (0.97 + 0.03 * sin(p.x * 0.011 + sin(p.y * 0.017) * 2.0));
    // Edges darken and fold lines catch the light.
    let e = min(min(in.uv.x, 1.0 - in.uv.x) * size.x, min(in.uv.y, 1.0 - in.uv.y) * size.y);
    col = col * (0.82 + 0.18 * smoothstep(0.0, 9.0, e));
    col = col + srgb(vec3<f32>(0.95, 0.85, 0.7)) * smoothstep(1.5, 0.0, abs(e - 1.5)) * 0.08;
    col = col * shade * (1.0 - params.p[1].x * 0.6);
    return vec4<f32>(col, 1.0) * in.color;
}
