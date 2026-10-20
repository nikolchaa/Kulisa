//! Particle nodes: `kulisa.particles{}`, their properties and methods, and the per-frame step
//! that emits new particles and uploads their records (the GPU does the rest).

use kulisa_core::{Color, Node, NodeId, NodeKind, ParticleBufferId, ParticleSystem, Shape, Span, Vec2};
use kulisa_render::Image;
use mlua::{Table, Value as LuaValue};

use super::api::{color, image_arg, number};
use super::{ImageInfo, State};

/// Options `kulisa.particles` reads itself (the rest are node properties).
pub(crate) const OPTIONS: &[&str] = &[
    "image",
    "capacity",
    "rate",
    "emitting",
    "emitter",
    "shape",
    "width",
    "height",
    "radius",
    "shape_angle",
    "direction",
    "spread",
    "speed",
    "life",
    "scale",
    "spin",
    "rotation",
    "tints",
    "gravity",
    "drag",
    "wander",
    "size",
    "colors",
    "fade",
    "frames",
    "animate",
    "stretch",
    "time_scale",
    "warm_up",
    "seed",
];

/// The particle systems in the scene and their GPU buffers.
#[derive(Default)]
pub(crate) struct Systems {
    list: Vec<(NodeId, ParticleBufferId)>,
    /// A soft round dot for systems without an image.
    dot: Option<ImageInfo>,
}

fn bad(what: &str, expected: &str) -> mlua::Error {
    mlua::Error::runtime(format!("`{what}` must be {expected}"))
}

/// A number or `{ min, max }`.
fn span(v: &LuaValue, what: &str, k: f32) -> mlua::Result<Span> {
    match v {
        LuaValue::Table(t) => {
            let a = number(&t.raw_get(1)?, what)?;
            let b = number(&t.raw_get::<LuaValue>(2)?.clone(), what).unwrap_or(a);
            Ok(Span(a * k, b * k))
        }
        v => Ok(Span::fixed(number(v, what)? * k)),
    }
}

/// A number or `{ a, b }` (a pair with the same default for both).
fn pair(v: &LuaValue, what: &str) -> mlua::Result<(f32, f32)> {
    match v {
        LuaValue::Table(t) => {
            let a = number(&t.raw_get(1)?, what)?;
            Ok((a, number(&t.raw_get(2)?, what).unwrap_or(a)))
        }
        v => {
            let a = number(v, what)?;
            Ok((a, a))
        }
    }
}

fn point(v: &LuaValue, what: &str) -> mlua::Result<Vec2> {
    match v {
        LuaValue::Table(t) => {
            let x = t.get::<LuaValue>("x")?;
            let (x, y) = if x.is_nil() {
                (t.raw_get::<LuaValue>(1)?, t.raw_get::<LuaValue>(2)?)
            } else {
                (x, t.get::<LuaValue>("y")?)
            };
            Ok(Vec2::new(number(&x, what)?, number(&y, what)?))
        }
        _ => Err(bad(what, "{ x, y }")),
    }
}

