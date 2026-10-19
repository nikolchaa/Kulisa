//! The `kulisa` table: everything a game script can do. Documented in docs/API.md.

use std::sync::Arc;

use kulisa_audio::PlayOptions;
use kulisa_core::{
    BlendMode, Camera, Color, Ease, Material, Migrations, Node, NodeId, NodeKind, Params, Prop, Rect,
    SaveFile, SaveHeader, ScaleMode, Slice, Sprite, Tween, UVec2, Value, Vec2, Viewport,
};
use kulisa_platform::Storage;
use kulisa_render::Image;
use kulisa_story::{Vm, compile};
use kulisa_text::{Align, TextBox, TextStyle, Typewriter};
use mlua::{
    AnyUserData, Function, IntoLua, Lua, MetaMethod, Table, UserData, UserDataMethods, Value as LuaValue,
    Variadic,
};

use super::display;
use super::hit;
use super::media::{self, TransitionSpec};
use super::mesh;
use super::{
    ImageInfo, Shared, State, TextNode, Waiting, choice_key, report, story_advance, story_restored,
    with_story,
};
use crate::story::{from_value, to_value};

/// Event names `kulisa.on` accepts.
const EVENTS: &[&str] = &[
    "update",
    "press",
    "release",
    "move",
    "key",
    "key_up",
    "wheel",
    "resize",
    "error",
    "say",
    "menu",
    "pause",
    "end",
    "stage",
    "restore",
    "tap",
    "long_press",
    "swipe",
    "pinch",
    "rotate",
    "pad_press",
    "pad_release",
    "pad_axis",
    "pad_connect",
    "pad_disconnect",
    "text",
    "preedit",
    "display",
];

pub(crate) fn shared(lua: &Lua) -> Shared {
    lua.app_data_ref::<Shared>().expect("kulisa game state").clone()
}

pub(crate) fn rt<T>(r: Result<T, String>) -> mlua::Result<T> {
    r.map_err(mlua::Error::runtime)
}

/// Runs `f` with the engine state. Never call Luau inside `f`.
pub(crate) fn with<R>(lua: &Lua, f: impl FnOnce(&mut State) -> mlua::Result<R>) -> mlua::Result<R> {
    let shared = shared(lua);
    let mut st = shared.try_borrow_mut().map_err(|_| mlua::Error::runtime("the engine is busy"))?;
    f(&mut st)
}

pub(crate) fn number(v: &LuaValue, what: &str) -> mlua::Result<f32> {
    match v {
        LuaValue::Number(n) => Ok(*n as f32),
        #[allow(clippy::unnecessary_cast)]
        LuaValue::Integer(i) => Ok(*i as f32),
        _ => Err(mlua::Error::runtime(format!("`{what}` must be a number, not {}", v.type_name()))),
    }
}

pub(crate) fn color(v: &LuaValue, what: &str) -> mlua::Result<Color> {
    let parsed = match v {
        LuaValue::String(s) => Color::hex(&s.to_string_lossy()),
        LuaValue::Table(t) => {
            let c = |i: usize, default: f32| t.raw_get::<Option<f32>>(i).ok().flatten().unwrap_or(default);
            Some(Color::srgb(c(1, 0.0), c(2, 0.0), c(3, 0.0), c(4, 1.0)))
        }
        _ => None,
    };
    parsed.ok_or_else(|| {
        mlua::Error::runtime(format!("`{what}` must be a color like \"#ff8800\" or {{r, g, b, a}} (0..1)"))
    })
}

fn hex(c: Color) -> String {
    let encode = |v: f32| {
        let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (s.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    let a = (c.a.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", encode(c.r), encode(c.g), encode(c.b), a)
}

const BLENDS: &[(&str, BlendMode)] = &[
    ("alpha", BlendMode::Alpha),
    ("add", BlendMode::Add),
    ("multiply", BlendMode::Multiply),
    ("screen", BlendMode::Screen),
];

const ALIGNS: &[(&str, Align)] = &[
    ("left", Align::Left),
    ("center", Align::Center),
    ("right", Align::Right),
    ("justified", Align::Justified),
];

const MODES: &[(&str, ScaleMode)] =
    &[("fit", ScaleMode::Fit), ("fill", ScaleMode::Fill), ("expand", ScaleMode::Expand)];

fn named<T: Copy + PartialEq>(table: &[(&str, T)], v: &LuaValue, what: &str) -> mlua::Result<T> {
    let s = match v {
        LuaValue::String(s) => s.to_string_lossy(),
        _ => String::new(),
    };
    table.iter().find(|(n, _)| *n == s).map(|(_, t)| *t).ok_or_else(|| {
        let names: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
        mlua::Error::runtime(format!("`{what}` must be one of: {}", names.join(", ")))
    })
}

fn name_of<T: Copy + PartialEq>(table: &[(&'static str, T)], v: T) -> &'static str {
    table.iter().find(|(_, t)| *t == v).map_or("?", |(n, _)| n)
}

fn params(v: &LuaValue) -> mlua::Result<Params> {
    let LuaValue::Table(t) = v else {
        return Err(mlua::Error::runtime("`params` must be a list of numbers"));
    };
    let mut p = [0.0; 16];
    let len = t.raw_len();
    if len > 16 {
        return Err(mlua::Error::runtime("`params` holds at most 16 numbers"));
    }
    for (i, slot) in p.iter_mut().enumerate().take(len) {
        *slot = number(&t.raw_get(i + 1)?, "params")?;
    }
    Ok(p)
}

fn params_table(lua: &Lua, p: &Params) -> mlua::Result<LuaValue> {
    let end = p.iter().rposition(|v| *v != 0.0).map_or(0, |i| i + 1);
    lua.create_sequence_from(p[..end].iter().copied()).map(LuaValue::Table)
}

/// `{ shader, shader, ... }` or `{ { shader = s, params = {...}, mask = image }, ... }`.
fn effects(st: &mut State, v: &LuaValue) -> mlua::Result<Vec<Material>> {
    let bad =
        || mlua::Error::runtime("`effects` must be a list of shaders or { shader = s, params = {...} }");
    match v {
        LuaValue::Nil => Ok(Vec::new()),
        LuaValue::Table(t) => t
            .sequence_values::<LuaValue>()
            .map(|item| match item? {
                LuaValue::UserData(ud) => {
                    Ok(Material::new(ud.borrow::<ShaderRef>().map_err(|_| bad())?.0, [0.0; 16]))
                }
                LuaValue::Table(e) => {
                    let shader = e.get::<AnyUserData>("shader").map_err(|_| bad())?;
                    let shader = shader.borrow::<ShaderRef>().map_err(|_| bad())?.0;
                    let p = match e.get::<LuaValue>("params")? {
                        LuaValue::Nil => [0.0; 16],
                        v => params(&v)?,
                    };
                    let mask = match e.get::<LuaValue>("mask")? {
                        LuaValue::Nil => None,
                        v => Some(image_arg(st, &v)?.id),
                    };
                    Ok(Material { shader, params: p, mask })
                }
                _ => Err(bad()),
            })
            .collect(),
        _ => Err(bad()),
    }
}

fn effects_table(lua: &Lua, list: &[Material]) -> mlua::Result<LuaValue> {
    let t = lua.create_table()?;
    for m in list {
        let e = lua.create_table()?;
        e.set("shader", ShaderRef(m.shader))?;
        e.set("params", params_table(lua, &m.params)?)?;
        if let Some(mask) = m.mask {
            e.set("mask", ImageRef(ImageInfo { id: mask, size: UVec2::ZERO }))?;
        }
        t.push(e)?;
    }
    Ok(LuaValue::Table(t))
}

// ---------------------------------------------------------------------------------------
// Handles

#[derive(Clone, Copy)]
pub(crate) struct NodeRef(pub NodeId);

#[derive(Clone, Copy)]
pub(crate) struct ImageRef(pub ImageInfo);

#[derive(Clone, Copy)]
pub(crate) struct ShaderRef(pub kulisa_core::ShaderId);

#[derive(Clone, Copy)]
pub(crate) struct TweenRef(pub u64);

#[derive(Clone, Copy)]
pub(crate) struct TransitionRef(pub u64);

struct CameraRef;
struct ScreenRef;
struct WindowRef;

impl UserData for WindowRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |lua, _, key: String| {
            with(lua, |st| match key.as_str() {
                "fullscreen" => Ok(st.display.requests.fullscreen.unwrap_or(st.display.fullscreen)),
                "can_quit" => Ok(st.display.device.can_quit),
                _ => Err(mlua::Error::runtime(format!("the window has no property `{key}`"))),
            })
        });
        m.add_meta_method(MetaMethod::NewIndex, |lua, _, (key, v): (String, LuaValue)| {
            with(lua, |st| match key.as_str() {
                "fullscreen" => {
                    let on = v
                        .as_boolean()
                        .ok_or_else(|| mlua::Error::runtime("`fullscreen` must be a boolean"))?;
                    st.display.requests.fullscreen = Some(on);
                    Ok(())
                }
                _ => Err(mlua::Error::runtime(format!("`window.{key}` cannot be set"))),
            })
        });
        // Closes the game where the platform allows it (desktop); see `can_quit`.
        m.add_method("quit", |lua, _, ()| {
            with(lua, |st| {
                st.flush_seen();
                st.display.requests.quit = true;
                Ok(())
            })
        });
    }
}

impl UserData for ImageRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |_, this, key: String| match key.as_str() {
            "width" => Ok(this.0.size.x),
            "height" => Ok(this.0.size.y),
            _ => Err(mlua::Error::runtime(format!("images have no property `{key}`"))),
        });
        m.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("image {}x{}", this.0.size.x, this.0.size.y))
        });
    }
}

