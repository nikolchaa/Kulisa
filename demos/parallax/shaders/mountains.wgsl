// A mountain range from a data texture (art/parallax/mountains.py): red is the rock's lightness under
// a sun on the left, green under a sun on the right (both with cast shadows), blue is snow, alpha
// the outline. The palette for the time of day comes in as params:
//   p[0] = (lit rock color rgb (linear), light side 0 = from the left .. 1 = from the right)
//   p[1] = (shadowed rock color rgb, snow amount)
//   p[2] = (haze color rgb, haze over the whole range)
//   p[3] = (lit snow color rgb, extra haze towards the foot)

fn fragment(in: VertexOut) -> vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    let a = t.a;
    // The image arrives linear and premultiplied; the data was written as plain 0..1 numbers.
    let d = pow(t.rgb / max(a, 0.0001), vec3<f32>(1.0 / 2.2));
    let l = mix(d.r, d.g, params.p[0].w);
    let k = clamp(l / 0.62, 0.0, 1.25);
    let lit = params.p[0].xyz;
    let shade = params.p[1].xyz;
    var col = shade * (1.0 - min(k, 1.0)) + lit * k;
    let snow_lit = params.p[3].xyz;
    let snow_shade = shade * 1.5 + vec3<f32>(0.02, 0.025, 0.04);
    col = mix(col, mix(snow_shade, snow_lit, min(k, 1.0)), clamp(d.b * params.p[1].w, 0.0, 1.0));
    let haze = clamp(params.p[2].w + params.p[3].w * smoothstep(0.45, 1.0, in.uv.y), 0.0, 1.0);
    col = mix(col, params.p[2].xyz, haze);
    return vec4<f32>(col * a, a) * in.color;
}
