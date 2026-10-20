// The squeegee: a blue plastic card with a rubber blade along its top edge (the edge that leads
// when it moves), a soft drop shadow, and a grip. params.p[0] = (width, height, _, _).

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn sd_box(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_round(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 { return sd_box(p, b - vec2<f32>(r)) - r; }

fn cov(d: f32) -> f32 { return clamp(0.5 - d, 0.0, 1.0); }

fn fragment(in: VertexOut) -> vec4<f32> {
    let size = params.p[0].xy;
    let p = in.uv * size - size * 0.5;
    // Shadow, offset down and blurred.
    let sh = sd_round(p - vec2<f32>(5.0, 9.0), size * 0.5 - vec2<f32>(14.0, 12.0), 10.0);
    let shadow = (1.0 - smoothstep(-6.0, 12.0, sh)) * 0.35;
    // Body.
    let half = size * 0.5 - vec2<f32>(12.0, 8.0);
    let body = sd_round(p, half, 9.0);
    let top = p.y + half.y; // distance from the blade edge, 0..
    var col = mix(srgb(vec3<f32>(0.3, 0.52, 0.95)), srgb(vec3<f32>(0.18, 0.34, 0.78)), smoothstep(0.0, half.y * 2.0, top));
    // Rubber blade strip along the leading edge.
    let rubber = cov(top - 11.0) * cov(body);
    col = mix(col, srgb(vec3<f32>(0.1, 0.11, 0.14)), rubber);
    // Highlights: top bevel and a soft diagonal sheen.
    col = col + srgb(vec3<f32>(0.9, 0.95, 1.0)) * smoothstep(1.5, 0.0, abs(top - 12.5)) * 0.35;
    col = col + srgb(vec3<f32>(0.8, 0.9, 1.0)) * smoothstep(0.12, 0.0, abs(in.uv.x * 0.7 - in.uv.y * 0.3 - 0.2)) * 0.18;
    // Grip.
    let grip = sd_round(p - vec2<f32>(0.0, 8.0), vec2<f32>(46.0, 13.0), 7.0);
    col = mix(col, srgb(vec3<f32>(0.07, 0.15, 0.4)), cov(grip) * 0.8);
    col = col + srgb(vec3<f32>(0.6, 0.75, 1.0)) * cov(abs(grip) - 0.7) * 0.25;
    let a = cov(body);
    let rgb = col * a;
    let out = vec4<f32>(rgb * 1.0 + vec3<f32>(0.0), a);
    // Composite the shadow underneath.
    return vec4<f32>(out.rgb, out.a + shadow * (1.0 - out.a)) * in.color;
}
