// Dual-filter blur, upsampling step: eight taps in a tent around each output pixel.
fn fragment(in: VertexOut) -> vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(tex));
    let o = px * (0.5 + params.p[0].x);
    var c = textureSample(tex, samp, in.uv + vec2<f32>(-o.x * 2.0, 0.0));
    c += textureSample(tex, samp, in.uv + vec2<f32>(o.x * 2.0, 0.0));
    c += textureSample(tex, samp, in.uv + vec2<f32>(0.0, -o.y * 2.0));
    c += textureSample(tex, samp, in.uv + vec2<f32>(0.0, o.y * 2.0));
    c += textureSample(tex, samp, in.uv + vec2<f32>(-o.x, -o.y)) * 2.0;
    c += textureSample(tex, samp, in.uv + vec2<f32>(o.x, -o.y)) * 2.0;
    c += textureSample(tex, samp, in.uv + vec2<f32>(-o.x, o.y)) * 2.0;
    c += textureSample(tex, samp, in.uv + vec2<f32>(o.x, o.y)) * 2.0;
    return c / 12.0;
}
