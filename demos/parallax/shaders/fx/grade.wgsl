// Color grading: lift, gamma and gain per channel, then saturation, contrast and temperature.
// p[0].rgb lift, p[1].rgb gamma, p[2].rgb gain, p[3] = (saturation, contrast, temperature, mix).
fn fragment(in: VertexOut) -> vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    let a = max(c.a, 0.0001);
    var rgb = c.rgb / a;
    // Work in display space (gamma) so the controls feel like an editor's.
    rgb = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
    let lift = params.p[0].rgb;
    let gamma = max(params.p[1].rgb, vec3<f32>(0.01));
    let gain = params.p[2].rgb;
    rgb = gain * (rgb + lift * (1.0 - rgb));
    rgb = pow(max(rgb, vec3<f32>(0.0)), 1.0 / gamma);
    let l = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    rgb = mix(vec3<f32>(l), rgb, params.p[3].x);
    rgb = (rgb - 0.5) * params.p[3].y + 0.5;
    let t = params.p[3].z;
    rgb *= vec3<f32>(1.0 + 0.1 * t, 1.0, 1.0 - 0.1 * t);
    rgb = pow(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(2.2));
    let graded = vec4<f32>(rgb * a, c.a);
    return mix(c, graded, params.p[3].w);
}