/// Applies the options in `o` (any subset) to `ps`.
pub(crate) fn configure(st: &mut State, ps: &mut ParticleSystem, o: &Table) -> mlua::Result<()> {
    let deg = std::f32::consts::PI / 180.0;
    let get = |k: &str| o.get::<LuaValue>(k);
    let e = &mut ps.emitter;
    if let v @ (LuaValue::Integer(_) | LuaValue::Number(_)) = get("rate")? {
        e.rate = number(&v, "rate")?.max(0.0);
    }
    if let LuaValue::Boolean(b) = get("emitting")? {
        e.emitting = b;
    }
    if !get("emitter")?.is_nil() {
        e.pos = point(&get("emitter")?, "emitter")?;
    }
    let dim = |k: &str, default: f32| -> mlua::Result<f32> {
        match get(k)? {
            LuaValue::Nil => Ok(default),
            v => number(&v, k),
        }
    };
    if let LuaValue::String(s) = get("shape")? {
        let angle = dim("shape_angle", 0.0)? * deg;
        e.shape = match s.to_string_lossy().as_str() {
            "point" => Shape::Point,
            "rect" => Shape::Rect(Vec2::new(dim("width", 100.0)?, dim("height", 100.0)?)),
            "circle" => Shape::Circle(dim("radius", 50.0)?),
            "ring" => Shape::Ring(dim("radius", 50.0)?),
            "line" => Shape::Line(dim("width", 100.0)?, angle),
            _ => return Err(bad("shape", "\"point\", \"rect\", \"circle\", \"ring\" or \"line\"")),
        };
    }
    if !get("direction")?.is_nil() {
        e.direction = number(&get("direction")?, "direction")? * deg;
    }
    if !get("spread")?.is_nil() {
        e.spread = number(&get("spread")?, "spread")? * deg;
    }
    if !get("speed")?.is_nil() {
        e.speed = span(&get("speed")?, "speed", 1.0)?;
    }
    if !get("life")?.is_nil() {
        e.life = span(&get("life")?, "life", 1.0)?;
    }
    if !get("scale")?.is_nil() {
        e.scale = span(&get("scale")?, "scale", 1.0)?;
    }
    if !get("spin")?.is_nil() {
        e.spin = span(&get("spin")?, "spin", deg)?;
    }
    match get("rotation")? {
        LuaValue::Nil => {}
        LuaValue::String(s) if s.to_string_lossy() == "random" => e.angle = None,
        v => e.angle = Some(number(&v, "rotation")? * deg),
    }
    if let LuaValue::Table(t) = get("tints")? {
        e.tints =
            t.sequence_values::<LuaValue>().map(|c| color(&c?, "tints")).collect::<mlua::Result<_>>()?;
    }
    let l = &mut ps.look;
    match get("gravity")? {
        LuaValue::Nil => {}
        v @ LuaValue::Table(_) => l.gravity = point(&v, "gravity")?,
        v => l.gravity = Vec2::new(0.0, number(&v, "gravity")?),
    }
    if !get("drag")?.is_nil() {
        l.drag = number(&get("drag")?, "drag")?.max(0.0);
    }
    if !get("wander")?.is_nil() {
        l.wander = match get("wander")? {
            LuaValue::Table(t) => {
                (number(&t.raw_get(1)?, "wander")?, number(&t.raw_get(2)?, "wander").unwrap_or(0.5))
            }
            v => (number(&v, "wander")?, 0.5),
        };
    }
    if !get("size")?.is_nil() {
        l.size = pair(&get("size")?, "size")?;
    }
    match get("colors")? {
        LuaValue::Nil => {}
        LuaValue::Table(t)
            if t.raw_len() > 0 && !matches!(t.raw_get::<LuaValue>(1)?, LuaValue::Number(_)) =>
        {
            let cs: Vec<Color> =
                t.sequence_values::<LuaValue>().map(|c| color(&c?, "colors")).collect::<mlua::Result<_>>()?;
            l.colors = match cs.as_slice() {
                [a] => [*a, *a, *a],
                [a, b] => [
                    *a,
                    Color::rgba((a.r + b.r) / 2.0, (a.g + b.g) / 2.0, (a.b + b.b) / 2.0, (a.a + b.a) / 2.0),
                    *b,
                ],
                [a, b, c, ..] => [*a, *b, *c],
                [] => l.colors,
            };
        }
        v => {
            let c = color(&v, "colors")?;
            l.colors = [c, c, c];
        }
    }
    if !get("fade")?.is_nil() {
        l.fade = pair(&get("fade")?, "fade")?;
    }
    if let LuaValue::Table(t) = get("frames")? {
        let cols = number(&t.raw_get(1)?, "frames")?.max(1.0) as u32;
        let rows = number(&t.raw_get(2)?, "frames").unwrap_or(1.0).max(1.0) as u32;
        l.frames = (cols, rows);
    }
    if let LuaValue::Boolean(b) = get("animate")? {
        l.animate = b;
    }
    if !get("stretch")?.is_nil() {
        l.stretch = number(&get("stretch")?, "stretch")?.max(0.0);
    }
    if !get("time_scale")?.is_nil() {
        ps.speed = number(&get("time_scale")?, "time_scale")?.max(0.0);
    }
    if !get("seed")?.is_nil() {
        ps.seed(number(&get("seed")?, "seed")? as u64);
    }
    if !get("image")?.is_nil() {
        ps.texture = image_arg(st, &get("image")?)?.id;
    }
    Ok(())
}

/// A white disc with soft edges (64 px), for systems without an image.
fn dot(st: &mut State) -> ImageInfo {
    if let Some(d) = st.particles.dot {
        return d;
    }
    let n = 64u32;
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(Vec2::splat(n as f32 / 2.0))
                / (n as f32 / 2.0);
            let a = (1.0 - d).clamp(0.0, 1.0).powf(1.6);
            px.extend_from_slice(&[255, 255, 255, (a * 255.0).round() as u8]);
        }
    }
    let id = st.renderer.create_texture(&Image::from_straight_rgba(n, n, px));
    let info = ImageInfo { id, size: kulisa_core::UVec2::splat(n) };
    st.particles.dot = Some(info);
    info
}

