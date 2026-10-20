// Glossy plastic for the protective films, the I/O sticker and the bag.
//
// The mesh supplies the shape: `in.data.xyz` is the surface normal of the film's top side in the
// face's own axes (a, b, up), scaled by (1 - occlusion), and `in.data.w` is a trapped-air amount
// (a bubble). Everything else is shading: fresnel, a blurry softbox reflection plus a sharp
// highlight, an underside tint when the film is turned over (found from the winding of uv against
// the screen), wrinkle normals, edge glints and curl shadow. The texture is the printed layer.
//
// params.p[0] = (light a, b, up, gloss)       direction to the light in face axes
// params.p[1] = (view a, b, up, base alpha)   direction to the viewer; alpha of clear film
// params.p[2] = (tint r, g, b, wrinkle)       body tint (linear) and wrinkle strength
// params.p[3] = (width, height, style, fade)  film size in face px; style = kind + 4 * mirrored
//               + 8 * printed (the texture is a print layer; otherwise it is ignored)
//               kinds: 0 clear film, 1 bag (milky), 2 sticker (opaque paper)

fn srgb(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn luma(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)); }

// A soft studio: a big key softbox with window bars, a dim fill and a gradient room.
fn environment(r: vec3<f32>, l: vec3<f32>) -> vec3<f32> {
    let key = pow(max(dot(r, l), 0.0), 16.0);
    let bars = 0.55 + 0.45 * smoothstep(0.2, 0.55, abs(fract((r.x - l.x) * 7.0 + (r.y - l.y) * 3.0) - 0.5));
    let fill_dir = normalize(vec3<f32>(-l.x * 0.6 + 0.15, -l.y * 0.8, 0.7));
    let fill = pow(max(dot(r, fill_dir), 0.0), 5.0) * 0.22;
    let room = 0.07 + 0.2 * clamp(r.z, 0.0, 1.0) + 0.05 * (r.y * 0.5 + 0.5);
    return vec3<f32>(1.0, 0.97, 0.92) * key * bars * 1.7 + vec3<f32>(0.75, 0.85, 1.0) * fill + vec3<f32>(room);
}

// Sharp creases: a few ridged sine layers at different angles, each masked by a slow field.
// Returns (height, d height / d x, d height / d y) in film pixels.
fn creases(p: vec2<f32>, seed: f32) -> vec3<f32> {
    var h = 0.0;
    var g = vec2<f32>(0.0);
    var amp = 1.0;
    for (var i = 0; i < 4; i = i + 1) {
        let fi = f32(i);
        let a = seed + 1.9 * fi + 0.5;
        let d = vec2<f32>(cos(a), sin(a));
        let f = 0.05 + 0.032 * fi;
        let ph = dot(p, d) * f + 2.1 * fi + 1.1 * sin(dot(p, vec2<f32>(-d.y, d.x)) * 0.013 + fi);
        let s = sin(ph);
        let m = smoothstep(0.15, 0.85, 0.5 + 0.5 * sin(dot(p, vec2<f32>(d.y, -d.x)) * 0.011 + 4.0 * fi + seed));
        h = h + (1.0 - abs(s)) * m * amp;
        let dh = -sign(s) * cos(ph) * f * m * amp;
        g = g + d * dh;
        amp = amp * 0.62;
    }
    return vec3<f32>(h, g);
}