impl UserData for ShaderRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(format!("shader {}", this.0.0)));
    }
}

impl UserData for TransitionRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| match key.as_str() {
            "done" => with(lua, |st| Ok(st.media.transition.as_ref().is_none_or(|t| t.id != this.0))),
            _ => Err(mlua::Error::runtime(format!("transitions have no property `{key}`"))),
        });
        m.add_method("skip", |lua, this, ()| {
            with(lua, |st| {
                if st.media.transition.as_ref().is_some_and(|t| t.id == this.0) {
                    media::finish_transition(st);
                }
                Ok(())
            })
        });
    }
}

/// `kulisa.screen.transition{ shader, time, ease, mask, params }`.
fn transition_spec(st: &mut State, o: &Table) -> mlua::Result<TransitionSpec> {
    let shader = match o.get::<LuaValue>("shader")? {
        LuaValue::UserData(ud) => ud.borrow::<ShaderRef>()?.0,
        LuaValue::String(s) => rt(load_shader(st, &s.to_string_lossy()))?,
        _ => return Err(mlua::Error::runtime("a transition needs a `shader` (a shader or a path)")),
    };
    let time = o.get::<Option<f32>>("time")?.unwrap_or(0.5).max(0.0);
    let ease = match o.get::<Option<String>>("ease")? {
        Some(name) => {
            Ease::from_name(&name).ok_or_else(|| mlua::Error::runtime(format!("unknown ease `{name}`")))?
        }
        None => Ease::Linear,
    };
    let mask = match o.get::<LuaValue>("mask")? {
        LuaValue::Nil => None,
        v => Some(image_arg(st, &v)?.id),
    };
    let mut p = [0.0; 16];
    if let LuaValue::Table(t) = o.get::<LuaValue>("params")? {
        if t.raw_len() > 15 {
            return Err(mlua::Error::runtime("transition `params` holds at most 15 numbers"));
        }
        for (i, slot) in p.iter_mut().enumerate().skip(1).take(t.raw_len()) {
            *slot = number(&t.raw_get(i)?, "params")?;
        }
    }
    Ok(TransitionSpec { shader, time, ease, mask, params: p })
}

impl UserData for TweenRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| match key.as_str() {
            "done" => with(lua, |st| Ok(!st.tweens.iter().any(|(id, _)| *id == this.0))),
            _ => Err(mlua::Error::runtime(format!("tweens have no property `{key}`"))),
        });
        m.add_method("cancel", |lua, this, ()| {
            with(lua, |st| {
                st.tweens.retain(|(id, _)| *id != this.0);
                Ok(())
            })
        });
    }
}

fn node_of(st: &State, id: NodeId) -> mlua::Result<&Node> {
    st.scene.get(id).ok_or_else(|| mlua::Error::runtime("this node was removed"))
}

fn node_mut(st: &mut State, id: NodeId) -> mlua::Result<&mut Node> {
    st.scene.get_mut(id).ok_or_else(|| mlua::Error::runtime("this node was removed"))
}

fn kind_name(kind: &NodeKind, is_text: bool) -> &'static str {
    match kind {
        NodeKind::Group => "group",
        NodeKind::Sprite(_) => "sprite",
        NodeKind::Quads(_) if is_text => "text",
        NodeKind::Quads(_) => "quads",
        NodeKind::Mesh(_) => "mesh",
        NodeKind::Layer(_) => "layer",
        NodeKind::Target(_) => "target",
    }
}

pub(crate) fn node_size(st: &State, id: NodeId) -> mlua::Result<Vec2> {
    Ok(match &node_of(st, id)?.kind {
        NodeKind::Sprite(s) => s.size,
        NodeKind::Target(t) => t.size.as_vec2(),
        NodeKind::Quads(_) => st.texts.get(&id).map_or(Vec2::ZERO, |t| t.text.layout.size),
        // The far edge of the vertices: what `center = true` and `width`/`height` mean here.
        NodeKind::Mesh(m) => m.bounds().max.max(Vec2::ZERO),
        _ => Vec2::ZERO,
    })
}

/// Lays out a text node again (after its markup, style or the display scale changed).
pub(crate) fn layout_text(st: &mut State, id: NodeId, restart: bool) -> Result<(), String> {
    let scale = st.scale();
    let State { texts, text, renderer, .. } = st;
    let Some(node) = texts.get_mut(&id) else { return Ok(()) };
    let rich = kulisa_text::parse(&node.markup).map_err(|e| format!("text markup {e}"))?;
    let elapsed = if restart { 0.0 } else { node.text.elapsed };
    node.text = TextBox::new(text.layout(renderer, &rich, &node.style, scale), node.text.typewriter);
    node.text.elapsed = elapsed;
    Ok(())
}

fn text_prop(lua: &Lua, t: &TextNode, key: &str) -> mlua::Result<Option<LuaValue>> {
    let num = |v: f32| Some(LuaValue::Number(v as f64));
    Ok(match key {
        "text" => Some(t.markup.as_str().into_lua(lua)?),
        "cps" => num(t.text.typewriter.cps),
        "fade" => num(t.text.typewriter.fade),
        "complete" => Some(LuaValue::Boolean(t.text.is_complete())),
        "duration" => num(t.text.duration()),
        "size" => num(t.style.size),
        "line_height" => num(t.style.line_height),
        "color" => Some(hex(t.style.color).into_lua(lua)?),
        "wrap" => t.style.width.and_then(num),
        "align" => Some(name_of(ALIGNS, t.style.align).into_lua(lua)?),
        "font" => t.style.font.as_deref().map(|f| f.into_lua(lua)).transpose()?,
        _ => None,
    })
}

/// Sets a text property; returns false if `key` is not one.
fn set_text_prop(st: &mut State, id: NodeId, key: &str, v: &LuaValue) -> mlua::Result<bool> {
    let Some(t) = st.texts.get_mut(&id) else { return Ok(false) };
    let restart = match key {
        "text" => {
            t.markup = match v {
                LuaValue::String(s) => s.to_string_lossy(),
                v => v.to_string()?,
            };
            true
        }
        "cps" => {
            t.text.typewriter.cps = number(v, key)?.max(0.0);
            return Ok(true);
        }
        "fade" => {
            t.text.typewriter.fade = number(v, key)?.max(0.0);
            return Ok(true);
        }
        "size" => {
            t.style.size = number(v, key)?;
            false
        }
        "line_height" => {
            t.style.line_height = number(v, key)?;
            false
        }
        "color" => {
            t.style.color = color(v, key)?;
            false
        }
        "wrap" => {
            t.style.width = if v.is_nil() { None } else { Some(number(v, key)?) };
            false
        }
        "align" => {
            t.style.align = named(ALIGNS, v, key)?;
            false
        }
        "font" => {
            t.style.font = match v {
                LuaValue::Nil => None,
                v => Some(v.to_string()?),
            };
            false
        }
        _ => return Ok(false),
    };
    rt(layout_text(st, id, restart))?;
    Ok(true)
}

