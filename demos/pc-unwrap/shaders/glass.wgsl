// A smoked glass panel over the rendered case: a dark tint, two soft diagonal reflections of the
// room's lights, and a bright line along the edges where the glass is cut.
// params.p[0] = (tint rgb (linear), tint alpha), params.p[1] = (reflection offset 0..1, panel
// aspect (width / height), reflection strength, _).

fn fragment(in: VertexOut) -> vec4<f32> {
    let uv = in.uv;
    let aspect = params.p[1].y;
    var col = params.p[0].rgb * params.p[0].a;
    var a = params.p[0].a;
    // Reflections: broad bands running from top-left to bottom-right.
    let d = (uv.x * aspect - uv.y * 0.55) / max(aspect, 0.2) - params.p[1].x;
    let band = exp(-pow(d / 0.09, 2.0)) * 0.05 + exp(-pow((d - 0.16) / 0.025, 2.0)) * 0.03;
    // A faint gradient: brighter towards the top, where it faces the ceiling lights.
    let sky = (1.0 - uv.y) * 0.025;
    let refl = (band + sky) * params.p[1].z;
    col += vec3<f32>(0.85, 0.9, 1.0) * refl;
    a = min(1.0, a + refl);
    // The cut edge.
    let e = min(min(uv.x, 1.0 - uv.x) * aspect, min(uv.y, 1.0 - uv.y));
    let edge = exp(-e * e / 0.00002) * 0.35;
    col += vec3<f32>(0.75, 0.8, 0.9) * edge;
    a = min(1.0, a + edge * 0.5);
    return vec4<f32>(col, a) * in.color;
}
