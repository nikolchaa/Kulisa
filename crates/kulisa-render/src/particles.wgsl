// ---- Kulisa particle prelude (appended after user code) ----
//
// Particle shaders define the same `fn fragment(in: VertexOut) -> vec4<f32>` as sprite shaders.
// The vertex stage below places each particle from its birth record and the system's clock:
// in.uv runs 0..1 over the particle (its sprite frame), in.color is the color over life times
// the tints (premultiplied linear), in.data is (life fraction 0..1, seed 0..1, age in seconds,
// speed in units per second).

struct Globals {
    proj: mat4x4<f32>,
    resolution: vec2<f32>,
    time: f32,
    _pad: f32,
}

struct Params {
    // The material's 16 numbers, as for any shader.
    p: array<vec4<f32>, 4>,
    // Node-to-pixels transform (x axis, y axis), its translation, clock, opacity.
    axes: vec4<f32>,
    origin_clock: vec4<f32>,
    // gravity x, y, drag, stretch
    motion: vec4<f32>,
    // wander amplitude, frequency, size at birth, size at death
    wander_size: vec4<f32>,
    colors: array<vec4<f32>, 3>,
    // fade in, fade out, frame columns, rows
    fade_frames: vec4<f32>,
    animate: vec4<f32>,
    tint: vec4<f32>,
    _pad: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var<uniform> params: Params;
@group(3) @binding(0) var mask_tex: texture_2d<f32>;
@group(3) @binding(1) var mask_samp: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) pixel: vec2<f32>,
    @location(3) data: vec4<f32>,
}

struct ParticleIn {
    @location(0) pos_vel: vec4<f32>,
    @location(1) birth_life_size_spin: vec4<f32>,
    @location(2) angle_seed: vec2<f32>,
    @location(3) tint: vec4<f32>,
}

fn kulisa_srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

fn kulisa_rotate(v: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(v.x * c - v.y * s, v.x * s + v.y * c);
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, p: ParticleIn) -> VertexOut {
    var out: VertexOut;
    let clock = params.origin_clock.z;
    let birth = p.birth_life_size_spin.x;
    let life = p.birth_life_size_spin.y;
    let age = clock - birth;
    let t = age / max(life, 0.0001);
    if (life <= 0.0 || age < 0.0 || t > 1.0) {
        // Not alive: a degenerate triangle outside the clip volume.
        out.position = vec4<f32>(-2.0, -2.0, 0.0, 1.0);
        return out;
    }
    // Motion in closed form: velocity with linear drag, gravity, and a wandering drift.
    let pos = p.pos_vel.xy;
    let vel = p.pos_vel.zw;
    let gravity = params.motion.xy;
    let drag = params.motion.z;
    var disp: vec2<f32>;
    var v_now: vec2<f32>;
    if (drag > 0.0001) {
        let e = exp(-drag * age);
        let k = (1.0 - e) / drag;
        disp = vel * k + gravity * (age - k) / drag;
        v_now = vel * e + gravity * k;
    } else {
        disp = vel * age + 0.5 * gravity * age * age;
        v_now = vel + gravity * age;
    }
    let seed = p.angle_seed.y;
    let amp = params.wander_size.x;
    let freq = params.wander_size.y * 6.2831853;
    let ph = seed * 43.0;
    disp += amp * vec2<f32>(sin(age * freq + ph) + 0.5 * sin(age * freq * 2.3 + ph * 1.7),
                            cos(age * freq * 0.8 + ph * 0.6) + 0.5 * cos(age * freq * 1.9 + ph * 2.3)) * 0.66;
    let center = pos + disp;

    // The quad: two triangles, corners from the vertex index.
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let corner = corners[vi % 6u];
    let size = mix(params.wander_size.z, params.wander_size.w, t) * p.birth_life_size_spin.z;
    var local = (corner - 0.5) * size;
    let speed = length(v_now);
    let stretch = params.motion.w;
    if (stretch > 0.0 && speed > 0.0001) {
        // Stretched along the velocity (sparks, rain).
        local.x *= 1.0 + stretch * speed / max(size, 0.0001);
        local = kulisa_rotate(local, atan2(v_now.y, v_now.x));
    } else {
        local = kulisa_rotate(local, p.angle_seed.x + p.birth_life_size_spin.w * age);
    }
    let lp = center + local;
    let pixel = params.axes.xy * lp.x + params.axes.zw * lp.y + params.origin_clock.xy;
    out.position = globals.proj * vec4<f32>(pixel, 0.0, 1.0);
    out.pixel = pixel;

    // Sprite sheet frame: random per particle, or playing over the life.
    let cols = params.fade_frames.z;
    let rows = params.fade_frames.w;
    let count = cols * rows;
    var frame = floor(seed * count);
    if (params.animate.x > 0.5) {
        frame = min(floor(t * count), count - 1.0);
    }
    let cell = vec2<f32>(frame - floor(frame / cols) * cols, floor(frame / cols));
    out.uv = (cell + corner) / vec2<f32>(cols, rows);

    // Color over life (three stops), fades, tints; premultiplied.
    var c: vec4<f32>;
    if (t < 0.5) {
        c = mix(params.colors[0], params.colors[1], t * 2.0);
    } else {
        c = mix(params.colors[1], params.colors[2], t * 2.0 - 1.0);
    }
    let fade_in = params.fade_frames.x;
    let fade_out = params.fade_frames.y;
    var a = c.a;
    if (fade_in > 0.0) {
        a *= clamp(t / fade_in, 0.0, 1.0);
    }
    if (fade_out > 0.0) {
        a *= clamp((1.0 - t) / fade_out, 0.0, 1.0);
    }
    let tint = vec4<f32>(kulisa_srgb_to_linear(p.tint.rgb), p.tint.a) * params.tint;
    let rgb = c.rgb * tint.rgb;
    a *= tint.a * params.origin_clock.w;
    out.color = vec4<f32>(rgb * a, a);
    out.data = vec4<f32>(t, seed, age, speed);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return fragment(in);
}