fn get_prop(lua: &Lua, st: &State, id: NodeId, key: &str) -> mlua::Result<LuaValue> {
    let n = node_of(st, id)?;
    let t = &n.transform;
    let num = |v: f32| Ok(LuaValue::Number(v as f64));
    if let Some(text) = st.texts.get(&id)
        && let Some(v) = text_prop(lua, text, key)?
    {
        return Ok(v);
    }
    if let Some(v) = mesh::get(st, id, key) {
        return Ok(v.map_or(LuaValue::Nil, LuaValue::Number));
    }
    match key {
        "x" => num(t.position.x),
        "y" => num(t.position.y),
        "z" => num(n.z as f32),
        "rotation" => num(t.rotation),
        "scale" | "scale_x" => num(t.scale.x),
        "scale_y" => num(t.scale.y),
        "origin_x" => num(t.origin.x),
        "origin_y" => num(t.origin.y),
        "opacity" => num(n.opacity),
        "visible" => Ok(LuaValue::Boolean(n.visible)),
        "blend" => name_of(BLENDS, n.blend).into_lua(lua),
        "tint" => hex(n.tint).into_lua(lua),
        "kind" => kind_name(&n.kind, st.texts.contains_key(&id)).into_lua(lua),
        "parent" => match n.parent() {
            Some(p) if p != st.scene.root() => NodeRef(p).into_lua(lua),
            _ => Ok(LuaValue::Nil),
        },
        "width" => num(node_size(st, id)?.x),
        "height" => num(node_size(st, id)?.y),
        "image" => match &n.kind {
            NodeKind::Sprite(s) if s.texture == st.white.id => Ok(LuaValue::Nil),
            NodeKind::Sprite(s) => {
                let size = st.renderer.texture_size(s.texture).unwrap_or_default();
                ImageRef(ImageInfo { id: s.texture, size }).into_lua(lua)
            }
            NodeKind::Mesh(m) if m.texture == st.white.id => Ok(LuaValue::Nil),
            NodeKind::Mesh(m) => {
                let size = st.renderer.texture_size(m.texture).unwrap_or_default();
                ImageRef(ImageInfo { id: m.texture, size }).into_lua(lua)
            }
            _ => Ok(LuaValue::Nil),
        },
        "texture" => match &n.kind {
            NodeKind::Target(t) => ImageRef(ImageInfo { id: t.texture, size: t.size }).into_lua(lua),
            _ => Ok(LuaValue::Nil),
        },
        "parallax" => match &n.kind {
            NodeKind::Layer(l) => num(l.parallax),
            _ => Ok(LuaValue::Nil),
        },
        "effects" => match &n.kind {
            NodeKind::Layer(l) => effects_table(lua, &l.effects),
            NodeKind::Target(t) => effects_table(lua, &t.effects),
            _ => Ok(LuaValue::Nil),
        },
        "clear" => match &n.kind {
            NodeKind::Target(t) => t.clear.map(hex).into_lua(lua),
            _ => Ok(LuaValue::Nil),
        },
        "clip" => match n.clip {
            Some(r) => {
                lua.create_sequence_from([r.min.x, r.min.y, r.size().x, r.size().y]).map(LuaValue::Table)
            }
            None => Ok(LuaValue::Nil),
        },
        "shader" => n.material.map(|m| ShaderRef(m.shader)).into_lua(lua),
        "params" => match &n.material {
            Some(m) => params_table(lua, &m.params),
            None => Ok(LuaValue::Nil),
        },
        "mask" => match n.material.and_then(|m| m.mask) {
            Some(id) => {
                let size = st.renderer.texture_size(id).unwrap_or_default();
                ImageRef(ImageInfo { id, size }).into_lua(lua)
            }
            None => Ok(LuaValue::Nil),
        },
        "slice" => match &n.kind {
            NodeKind::Sprite(Sprite { slice: Some(s), .. }) => {
                lua.create_sequence_from([s.left, s.top, s.right, s.bottom]).map(LuaValue::Table)
            }
            _ => Ok(LuaValue::Nil),
        },
        "slice_scale" => match &n.kind {
            NodeKind::Sprite(Sprite { slice: Some(s), .. }) => num(s.scale),
            _ => Ok(LuaValue::Nil),
        },
        _ => Err(mlua::Error::runtime(format!("nodes have no property `{key}`"))),
    }
}

/// An image from a handle or an asset path.
pub(crate) fn image_arg(st: &mut State, v: &LuaValue) -> mlua::Result<ImageInfo> {
    match v {
        LuaValue::UserData(ud) => Ok(ud.borrow::<ImageRef>()?.0),
        LuaValue::String(s) => rt(load_image(st, &s.to_string_lossy())),
        _ => Err(mlua::Error::runtime("`image` must be an image or an asset path")),
    }
}

/// `slice = 12` (every border) or `{ left, top, right, bottom }`, in image pixels.
fn slice(v: &LuaValue, source: Vec2, scale: f32) -> mlua::Result<Option<Slice>> {
    let b = match v {
        LuaValue::Nil => return Ok(None),
        LuaValue::Table(t) => {
            let c = |i: usize| -> mlua::Result<f32> { number(&t.raw_get(i)?, "slice") };
            [c(1)?, c(2)?, c(3)?, c(4)?]
        }
        v => [number(v, "slice")?; 4],
    };
    if b.iter().any(|x| *x < 0.0) {
        return Err(mlua::Error::runtime("`slice` borders cannot be negative"));
    }
    Ok(Some(Slice { left: b[0], top: b[1], right: b[2], bottom: b[3], source, scale }))
}

fn set_prop(st: &mut State, id: NodeId, key: &str, v: LuaValue) -> mlua::Result<()> {
    if set_text_prop(st, id, key, &v)? {
        return Ok(());
    }
    let image =
        if (key == "image" || key == "mask") && !v.is_nil() { Some(image_arg(st, &v)?) } else { None };
    let fx = if key == "effects" { Some(effects(st, &v)?) } else { None };
    let slice_source = match node_of(st, id)?.kind {
        NodeKind::Sprite(ref s) => {
            st.renderer.texture_size(image.map_or(s.texture, |i| i.id)).unwrap_or(UVec2::ONE)
        }
        _ => UVec2::ONE,
    };
    let white = st.white;
    let root = st.scene.root();
    if key == "parent" {
        let parent = match &v {
            LuaValue::Nil => root,
            LuaValue::UserData(ud) => ud.borrow::<NodeRef>()?.0,
            _ => return Err(mlua::Error::runtime("`parent` must be a node or nil")),
        };
        node_of(st, parent)?;
        return if st.scene.reparent(id, parent) {
            Ok(())
        } else {
            Err(mlua::Error::runtime("a node cannot be its own ancestor"))
        };
    }
    let n = node_mut(st, id)?;
    let t = &mut n.transform;
    match key {
        "x" => t.position.x = number(&v, key)?,
        "y" => t.position.y = number(&v, key)?,
        "z" => n.z = number(&v, key)? as i32,
        "rotation" => t.rotation = number(&v, key)?,
        "scale" => t.scale = Vec2::splat(number(&v, key)?),
        "scale_x" => t.scale.x = number(&v, key)?,
        "scale_y" => t.scale.y = number(&v, key)?,
        "origin_x" => t.origin.x = number(&v, key)?,
        "origin_y" => t.origin.y = number(&v, key)?,
        "opacity" => n.opacity = number(&v, key)?,
        "visible" => {
            n.visible = v.as_boolean().ok_or_else(|| mlua::Error::runtime("`visible` must be a boolean"))?
        }
        "blend" => n.blend = named(BLENDS, &v, key)?,
        "tint" => n.tint = color(&v, key)?,
        "width" | "height" => match &mut n.kind {
            NodeKind::Sprite(s) if key == "width" => s.size.x = number(&v, key)?,
            NodeKind::Sprite(s) => s.size.y = number(&v, key)?,
            _ => return Err(mlua::Error::runtime(format!("`{key}` can only be set on sprites"))),
        },
        "image" => match &mut n.kind {
            NodeKind::Sprite(s) => {
                s.texture = image.unwrap_or(white).id;
                if let Some(sl) = &mut s.slice {
                    sl.source = slice_source.as_vec2();
                }
            }
            NodeKind::Mesh(m) => m.texture = image.unwrap_or(white).id,
            _ => return Err(mlua::Error::runtime("`image` can only be set on sprites and meshes")),
        },
        "slice" => match &mut n.kind {
            NodeKind::Sprite(s) => {
                s.slice = slice(&v, slice_source.as_vec2(), s.slice.map_or(1.0, |s| s.scale))?
            }
            _ => return Err(mlua::Error::runtime("`slice` can only be set on sprites")),
        },
        "slice_scale" => match &mut n.kind {
            NodeKind::Sprite(Sprite { slice: Some(sl), .. }) => sl.scale = number(&v, key)?,
            _ => return Err(mlua::Error::runtime("set `slice` before `slice_scale`")),
        },
        "mask" => match &mut n.material {
            Some(m) => m.mask = image.map(|i| i.id),
            None => return Err(mlua::Error::runtime("set `shader` before `mask`")),
        },
        "parallax" => match &mut n.kind {
            NodeKind::Layer(l) => l.parallax = number(&v, key)?,
            _ => return Err(mlua::Error::runtime("`parallax` can only be set on layers")),
        },
        "effects" => match &mut n.kind {
            NodeKind::Layer(l) => l.effects = fx.unwrap(),
            NodeKind::Target(t) => t.effects = fx.unwrap(),
            _ => return Err(mlua::Error::runtime("`effects` can only be set on layers and targets")),
        },
        "clear" => match &mut n.kind {
            NodeKind::Target(t) => t.clear = if v.is_nil() { None } else { Some(color(&v, key)?) },
            _ => return Err(mlua::Error::runtime("`clear` can only be set on targets")),
        },
        "clip" => {
            n.clip = match &v {
                LuaValue::Nil => None,
                LuaValue::Table(t) => {
                    let c = |i: usize| -> mlua::Result<f32> { number(&t.raw_get(i)?, "clip") };
                    Some(Rect::new(c(1)?, c(2)?, c(3)?, c(4)?))
                }
                _ => return Err(mlua::Error::runtime("`clip` must be {x, y, width, height} or nil")),
            }
        }
        "shader" => {
            n.material = match &v {
                LuaValue::Nil => None,
                LuaValue::UserData(ud) => {
                    let shader = ud.borrow::<ShaderRef>()?.0;
                    let old = n.material;
                    Some(Material {
                        shader,
                        params: old.map_or([0.0; 16], |m| m.params),
                        mask: old.and_then(|m| m.mask),
                    })
                }
                _ => return Err(mlua::Error::runtime("`shader` must be a shader or nil")),
            }
        }
        "params" => match &mut n.material {
            Some(m) => m.params = params(&v)?,
            None => return Err(mlua::Error::runtime("set `shader` before `params`")),
        },
        "kind" | "texture" | "complete" | "duration" | "vertex_count" | "triangle_count" | "cols"
        | "rows" => {
            return Err(mlua::Error::runtime(format!("`{key}` is read-only")));
        }
        _ => return Err(mlua::Error::runtime(format!("nodes have no property `{key}`"))),
    }
    Ok(())
}