fn fragment(in: VertexOut) -> vec4<f32> {
    let light = normalize(params.p[0].xyz);
    let gloss = params.p[0].w;
    let base_alpha = params.p[1].w;
    let tint = params.p[2].xyz;
    let wrinkle = params.p[2].w;
    let size = params.p[3].xy;
    var style = params.p[3].z;
    let fade = params.p[3].w;
    let printed = style >= 7.5;
    style = style - select(0.0, 8.0, printed);
    let mirrored = style >= 3.5;
    let kind = style - select(0.0, 4.0, mirrored);
    let is_bag = kind > 0.5 && kind < 1.5;
    let is_sticker = kind > 1.5;

    // Which side are we looking at? The sign of uv's orientation on screen says whether the
    // film is turned over (compared with how the face itself is oriented).
    let dpx = dpdx(in.pixel);
    let dpy = dpdy(in.pixel);
    let dux = dpdx(in.uv);
    let duy = dpdy(in.uv);
    let det_screen = dpx.x * dpy.y - dpx.y * dpy.x;
    let det_uv = dux.x * duy.y - dux.y * duy.x;
    var turned = det_screen * det_uv < 0.0;
    if (mirrored) { turned = !turned; }

    // Surface normal from the mesh (its length carries the curl shadow).
    var n = in.data.xyz;
    let len = max(length(n), 0.0001);
    let occlusion = clamp(1.0 - len, 0.0, 1.0);
    n = n / len;
    if (len < 0.01) { n = vec3<f32>(0.0, 0.0, 1.0); }

    // Wrinkles tilt the normal a little; they follow the material (uv), not the screen.
    let px = in.uv * size;
    let cr = creases(px, 1.3 + select(0.0, 2.1, is_bag));
    let wr = wrinkle * select(1.0, 1.7, is_bag);
    n = normalize(n + vec3<f32>(-cr.y, -cr.z, 0.0) * wr * 3.0);
    // The air under a bubble pushes the film's top up into a lens: nothing extra to do for the
    // normal (the mesh has it), but the lens whitens.
    let air = clamp(in.data.w, 0.0, 1.0);

    var v = normalize(params.p[1].xyz + vec3<f32>((in.uv.x - 0.5) * 0.34, (in.uv.y - 0.5) * 0.3, 0.0));
    var s = n;
    if (turned) { s = -n; }
    let ndv = max(dot(s, v), 0.0);
    let fres = 0.045 + 0.955 * pow(1.0 - ndv, 5.0);
    let r = reflect(-v, s);
    var env = environment(r, light);
    let h = normalize(light + v);
    let spec_pow = select(120.0, 40.0, is_bag);
    let spec = min(pow(max(dot(s, h), 0.0), spec_pow) * gloss * select(2.2, 1.0, is_bag), 1.2);
    let diffuse = clamp(dot(s, light) * 0.5 + 0.5, 0.0, 1.0);

    // Body: clear film is nearly see-through; its underside (adhesive) and a bag are milky;
    // a sticker is opaque paper.
    var alpha = base_alpha;
    var body = tint;
    if (is_bag) { alpha = base_alpha + 0.22 * (1.0 - ndv) + 0.1 * cr.x * wrinkle; }
    if (turned) {
        alpha = min(1.0, alpha + 0.32);
        body = mix(body, srgb(vec3<f32>(0.78, 0.85, 0.95)), 0.7);
        env = env * 0.7;
    }
    if (is_sticker) {
        alpha = 1.0;
        body = select(srgb(vec3<f32>(0.95, 0.95, 0.97)), srgb(vec3<f32>(0.82, 0.8, 0.74)), turned);
    }
    alpha = alpha + air * 0.06;
    // A bubble is a lens: a crescent highlight on the side facing the light, a ring of shade
    // opposite it, and a faint brightening inside.
    let lens = smoothstep(0.02, 0.25, air);
    let lens_hi = pow(max(dot(s, h), 0.0), 26.0) * lens;
    let shade = (0.45 + 0.55 * diffuse) * (1.0 - occlusion * 1.3);
    var color = body * alpha * shade;
    var a = alpha;
    // Reflections add light (premultiplied: color and a bit of coverage).
    let refl = (env * fres * (1.0 - occlusion) + vec3<f32>(spec + lens_hi * 0.9) * (1.0 - occlusion * 0.7)) * (1.0 + air * 0.5);
    color = color + refl;
    a = min(1.0, a + luma(refl) * 0.85 + occlusion * 0.12);

    // Edge glints: a thin bright line along the border and hot corners.
    let e = min(min(in.uv.x, 1.0 - in.uv.x) * size.x, min(in.uv.y, 1.0 - in.uv.y) * size.y);
    let rim = exp(-e * e / 2.2) * (0.32 + 0.4 * pow(clamp(dot(s, h), 0.0, 1.0), 2.0));
    let cx = min(in.uv.x, 1.0 - in.uv.x) * size.x;
    let cy = min(in.uv.y, 1.0 - in.uv.y) * size.y;
    let corner = exp(-(cx * cx + cy * cy) / 90.0) * 0.5;
    // Thin strips (tape) are all edge: skip the glints there.
    let edge = (rim + corner) * select(1.0, 0.35, is_bag) * smoothstep(20.0, 60.0, min(size.x, size.y));
    color = color + vec3<f32>(0.9, 0.95, 1.0) * edge * 0.6;
    a = min(1.0, a + edge * 0.5);

    // The printed layer: on top when we see the printed side, ghosted through the film otherwise.
    var print = vec4<f32>(0.0);
    if (printed) { print = textureSample(tex, samp, in.uv); }
    if (turned && !is_sticker) {
        color = color * (1.0 - 0.35 * print.a) + print.rgb * 0.3;
    } else if (!turned) {
        color = color * (1.0 - print.a) + print.rgb * (0.55 + 0.45 * shade);
        a = a * (1.0 - print.a) + print.a;
    }

    let out = vec4<f32>(color, a) * fade;
    return out * in.color;
}
