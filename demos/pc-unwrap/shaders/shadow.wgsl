// Soft cast shadow of a lifted film on the surface below it. The mesh carries the strength in
// in.data.x; the edges of the film feather out so the shadow has no hard outline.
// params.p[0] = (width, height, _, _) of the film in face pixels.
fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let e = min(min(in.uv.x, 1.0 - in.uv.x) * size.x, min(in.uv.y, 1.0 - in.uv.y) * size.y);
    let a = in.data.x * smoothstep(0.0, 12.0, e);
    return vec4<f32>(0.0, 0.0, 0.0, a) * in.color;
}