impl UserData for NodeRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            with(lua, |st| get_prop(lua, st, this.0, &key))
        });
        m.add_meta_method(MetaMethod::NewIndex, |lua, this, (key, v): (String, LuaValue)| {
            with(lua, |st| set_prop(st, this.0, &key, v))
        });
        m.add_meta_method(MetaMethod::Eq, |_, this, other: AnyUserData| {
            Ok(other.borrow::<NodeRef>().is_ok_and(|o| o.0 == this.0))
        });
        m.add_meta_method(MetaMethod::ToString, |lua, this, ()| {
            with(lua, |st| {
                Ok(match st.scene.get(this.0) {
                    Some(n) => kind_name(&n.kind, st.texts.contains_key(&this.0)).to_string(),
                    None => "removed node".to_string(),
                })
            })
        });
        m.add_method("remove", |lua, this, ()| {
            with(lua, |st| {
                if this.0 != st.scene.root() {
                    st.scene.remove(this.0);
                }
                Ok(())
            })
        });
        m.add_method("exists", |lua, this, ()| with(lua, |st| Ok(st.scene.contains(this.0))));
        m.add_method("tween", |lua, this, (props, opts): (Table, LuaValue)| {
            tween(lua, this.0, false, props, opts)
        });
        m.add_method("contains", |lua, this, (x, y, alpha): (f32, f32, LuaValue)| {
            let alpha = hit::threshold(&alpha)?;
            with(lua, |st| Ok(hit::contains(st, this.0, Vec2::new(x, y), alpha)))
        });
        m.add_method("skip", |lua, this, ()| {
            with(lua, |st| {
                if let Some(t) = st.texts.get_mut(&this.0) {
                    t.text.skip();
                }
                Ok(())
            })
        });
        m.add_method("restart", |lua, this, ()| {
            with(lua, |st| {
                if let Some(t) = st.texts.get_mut(&this.0) {
                    t.text.elapsed = 0.0;
                }
                Ok(())
            })
        });
        // Mesh editing: bulk setters take a flat table or a buffer of f32.
        for (method, array) in [
            ("set_positions", "positions"),
            ("set_uvs", "uvs"),
            ("set_colors", "colors"),
            ("set_data", "data"),
            ("set_indices", "indices"),
        ] {
            m.add_method(method, move |lua, this, data: LuaValue| {
                with(lua, |st| mesh::set_array(st, this.0, array, &data))
            });
        }
        for (method, array) in
            [("positions", "positions"), ("uvs", "uvs"), ("colors", "colors"), ("data", "data")]
        {
            m.add_method(method, move |lua, this, buf: Option<mlua::Buffer>| {
                with(lua, |st| mesh::get_array(lua, st, this.0, array, buf))
            });
        }
        m.add_method("set_geometry", |lua, this, o: Table| {
            with(lua, |st| mesh::set_geometry(st, this.0, &o))
        });
        m.add_method("vertex", |lua, this, i: usize| with(lua, |st| mesh::vertex(st, this.0, i)));
        m.add_method(
            "set_vertex",
            |lua, this, (i, x, y, u, v): (usize, f32, f32, Option<f32>, Option<f32>)| {
                with(lua, |st| mesh::set_vertex(st, this.0, i, (x, y), u.zip(v)))
            },
        );
        m.add_method("set_color", |lua, this, (i, c): (usize, LuaValue)| {
            with(lua, |st| mesh::set_color(st, this.0, i, &c))
        });
        m.add_method("vertex_at", |lua, this, (col, row): (u32, u32)| {
            with(lua, |st| mesh::grid_vertex(st, this.0, col, row))
        });
        m.add_method("children", |lua, this, ()| {
            let ids = with(lua, |st| Ok(node_of(st, this.0)?.children().to_vec()))?;
            lua.create_sequence_from(ids.into_iter().map(NodeRef))
        });
        m.add_method("add", |lua, this, child: AnyUserData| {
            let child = child.borrow::<NodeRef>()?.0;
            with(lua, |st| set_prop(st, child, "parent", NodeRef(this.0).into_lua(lua)?))
        });
    }
}

// ---------------------------------------------------------------------------------------
// Tweens

fn tween(lua: &Lua, node: NodeId, camera: bool, props: Table, opts: LuaValue) -> mlua::Result<TweenRef> {
    let mut targets = Vec::new();
    for pair in props.pairs::<String, LuaValue>() {
        let (key, v) = pair?;
        let prop = |p: Prop| -> mlua::Result<(Prop, f32)> { Ok((p, number(&v, &key)?)) };
        match (key.as_str(), camera) {
            ("x", false) => targets.push(prop(Prop::X)?),
            ("y", false) => targets.push(prop(Prop::Y)?),
            ("rotation", false) => targets.push(prop(Prop::Rotation)?),
            ("scale", false) => targets.extend([prop(Prop::ScaleX)?, prop(Prop::ScaleY)?]),
            ("scale_x", false) => targets.push(prop(Prop::ScaleX)?),
            ("scale_y", false) => targets.push(prop(Prop::ScaleY)?),
            ("opacity", false) => targets.push(prop(Prop::Opacity)?),
            ("tint", false) => {
                let c = color(&v, "tint")?;
                targets.extend([
                    (Prop::TintR, c.r),
                    (Prop::TintG, c.g),
                    (Prop::TintB, c.b),
                    (Prop::TintA, c.a),
                ]);
            }
            ("x", true) => targets.push(prop(Prop::CameraX)?),
            ("y", true) => targets.push(prop(Prop::CameraY)?),
            ("zoom", true) => targets.push(prop(Prop::CameraZoom)?),
            ("rotation", true) => targets.push(prop(Prop::CameraRotation)?),
            _ => {
                let what = if camera { "the camera" } else { "a node" };
                return Err(mlua::Error::runtime(format!("cannot tween `{key}` of {what}")));
            }
        }
    }
    let mut t = Tween::new(node, Vec::new(), 0.0);
    match &opts {
        LuaValue::Nil => {}
        LuaValue::Table(o) => {
            t.duration = o.get::<Option<f32>>("time")?.unwrap_or(0.0).max(0.0);
            t.delay = o.get::<Option<f32>>("delay")?.unwrap_or(0.0).max(0.0);
            t.yoyo = o.get::<Option<bool>>("yoyo")?.unwrap_or(false);
            t.repeat = match o.get::<Option<f64>>("repeat")? {
                Some(n) if n < 0.0 || n.is_infinite() => None,
                Some(n) => Some(n as u32),
                None => Some(0),
            };
            if let Some(name) = o.get::<Option<String>>("ease")? {
                t.ease = Ease::from_name(&name).ok_or_else(|| {
                    let names: Vec<&str> = Ease::NAMES.iter().map(|(n, _)| *n).collect();
                    mlua::Error::runtime(format!("unknown ease `{name}`; use one of: {}", names.join(", ")))
                })?;
            }
        }
        v => t.duration = number(v, "time")?.max(0.0),
    }
    t.props = targets.iter().map(|(p, to)| (*p, 0.0, *to)).collect();
    with(lua, |st| {
        if !camera {
            node_of(st, node)?;
        }
        // A new tween takes over the properties it animates.
        for (_, other) in &mut st.tweens {
            if other.node == node {
                other.props.retain(|(p, ..)| !targets.iter().any(|(q, _)| q == p));
            }
        }
        st.tweens.retain(|(_, other)| !other.props.is_empty());
        let id = st.next_id();
        st.tweens.push((id, t));
        Ok(TweenRef(id))
    })
}

