// Example sprite shader: horizontal sine wave. params.p[0] = (amplitude, frequency, speed, _).
fn fragment(in: VertexOut) -> vec4<f32> {
    let p = params.p[0];
    let uv = in.uv + vec2<f32>(sin(in.uv.y * p.y + globals.time * p.z) * p.x, 0.0);
    return textureSample(tex, samp, uv) * in.color;
}