/// Makes the node for `kulisa.particles{}`.
pub(crate) fn make(st: &mut State, o: &Table) -> mlua::Result<Node> {
    let capacity = match o.get::<LuaValue>("capacity")? {
        LuaValue::Nil => 500,
        v => number(&v, "capacity")?.clamp(1.0, 262_144.0) as usize,
    };
    let tex = dot(st).id;
    let mut ps = ParticleSystem::new(tex, capacity);
    configure(st, &mut ps, o)?;
    if let Some(secs) = o.get::<Option<f32>>("warm_up")? {
        ps.warm_up(secs.max(0.0));
    }
    let buffer = st.renderer.create_particles(capacity as u32);
    ps.buffer = Some(buffer);
    ps.mark_all_dirty();
    Ok(Node::particles(ps))
}

/// Records a new particle node so its system steps every frame.
pub(crate) fn added(st: &mut State, id: NodeId) {
    if let Some(NodeKind::Particles(ps)) = st.scene.get(id).map(|n| &n.kind)
        && let Some(b) = ps.buffer
    {
        st.particles.list.push((id, b));
    }
}

/// Steps every system by `dt`, uploads what was emitted and frees the buffers of removed ones.
pub(crate) fn update(st: &mut State, dt: f32) {
    let State { scene, renderer, particles, .. } = st;
    particles.list.retain(|&(id, buffer)| match scene.get_mut(id).map(|n| &mut n.kind) {
        Some(NodeKind::Particles(ps)) => {
            ps.step(dt);
            if let Some((first, records)) = ps.take_dirty() {
                renderer.write_particles(buffer, first, records);
            }
            true
        }
        _ => {
            renderer.free_particles(buffer);
            false
        }
    });
}

fn system(st: &mut State, id: NodeId) -> mlua::Result<&mut ParticleSystem> {
    match st.scene.get_mut(id).map(|n| &mut n.kind) {
        Some(NodeKind::Particles(ps)) => Ok(ps),
        Some(_) => Err(mlua::Error::runtime("this method needs a particles node (kulisa.particles)")),
        None => Err(mlua::Error::runtime("this node was removed")),
    }
}

/// Properties of particle nodes, or `None` for other nodes and keys.
pub(crate) fn get(st: &State, id: NodeId, key: &str) -> Option<LuaValue> {
    let Some(NodeKind::Particles(ps)) = st.scene.get(id).map(|n| &n.kind) else { return None };
    Some(match key {
        "rate" => LuaValue::Number(ps.emitter.rate as f64),
        "emitting" => LuaValue::Boolean(ps.emitter.emitting),
        "emitter_x" => LuaValue::Number(ps.emitter.pos.x as f64),
        "emitter_y" => LuaValue::Number(ps.emitter.pos.y as f64),
        "time_scale" => LuaValue::Number(ps.speed as f64),
        "alive" => LuaValue::Integer(ps.alive() as _),
        "capacity" => LuaValue::Integer(ps.capacity() as _),
        "clock" => LuaValue::Number(ps.clock as f64),
        _ => return None,
    })
}

/// Sets a particle node property; `None` if `key` is not one.
pub(crate) fn set(st: &mut State, id: NodeId, key: &str, v: &LuaValue) -> Option<mlua::Result<()>> {
    if !matches!(key, "rate" | "emitting" | "emitter_x" | "emitter_y" | "time_scale") {
        return None;
    }
    let ps = system(st, id).ok()?;
    let num = |what: &str| number(v, what);
    Some((|| {
        match key {
            "rate" => ps.emitter.rate = num("rate")?.max(0.0),
            "emitting" => ps.emitter.emitting = v.as_boolean().ok_or_else(|| bad("emitting", "a boolean"))?,
            "emitter_x" => ps.emitter.pos.x = num("emitter_x")?,
            "emitter_y" => ps.emitter.pos.y = num("emitter_y")?,
            _ => ps.speed = num("time_scale")?.max(0.0),
        }
        Ok(())
    })())
}

/// `ps:burst(n, x?, y?)`: `n` particles now, at a point in the node's space or the emitter.
pub(crate) fn burst(st: &mut State, id: NodeId, n: usize, at: Option<Vec2>) -> mlua::Result<()> {
    system(st, id)?.burst(n.min(65_536), at);
    Ok(())
}

/// `ps:reset()`: removes every particle.
pub(crate) fn reset(st: &mut State, id: NodeId) -> mlua::Result<()> {
    system(st, id)?.clear();
    Ok(())
}

/// `ps:set{...}`: changes any of the options.
pub(crate) fn set_options(st: &mut State, id: NodeId, o: &Table) -> mlua::Result<()> {
    let white = st.white.id;
    let mut ps = std::mem::replace(system(st, id)?, ParticleSystem::new(white, 1));
    let r = configure(st, &mut ps, o);
    *system(st, id)? = ps;
    r
}