// ---------------------------------------------------------------------------------------
// Camera and screen

impl UserData for CameraRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |_lua, _, key: String| {
            with(_lua, |st| {
                let c = &st.scene.camera;
                Ok(match key.as_str() {
                    "x" => c.position.x,
                    "y" => c.position.y,
                    "zoom" => c.zoom,
                    "rotation" => c.rotation,
                    "shake_x" => c.shake.x,
                    "shake_y" => c.shake.y,
                    _ => return Err(mlua::Error::runtime(format!("the camera has no property `{key}`"))),
                })
            })
        });
        m.add_meta_method(MetaMethod::NewIndex, |lua, _, (key, v): (String, LuaValue)| {
            let v = number(&v, &key)?;
            with(lua, |st| {
                let c = &mut st.scene.camera;
                match key.as_str() {
                    "x" => c.position.x = v,
                    "y" => c.position.y = v,
                    "zoom" => c.zoom = v,
                    "rotation" => c.rotation = v,
                    "shake_x" => c.shake.x = v,
                    "shake_y" => c.shake.y = v,
                    _ => return Err(mlua::Error::runtime(format!("the camera has no property `{key}`"))),
                }
                Ok(())
            })
        });
        m.add_method("tween", |lua, _, (props, opts): (Table, LuaValue)| {
            let root = with(lua, |st| Ok(st.scene.root()))?;
            tween(lua, root, true, props, opts)
        });
        m.add_method("reset", |lua, _, ()| {
            with(lua, |st| {
                st.scene.camera = Camera::new(st.viewport.design * 0.5);
                Ok(())
            })
        });
    }
}

impl UserData for ScreenRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |lua, _, key: String| {
            with(lua, |st| match key.as_str() {
                "width" => (st.viewport.design.x as f64).into_lua(lua),
                "height" => (st.viewport.design.y as f64).into_lua(lua),
                "mode" => name_of(MODES, st.viewport.mode).into_lua(lua),
                "scale" => (st.scale() as f64).into_lua(lua),
                "clear" => hex(st.scene.clear).into_lua(lua),
                "effects" => effects_table(lua, &st.scene.effects),
                "profile" => st.display.device.form.name().into_lua(lua),
                "orientation" => display::orientation(st).into_lua(lua),
                "mismatch" => display::mismatch(st).into_lua(lua),
                "touch" => st.display.device.touch.into_lua(lua),
                _ => Err(mlua::Error::runtime(format!("the screen has no property `{key}`"))),
            })
        });
        m.add_meta_method(MetaMethod::NewIndex, |lua, _, (key, v): (String, LuaValue)| {
            with(lua, |st| {
                match key.as_str() {
                    "clear" => st.scene.clear = color(&v, "clear")?,
                    "effects" => st.scene.effects = effects(st, &v)?,
                    _ => return Err(mlua::Error::runtime(format!("`screen.{key}` cannot be set"))),
                }
                Ok(())
            })
        });
        // Captures the screen for the next transition.
        m.add_method("freeze", |lua, _, ()| with(lua, |st| rt(media::freeze(st, false)).map(|_| ())));
        m.add_method("transition", |lua, _, opts: Table| {
            with(lua, |st| {
                let spec = transition_spec(st, &opts)?;
                rt(media::start_transition(st, spec)).map(TransitionRef)
            })
        });
        // capture(width, height, image?): the screen as an image (thumbnails); reuses `image`.
        m.add_method("capture", |lua, _, (w, h, reuse): (u32, u32, Option<AnyUserData>)| {
            let reuse = reuse.map(|u| u.borrow::<ImageRef>().map(|i| i.0)).transpose()?;
            with(lua, |st| rt(media::capture(st, w, h, reuse)).map(ImageRef))
        });
        // The visible area minus notches and system bars: x, y, width, height.
        m.add_method("safe", |lua, _, ()| {
            with(lua, |st| {
                let r = super::ui::areas(st).safe;
                Ok((r.min.x, r.min.y, r.size().x, r.size().y))
            })
        });
        // The design-space rectangle the window shows: x, y, width, height.
        m.add_method("visible", |lua, _, ()| {
            with(lua, |st| {
                let r = st.viewport.layout(st.renderer.size()).visible;
                Ok((r.min.x, r.min.y, r.size().x, r.size().y))
            })
        });
    }
}

// ---------------------------------------------------------------------------------------
// Nodes

/// Adds the node `make` builds under `opts.parent` (or the root), then applies the other
/// options as properties. `center = true` puts the origin in the middle.
fn create(
    lua: &Lua,
    opts: Option<Table>,
    consumed: &[&str],
    make: impl FnOnce(&mut State, &Table) -> mlua::Result<(Node, Option<TextNode>)>,
) -> mlua::Result<NodeRef> {
    let opts = match opts {
        Some(o) => o,
        None => lua.create_table()?,
    };
    let parent = match opts.get::<LuaValue>("parent")? {
        LuaValue::Nil => None,
        LuaValue::UserData(ud) => Some(ud.borrow::<NodeRef>()?.0),
        _ => return Err(mlua::Error::runtime("`parent` must be a node")),
    };
    let mut rest: Vec<(String, LuaValue)> = opts
        .pairs::<String, LuaValue>()
        .filter_map(Result::ok)
        .filter(|(k, _)| k != "parent" && k != "center" && !consumed.contains(&k.as_str()))
        .collect();
    // `params` and `mask` need the shader set first, `slice_scale` the slice.
    rest.sort_by_key(|(k, _)| match k.as_str() {
        "shader" | "slice" => 0,
        "params" | "mask" | "slice_scale" => 2,
        _ => 1,
    });
    let center = opts.get::<Option<bool>>("center")?.unwrap_or(false);
    with(lua, |st| {
        let parent = parent.unwrap_or(st.scene.root());
        node_of(st, parent)?;
        let (node, text) = make(st, &opts)?;
        let id = st.scene.add(parent, node);
        let result = (|| {
            if let Some(text) = text {
                st.texts.insert(id, text);
                rt(layout_text(st, id, true))?;
            }
            for (k, v) in rest {
                set_prop(st, id, &k, v)?;
            }
            if center {
                let size = node_size(st, id)?;
                node_mut(st, id)?.transform.origin = size * 0.5;
            }
            Ok(())
        })();
        if let Err(e) = result {
            st.scene.remove(id);
            st.texts.remove(&id);
            return Err(e);
        }
        Ok(NodeRef(id))
    })
}

fn sprite(st: &mut State, opts: &Table) -> mlua::Result<(Node, Option<TextNode>)> {
    let image = match opts.get::<LuaValue>("image")? {
        LuaValue::Nil => st.white,
        v => image_arg(st, &v)?,
    };
    let size = if image.id == st.white.id { Vec2::splat(100.0) } else { image.size.as_vec2() };
    Ok((Node::sprite(image.id, size), None))
}

fn text(_st: &mut State, opts: &Table) -> mlua::Result<(Node, Option<TextNode>)> {
    let mut style = TextStyle::default();
    if let Some(v) = opts.get::<Option<f32>>("size")? {
        style.size = v;
    }
    if let Some(v) = opts.get::<Option<f32>>("line_height")? {
        style.line_height = v;
    }
    style.width = opts.get::<Option<f32>>("wrap")?;
    style.font = opts.get::<Option<String>>("font")?;
    match opts.get::<LuaValue>("color")? {
        LuaValue::Nil => {}
        v => style.color = color(&v, "color")?,
    }
    match opts.get::<LuaValue>("align")? {
        LuaValue::Nil => {}
        v => style.align = named(ALIGNS, &v, "align")?,
    }
    let typewriter = Typewriter {
        cps: opts.get::<Option<f32>>("cps")?.unwrap_or(0.0),
        fade: opts.get::<Option<f32>>("fade")?.unwrap_or(Typewriter::default().fade),
    };
    let markup = match opts.get::<LuaValue>("text")? {
        LuaValue::Nil => String::new(),
        v => v.to_string()?,
    };
    let text = TextNode { markup, style, text: TextBox::new(Default::default(), typewriter) };
    Ok((Node::quads(Vec::new()), Some(text)))
}

const TEXT_OPTIONS: &[&str] =
    &["text", "size", "line_height", "wrap", "font", "color", "align", "cps", "fade"];

// ---------------------------------------------------------------------------------------
// Assets

