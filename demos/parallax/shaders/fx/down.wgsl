// Dual-filter blur, downsampling step: five taps around each output pixel of the smaller image.
// Run it at decreasing `scale`s, then `up.wgsl` back up, for a wide, cheap blur.
fn fragment(in: VertexOut) -> vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(tex));
    let o = px * (0.5 + params.p[0].x);
    var c = textureSample(tex, samp, in.uv) * 4.0;
    c += textureSample(tex, samp, in.uv + vec2<f32>(-o.x, -o.y));
    c += textureSample(tex, samp, in.uv + vec2<f32>(o.x, -o.y));
    c += textureSample(tex, samp, in.uv + vec2<f32>(-o.x, o.y));
    c += textureSample(tex, samp, in.uv + vec2<f32>(o.x, o.y));
    return c / 8.0;
}