pub(crate) fn load_image(st: &mut State, path: &str) -> Result<ImageInfo, String> {
    if let Some(info) = st.images.get(path) {
        return Ok(*info);
    }
    let bytes = st.assets.read(path)?;
    let image = Image::decode(&bytes).map_err(|e| format!("{path}: {e}"))?;
    let info =
        ImageInfo { id: st.renderer.create_texture(&image), size: UVec2::new(image.width, image.height) };
    st.images.insert(path.to_string(), info);
    Ok(info)
}

fn load_shader(st: &mut State, path: &str) -> Result<kulisa_core::ShaderId, String> {
    if let Some(id) = st.shaders.get(path) {
        return Ok(*id);
    }
    let source = st.assets.read_string(path)?;
    let id = st.renderer.create_shader(path, &source)?;
    st.shaders.insert(path.to_string(), id);
    Ok(id)
}

fn require(lua: &Lua, name: String) -> mlua::Result<LuaValue> {
    let cache: Table = match lua.named_registry_value::<Option<Table>>("kulisa.modules")? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            lua.set_named_registry_value("kulisa.modules", &t)?;
            t
        }
    };
    let path = if name.ends_with(".luau") { name.clone() } else { format!("{name}.luau") };
    match cache.raw_get::<LuaValue>(path.as_str())? {
        LuaValue::Nil => {}
        LuaValue::Boolean(false) => {
            return Err(mlua::Error::runtime(format!("`{path}` requires itself (through other modules)")));
        }
        v => return Ok(v),
    }
    let source = rt(with(lua, |st| Ok(st.assets.read_string(&path)))?)?;
    cache.raw_set(path.as_str(), false)?;
    let result = lua.load(&source).set_name(format!("@{path}")).call::<LuaValue>(());
    let value = match result {
        Ok(LuaValue::Nil) => LuaValue::Boolean(true),
        Ok(v) => v,
        Err(e) => {
            cache.raw_set(path.as_str(), LuaValue::Nil)?;
            return Err(e);
        }
    };
    cache.raw_set(path.as_str(), &value)?;
    Ok(value)
}

// ---------------------------------------------------------------------------------------
// Story, state and saves

fn story_files(st: &State, files: Option<Vec<String>>) -> Result<Vec<(String, String)>, String> {
    let files =
        files.unwrap_or_else(|| st.assets.list("").into_iter().filter(|f| f.ends_with(".kulisa")).collect());
    if files.is_empty() {
        return Err("the game has no .kulisa files".into());
    }
    files.into_iter().map(|f| st.assets.read_string(&f).map(|src| (f, src))).collect()
}

fn story_api(lua: &Lua) -> mlua::Result<Table> {
    let s = lua.create_table()?;
    s.set(
        "load",
        lua.create_function(|lua, files: Option<Vec<String>>| {
            let sources = rt(with(lua, |st| Ok(story_files(st, files)))?)?;
            let refs: Vec<(&str, &str)> = sources.iter().map(|(f, s)| (f.as_str(), s.as_str())).collect();
            let program = compile(&refs).map_err(|e| mlua::Error::runtime(e.to_string()))?;
            rt(with_story(&shared(lua), false, |s| {
                s.host.prepare(&program).map_err(|e| e.to_string())?;
                s.vm = Vm::new(Arc::new(program));
                s.loaded = true;
                s.waiting = Waiting::Nothing;
                Ok(())
            }))
        })?,
    )?;
    s.set(
        "start",
        lua.create_function(|lua, label: Option<String>| {
            let sh = shared(lua);
            let label = label.unwrap_or_else(|| "start".into());
            rt(with_story(&sh, true, |s| s.vm.start(&mut s.host, &label).map_err(|e| e.to_string())))?;
            rt(story_advance(lua, &sh))
        })?,
    )?;
    // Continues past the current line or pause. Returns false when the story waits for
    // something else (a menu choice, or nothing because it ended).
    s.set(
        "next",
        lua.create_function(|lua, ()| {
            let sh = shared(lua);
            let waiting = rt(with_story(&sh, true, |s| Ok(s.waiting)))?;
            if !matches!(waiting, Waiting::Line | Waiting::Pause) {
                return Ok(false);
            }
            rt(story_advance(lua, &sh)).map(|_| true)
        })?,
    )?;
    s.set(
        "choose",
        lua.create_function(|lua, index: usize| {
            let sh = shared(lua);
            let key = rt(with_story(&sh, true, |s| {
                let key = s.vm.line_key();
                s.vm.choose(&mut s.host, index.wrapping_sub(1)).map_err(|e| e.to_string())?;
                Ok(key)
            }))?;
            if let Some(k) = key {
                let mut st = sh.borrow_mut();
                st.seen_dirty |= st.seen.insert(choice_key(k, index.wrapping_sub(1)));
            }
            rt(story_advance(lua, &sh))
        })?,
    )?;
    // Whether the current line had been seen before (in any session).
    s.set("seen", lua.create_function(|lua, ()| with(lua, |st| Ok(st.line_seen)))?)?;
    s.set(
        "can_rollback",
        lua.create_function(|lua, ()| rt(with_story(&shared(lua), true, |s| Ok(s.vm.can_rollback()))))?,
    )?;
    s.set(
        "block_rollback",
        lua.create_function(|lua, ()| {
            rt(with_story(&shared(lua), true, |s| {
                s.vm.block_rollback();
                Ok(())
            }))
        })?,
    )?;
    s.set(
        "rollback",
        lua.create_function(|lua, ()| {
            let sh = shared(lua);
            let moved = rt(with_story(&sh, true, |s| s.vm.rollback(&mut s.host).map_err(|e| e.to_string())))?;
            if moved {
                rt(story_restored(lua, &sh))?;
            }
            Ok(moved)
        })?,
    )?;
    s.set(
        "roll_forward",
        lua.create_function(|lua, ()| {
            let sh = shared(lua);
            let moved =
                rt(with_story(&sh, true, |s| s.vm.roll_forward(&mut s.host).map_err(|e| e.to_string())))?;
            if moved {
                rt(story_restored(lua, &sh))?;
            }
            Ok(moved)
        })?,
    )?;
    s.set(
        "can_roll_forward",
        lua.create_function(|lua, ()| rt(with_story(&shared(lua), true, |s| Ok(s.vm.can_roll_forward()))))?,
    )?;
    s.set(
        "ended",
        lua.create_function(|lua, ()| rt(with_story(&shared(lua), true, |s| Ok(s.vm.is_ended()))))?,
    )?;
    s.set(
        "vars",
        lua.create_function(|lua, ()| rt(with_story(&shared(lua), false, |s| Ok(s.host.vars().clone()))))?,
    )?;
    s.set(
        "define",
        lua.create_function(|lua, (name, value): (String, LuaValue)| {
            rt(with_story(&shared(lua), false, |s| {
                s.host.define_value(&name, value).map_err(|e| e.to_string())
            }))
        })?,
    )?;
    s.set(
        "stage",
        lua.create_function(|lua, ()| {
            let stage = rt(with_story(&shared(lua), true, |s| Ok(s.vm.stage().to_value())))?;
            from_value(lua, &stage)
        })?,
    )?;
    Ok(s)
}

fn state_api(lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set(
        "register",
        lua.create_function(|lua, (name, hooks): (String, Table)| {
            let save: Function = hooks.get("save")?;
            let load: Function = hooks.get("load")?;
            rt(with_story(&shared(lua), false, |s| {
                s.host.register_state(&name, save, load).map_err(|e| e.to_string())
            }))
        })?,
    )?;
    Ok(t)
}

fn now_secs() -> u64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn storage(st: &State) -> mlua::Result<&Storage> {
    st.storage
        .as_ref()
        .ok_or_else(|| mlua::Error::runtime("call kulisa.save.setup{ id = \"my_game\" } first"))
}

fn save_api(lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set(
        "setup",
        lua.create_function(|lua, opts: Table| {
            let id: String = opts.get("id")?;
            let version = opts.get::<Option<u32>>("version")?.unwrap_or(1);
            let storage = rt(Storage::new(&id, "saves"))?;
            let persist = rt(Storage::new(&id, "persistent"))?;
            with(lua, |st| {
                st.game_id = id;
                st.storage = Some(storage);
                st.persist = Some(persist);
                st.save_version = version;
                st.load_seen();
                Ok(())
            })
        })?,
    )?;
    t.set(
        "migrate",
        lua.create_function(|lua, (from, f): (u32, Function)| {
            with(lua, |st| {
                st.migrations.push((from, f));
                Ok(())
            })
        })?,
    )?;
    // write(slot, { title = ?, data = ? }): saves the story (with registered state) and
    // optional extra data.
    t.set(
        "write",
        lua.create_function(|lua, (slot, opts): (String, Option<Table>)| {
            let sh = shared(lua);
            let (title, data, thumb) = match &opts {
                Some(o) => (
                    o.get::<Option<String>>("title")?,
                    o.get::<LuaValue>("data")?,
                    o.get::<Option<AnyUserData>>("thumbnail")?
                        .map(|u| u.borrow::<ImageRef>().map(|i| i.0.id))
                        .transpose()?,
                ),
                None => (None, LuaValue::Nil, None),
            };
            let data = rt(to_value(&data, "data", true, 0))?;
            let (story, last_line) = rt(with_story(&sh, false, |s| {
                let body = if s.loaded { s.vm.save().map_err(|e| e.to_string())? } else { Value::Nil };
                Ok((body, s.last_line.clone()))
            }))?;
            with(lua, |st| {
                storage(st)?;
                // Native platforms wait for the pixels; the web writes now and adds the
                // thumbnail when they arrive.
                let pixels = match thumb {
                    Some(id) => rt(media::capture_pixels(st, id, true))?,
                    None => None,
                };
                let thumbnail = pixels.map(|p| rt(p.encode_png())).transpose()?;
                if let (Some(id), None) = (thumb, &thumbnail) {
                    st.media.pending_saves.push((slot.clone(), id));
                }
                let header = SaveHeader {
                    game_id: st.game_id.clone(),
                    game_version: String::new(),
                    save_version: st.save_version,
                    timestamp: now_secs(),
                    slot: slot.clone(),
                    playtime: st.playtime,
                    title: title.unwrap_or_else(|| last_line.chars().take(80).collect()),
                    thumbnail,
                };
                let body = Value::map().with("story", story).with("data", data);
                rt(storage(st)?.write(&slot, &SaveFile { header, body }.to_bytes()))
            })
        })?,
    )?;
    // load(slot): restores the story and returns the save's extra data.
    t.set(
        "load",
        lua.create_function(|lua, slot: String| {
            let sh = shared(lua);
            let (bytes, version, steps) = with(lua, |st| {
                let bytes = rt(storage(st)?.read(&slot))?
                    .ok_or_else(|| mlua::Error::runtime(format!("no save in slot `{slot}`")))?;
                Ok((bytes, st.save_version, st.migrations.clone()))
            })?;
            let mut save = rt(SaveFile::from_bytes(&bytes))?;
            let mut migrations = Migrations::default();
            for (from, f) in steps {
                let lua = lua.clone();
                migrations.add(from, move |body| {
                    let arg = from_value(&lua, body).map_err(|e| e.to_string())?;
                    let out: LuaValue = f.call(arg.clone()).map_err(|e| e.to_string())?;
                    let out = if out.is_nil() { arg } else { out };
                    *body = to_value(&out, "save", true, 0)?;
                    Ok(())
                });
            }
            rt(migrations.apply(&mut save, version))?;
            sh.borrow_mut().playtime = save.header.playtime;
            let story = save.body.get("story").clone();
            if story != Value::Nil {
                let warnings =
                    rt(with_story(&sh, true, |s| s.vm.load(&mut s.host, &story).map_err(|e| e.to_string())))?;
                warnings.iter().for_each(|w| log::warn!("{w}"));
                rt(story_restored(lua, &sh))?;
            }
            from_value(lua, save.body.get("data"))
        })?,
    )?;
    t.set(
        "list",
        lua.create_function(|lua, ()| {
            let headers = with(lua, |st| {
                let storage = storage(st)?;
                Ok(storage
                    .list()
                    .into_iter()
                    .filter_map(|slot| {
                        let bytes = storage.read(&slot).ok().flatten()?;
                        SaveFile::read_header(&bytes).ok()
                    })
                    .collect::<Vec<_>>())
            })?;
            let list = lua.create_table()?;
            for h in headers {
                let t = lua.create_table()?;
                if let Some(png) = &h.thumbnail
                    && let Some(img) = with(lua, |st| Ok(media::thumbnail(st, &h.slot, h.timestamp, png)))?
                {
                    t.set("thumbnail", ImageRef(img))?;
                }
                t.set("slot", h.slot)?;
                t.set("title", h.title)?;
                t.set("timestamp", h.timestamp)?;
                t.set("playtime", h.playtime)?;
                t.set("version", h.save_version)?;
                list.push(t)?;
            }
            Ok(list)
        })?,
    )?;
    t.set(
        "exists",
        lua.create_function(|lua, slot: String| with(lua, |st| Ok(rt(storage(st)?.read(&slot))?.is_some())))?,
    )?;
    t.set(
        "delete",
        lua.create_function(|lua, slot: String| {
            with(lua, |st| {
                media::forget_thumbnail(st, &slot);
                rt(storage(st)?.delete(&slot))
            })
        })?,
    )?;
    Ok(t)
}

/// Rewrites saves whose thumbnail pixels arrived after they were written (web).
pub(crate) fn finish_pending_saves(shared: &Shared) {
    let mut st = shared.borrow_mut();
    let st = &mut *st;
    if st.media.pending_saves.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut st.media.pending_saves);
    for (slot, id) in pending {
        let pixels = match media::capture_pixels(st, id, false) {
            Ok(Some(p)) => p,
            Ok(None) => {
                st.media.pending_saves.push((slot, id));
                continue;
            }
            Err(e) => {
                log::warn!("thumbnail for `{slot}`: {e}");
                continue;
            }
        };
        let Some(storage) = &st.storage else { continue };
        let result = (|| {
            let bytes = storage.read(&slot)?.ok_or("the save is gone")?;
            let mut save = SaveFile::from_bytes(&bytes)?;
            save.header.thumbnail = Some(pixels.encode_png()?);
            storage.write(&slot, &save.to_bytes())
        })();
        if let Err(e) = result {
            log::warn!("thumbnail for `{slot}`: {e}");
        }
    }
}

/// `kulisa.persist`: plain data that outlives saves and rollback (settings, unlocks).
fn persist_api(lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set(
        "read",
        lua.create_function(|lua, name: String| {
            let v = with(lua, |st| {
                let p = st
                    .persist
                    .as_ref()
                    .ok_or_else(|| mlua::Error::runtime("call kulisa.save.setup first"))?;
                match rt(p.read(&name))? {
                    Some(bytes) => rt(Value::decode(&mut bytes.as_slice())),
                    None => Ok(Value::Nil),
                }
            })?;
            from_value(lua, &v)
        })?,
    )?;
    t.set(
        "write",
        lua.create_function(|lua, (name, data): (String, LuaValue)| {
            if name == "seen" {
                return Err(mlua::Error::runtime("`seen` is reserved for the engine"));
            }
            let v = rt(to_value(&data, &name, true, 0))?;
            with(lua, |st| {
                let p = st
                    .persist
                    .as_ref()
                    .ok_or_else(|| mlua::Error::runtime("call kulisa.save.setup first"))?;
                rt(p.write(&name, &v.to_bytes()))
            })
        })?,
    )?;
    Ok(t)
}

// ---------------------------------------------------------------------------------------
// Audio

fn audio_api(lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set(
        "play",
        lua.create_function(|lua, (channel, path, opts): (String, String, Option<Table>)| {
            let mut o = PlayOptions::default();
            if let Some(opts) = opts {
                o.looping = opts.get::<Option<bool>>("loop")?.unwrap_or(false);
                o.volume = opts.get::<Option<f32>>("volume")?.unwrap_or(1.0);
                o.fade_in = opts.get::<Option<f32>>("fade")?.unwrap_or(0.0);
                o.crossfade = opts.get::<Option<f32>>("crossfade")?.unwrap_or(0.0);
                o.rate = opts.get::<Option<f32>>("rate")?.unwrap_or(1.0);
                o.pan = opts.get::<Option<f32>>("pan")?.unwrap_or(0.0);
            }
            with(lua, |st| rt(st.play(&channel, &path, o)))
        })?,
    )?;
    t.set(
        "stop",
        lua.create_function(|lua, (channel, fade): (String, Option<f32>)| {
            with(lua, |st| rt(st.stop(&channel, fade.unwrap_or(0.0))))
        })?,
    )?;
    t.set(
        "playing",
        lua.create_function(|lua, channel: String| {
            with(lua, |st| Ok(st.audio.as_mut().is_some_and(|a| a.is_playing(&channel))))
        })?,
    )?;
    // volume(channel) reads, volume(channel, v, fade?) sets. Channel "master" is the mix.
    t.set(
        "volume",
        lua.create_function(|lua, (channel, v, fade): (String, Option<f32>, Option<f32>)| {
            with(lua, |st| {
                if let Some(v) = v {
                    st.volumes.insert(channel.clone(), v);
                    if let Some(a) = &mut st.audio {
                        if channel == "master" {
                            a.set_master_volume(v, fade.unwrap_or(0.0));
                        } else {
                            rt(a.set_volume(&channel, v, fade.unwrap_or(0.0)))?;
                        }
                    }
                }
                Ok(st.volumes.get(&channel).copied().unwrap_or(1.0))
            })
        })?,
    )?;
    Ok(t)
}

// ---------------------------------------------------------------------------------------

fn platform() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

fn join(args: Variadic<LuaValue>) -> mlua::Result<String> {
    let parts: Vec<String> = args.iter().map(|v| v.to_string()).collect::<mlua::Result<_>>()?;
    Ok(parts.join("\t"))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let k = lua.create_table()?;
    k.set("version", env!("CARGO_PKG_VERSION"))?;
    k.set("platform", platform())?;
    let log = lua.create_function(|_, args: Variadic<LuaValue>| {
        log::info!(target: "luau", "{}", join(args)?);
        Ok(())
    })?;
    k.set("log", &log)?;
    lua.globals().set("print", log)?;
    k.set(
        "_report",
        lua.create_function(|lua, msg: String| {
            report(&shared(lua), &msg);
            Ok(())
        })?,
    )?;
    lua.globals().set("require", lua.create_function(require)?)?;

    k.set("now", lua.create_function(|lua, ()| with(lua, |st| Ok(st.time)))?)?;
    k.set(
        "display",
        lua.create_function(|lua, (first, h, mode): (LuaValue, Option<f32>, Option<LuaValue>)| {
            if let LuaValue::Table(t) = &first {
                return with(lua, |st| display::configure(st, t, MODES));
            }
            let w = number(&first, "width")?;
            let h = h.ok_or_else(|| {
                mlua::Error::runtime("kulisa.display(width, height, mode?) or kulisa.display{ ... }")
            })?;
            let mode = match mode {
                None | Some(LuaValue::Nil) => ScaleMode::Expand,
                Some(v) => named(MODES, &v, "mode")?,
            };
            with(lua, |st| {
                display::apply(st, Viewport::new(w, h, mode), display::Orientation::Auto);
                Ok(())
            })
        })?,
    )?;
    k.set("window", WindowRef)?;
    k.set("camera", CameraRef)?;
    k.set("screen", ScreenRef)?;

    k.set(
        "group",
        lua.create_function(|lua, o: Option<Table>| create(lua, o, &[], |_, _| Ok((Node::group(), None))))?,
    )?;
    k.set("sprite", lua.create_function(|lua, o: Option<Table>| create(lua, o, &["image"], sprite))?)?;
    k.set("text", lua.create_function(|lua, o: Option<Table>| create(lua, o, TEXT_OPTIONS, text))?)?;
    k.set(
        "mesh",
        lua.create_function(|lua, o: Option<Table>| {
            create(lua, o, mesh::OPTIONS, |st, opts| Ok((Node::mesh(mesh::make(st, opts)?), None)))
        })?,
    )?;
    k.set(
        "layer",
        lua.create_function(|lua, o: Option<Table>| {
            create(lua, o, &[], |_, _| Ok((Node::layer(1.0), None)))
        })?,
    )?;
    k.set(
        "target",
        lua.create_function(|lua, o: Option<Table>| {
            create(lua, o, &["width", "height"], |st, opts| {
                let w: u32 = opts.get("width")?;
                let h: u32 = opts.get("height")?;
                let tex = st.renderer.create_target(w.max(1), h.max(1));
                Ok((Node::target(tex, UVec2::new(w.max(1), h.max(1))), None))
            })
        })?,
    )?;

    k.set(
        "load_image",
        lua.create_function(|lua, path: String| with(lua, |st| rt(load_image(st, &path)).map(ImageRef)))?,
    )?;
    k.set(
        "load_shader",
        lua.create_function(|lua, path: String| with(lua, |st| rt(load_shader(st, &path)).map(ShaderRef)))?,
    )?;
    k.set(
        "load_font",
        lua.create_function(|lua, path: String| {
            with(lua, |st| {
                let bytes = rt(st.assets.read(&path))?;
                let families = st.text.load_font(bytes);
                if families.is_empty() {
                    return Err(mlua::Error::runtime(format!("`{path}` holds no usable font")));
                }
                Ok(families)
            })
        })?,
    )?;
    k.set(
        "read_text",
        lua.create_function(|lua, path: String| with(lua, |st| rt(st.assets.read_string(&path))))?,
    )?;
    k.set("exists", lua.create_function(|lua, path: String| with(lua, |st| Ok(st.assets.exists(&path))))?)?;
    k.set("list", lua.create_function(|lua, dir: String| with(lua, |st| Ok(st.assets.list(&dir))))?)?;

    k.set(
        "on",
        lua.create_function(|lua, (event, f): (String, Function)| {
            if !EVENTS.contains(&event.as_str()) {
                return Err(mlua::Error::runtime(format!(
                    "unknown event `{event}`; events are: {}",
                    EVENTS.join(", ")
                )));
            }
            with(lua, |st| {
                let id = st.next_id();
                st.handlers.entry(event).or_default().push((id, f));
                Ok(id)
            })
        })?,
    )?;
    k.set(
        "off",
        lua.create_function(|lua, id: u64| {
            with(lua, |st| {
                st.handlers.values_mut().for_each(|h| h.retain(|(i, _)| *i != id));
                Ok(())
            })
        })?,
    )?;
    // kulisa.pick(x, y, { root, alpha, filter }) -> the topmost node under the point.
    // kulisa.pick_all(x, y, { root, alpha }) -> every node there, topmost first.
    let pick_args = |opts: &Option<Table>| -> mlua::Result<(Option<NodeId>, Option<f32>)> {
        let Some(o) = opts else { return Ok((None, None)) };
        let root = match o.get::<LuaValue>("root")? {
            LuaValue::Nil => None,
            LuaValue::UserData(ud) => Some(ud.borrow::<NodeRef>()?.0),
            _ => return Err(mlua::Error::runtime("`root` must be a node")),
        };
        Ok((root, hit::threshold(&o.get::<LuaValue>("alpha")?)?))
    };
    k.set(
        "pick_all",
        lua.create_function(move |lua, (x, y, opts): (f32, f32, Option<Table>)| {
            let (root, alpha) = pick_args(&opts)?;
            let ids = with(lua, |st| Ok(hit::pick_all(st, Vec2::new(x, y), root, alpha)))?;
            lua.create_sequence_from(ids.into_iter().map(NodeRef))
        })?,
    )?;
    k.set(
        "pick",
        lua.create_function(move |lua, (x, y, opts): (f32, f32, Option<Table>)| {
            let (root, alpha) = pick_args(&opts)?;
            let filter = opts.as_ref().map(|o| o.get::<Option<Function>>("filter")).transpose()?.flatten();
            let ids = with(lua, |st| Ok(hit::pick_all(st, Vec2::new(x, y), root, alpha)))?;
            for id in ids {
                // The filter runs Luau, so the state is not borrowed here.
                if filter.as_ref().is_none_or(|f| f.call::<bool>(NodeRef(id)).unwrap_or(false)) {
                    return Ok(Some(NodeRef(id)));
                }
            }
            Ok(None)
        })?,
    )?;
    let input = lua.create_table()?;
    input.set(
        "pointer",
        lua.create_function(|lua, ()| with(lua, |st| Ok(st.pointer.map(|p| (p.x, p.y)).unzip())))?,
    )?;
    input.set("down", lua.create_function(|lua, key: String| with(lua, |st| Ok(st.keys.contains(&key))))?)?;
    // kulisa.input.pointers() -> { { id, x, y, touch, down }, ... }: the mouse and every touch.
    input.set(
        "pointers",
        lua.create_function(|lua, ()| {
            let list = with(lua, |st| {
                Ok(st
                    .pointers
                    .iter()
                    .filter_map(|(id, p)| p.pos.map(|pos| (*id, pos, p.touch, p.down)))
                    .collect::<Vec<_>>())
            })?;
            let out = lua.create_table()?;
            for (id, pos, touch, down) in list {
                let t = lua.create_table()?;
                t.set("id", id)?;
                t.set("x", pos.x)?;
                t.set("y", pos.y)?;
                t.set("touch", touch)?;
                t.set("down", down)?;
                out.push(t)?;
            }
            Ok(out)
        })?,
    )?;
    super::pad::install(lua, &input)?;
    super::typing::install(lua, &input)?;
    super::sensors::install(lua, &k)?;
    k.set("input", input)?;
    super::interact::install(lua, &k)?;

    k.set("audio", audio_api(lua)?)?;
    k.set("story", story_api(lua)?)?;
    k.set("state", state_api(lua)?)?;
    k.set("save", save_api(lua)?)?;
    k.set("persist", persist_api(lua)?)?;
    k.set("ui", super::ui::api(lua)?)?;
    lua.globals().set("kulisa", k)
}
