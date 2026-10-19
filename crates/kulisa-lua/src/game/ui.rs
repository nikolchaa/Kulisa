//! `kulisa.ui`: declarative screens. Luau screen functions return widget tables; they are
//! converted into `kulisa_ui` elements, reconciled, laid out and kept in sync with scene
//! nodes (a layer per screen, a group per widget).

use std::collections::HashMap;

use kulisa_core::{Color, Node, NodeId, NodeKind, Rect, Slice, Vec2};
use kulisa_render::Renderer;
use kulisa_text::{Align as TextAlignment, TextBox, TextStyle, TextSystem, Typewriter};
use kulisa_ui::{
    Align, Anchor, Area, Areas, Deps, Element, Fill, Gestures, Justify, Kind, Layout, Look, Measure, Sides,
    Size, SliderSpec, State as WState, TextAlign, TextLook, Ui, UiEvent, Widget,
};
use mlua::{
    AnyUserData, Function, Lua, MetaMethod, Table, UserData, UserDataMethods, Value as LuaValue, Variadic,
};

use super::api::{NodeRef, color, image_arg, layout_text, number, rt, shared, with};
use super::{Shared, State, TextNode, report};

/// The scene nodes of one widget.
#[derive(Debug, Default)]
pub(crate) struct UiNodes {
    group: Option<NodeId>,
    bg: Option<NodeId>,
    /// The text node or image sprite.
    content: Option<NodeId>,
    bar: Option<NodeId>,
    thumb: Option<NodeId>,
    /// A game node shown by a `node` widget (detached, not removed, with the widget).
    embedded: Option<NodeId>,
    /// What the text node was last set to: look, color, wrap width.
    text: Option<(TextLook, Color, f32)>,
}

pub(crate) struct ScreenDef {
    build: Function,
    z: i32,
    modal: bool,
    area: Area,
}

pub(crate) struct UiState {
    pub ui: Ui<UiNodes>,
    pub defs: HashMap<String, ScreenDef>,
    pub props: HashMap<String, LuaValue>,
    /// Per screen: the callbacks and embedded nodes of its last build.
    pub callbacks: HashMap<String, Vec<Function>>,
    pub embeds: HashMap<String, Vec<NodeId>>,
    pub layers: HashMap<String, NodeId>,
    pub deps: Deps,
    pub gestures: Gestures,
    pub next_signal: u64,
    /// The current press went to the UI.
    pub pressed_by_ui: bool,
    /// Insets of the window in pixels (left, top, right, bottom): notches, system bars.
    pub safe_px: [f32; 4],
}

impl Default for UiState {
    fn default() -> Self {
        let r = Rect::new(0.0, 0.0, 1.0, 1.0);
        Self {
            ui: Ui::new(Areas { screen: r, safe: r, design: r }),
            defs: HashMap::new(),
            props: HashMap::new(),
            callbacks: HashMap::new(),
            embeds: HashMap::new(),
            layers: HashMap::new(),
            deps: Deps::default(),
            gestures: Gestures::default(),
            next_signal: 0,
            pressed_by_ui: false,
            safe_px: [0.0; 4],
        }
    }
}

// ---------------------------------------------------------------------------------------
// Signals

pub(crate) struct SignalRef(pub u64);

fn values(lua: &Lua) -> mlua::Result<Table> {
    if let Some(t) = lua.named_registry_value::<Option<Table>>("kulisa.ui.values")? {
        return Ok(t);
    }
    let t = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set("__mode", "k")?;
    t.set_metatable(Some(mt))?;
    lua.set_named_registry_value("kulisa.ui.values", &t)?;
    Ok(t)
}

fn signal_get(lua: &Lua, ud: &AnyUserData, track: bool) -> mlua::Result<LuaValue> {
    let id = ud.borrow::<SignalRef>()?.0;
    if track {
        with(lua, |st| {
            st.ui.deps.read(id);
            Ok(())
        })?;
    }
    values(lua)?.raw_get(ud)
}

impl UserData for SignalRef {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_function(MetaMethod::Call, |lua, (ud, _): (AnyUserData, Variadic<LuaValue>)| {
            signal_get(lua, &ud, true)
        });
        m.add_function("get", |lua, ud: AnyUserData| signal_get(lua, &ud, true));
        m.add_function("peek", |lua, ud: AnyUserData| signal_get(lua, &ud, false));
        m.add_function("set", |lua, (ud, v): (AnyUserData, LuaValue)| {
            let id = ud.borrow::<SignalRef>()?.0;
            let vals = values(lua)?;
            let old: LuaValue = vals.raw_get(&ud)?;
            let same = !matches!(v, LuaValue::Table(_)) && old == v;
            vals.raw_set(&ud, v)?;
            if !same {
                with(lua, |st| {
                    st.ui.deps.changed(id);
                    Ok(())
                })?;
            }
            Ok(())
        });
        m.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(format!("signal {}", this.0)));
    }
}

/// A prop value, reading signals (which subscribes the screen being built).
fn read(st: &mut State, values: &Table, v: LuaValue) -> mlua::Result<LuaValue> {
    if let LuaValue::UserData(ud) = &v
        && let Ok(s) = ud.borrow::<SignalRef>()
    {
        st.ui.deps.read(s.0);
        return values.raw_get(ud);
    }
    Ok(v)
}

// ---------------------------------------------------------------------------------------
// Widget tables to elements

struct Convert<'a> {
    st: &'a mut State,
    values: Table,
    callbacks: Vec<Function>,
    embeds: Vec<NodeId>,
}

fn bad(what: &str, expected: &str) -> mlua::Error {
    mlua::Error::runtime(format!("`{what}` must be {expected}"))
}

fn size(v: &LuaValue, what: &str, grow: f32) -> mlua::Result<Size> {
    Ok(match v {
        LuaValue::Nil => Size::Auto,
        LuaValue::String(s) => {
            let s = s.to_string_lossy();
            match s.as_str() {
                "auto" => Size::Auto,
                "fill" => Size::Fill(grow),
                _ => match s.strip_suffix('%').and_then(|p| p.trim().parse::<f32>().ok()) {
                    Some(p) => Size::Percent(p / 100.0),
                    None => return Err(bad(what, "a number, \"50%\", \"fill\" or \"auto\"")),
                },
            }
        }
        v => Size::Px(number(v, what)?),
    })
}

fn sides(v: &LuaValue, what: &str) -> mlua::Result<Sides> {
    Ok(match v {
        LuaValue::Nil => Sides::default(),
        LuaValue::Table(t) => {
            let n = |i: usize| -> mlua::Result<f32> { number(&t.raw_get(i)?, what) };
            match t.raw_len() {
                2 => {
                    let (v, h) = (n(1)?, n(2)?);
                    Sides { left: h, top: v, right: h, bottom: v }
                }
                4 => Sides { top: n(1)?, right: n(2)?, bottom: n(3)?, left: n(4)? },
                _ => {
                    return Err(bad(
                        what,
                        "a number, { vertical, horizontal } or { top, right, bottom, left }",
                    ));
                }
            }
        }
        v => Sides::all(number(v, what)?),
    })
}

fn opt_num(v: &LuaValue, what: &str) -> mlua::Result<Option<f32>> {
    if v.is_nil() { Ok(None) } else { number(v, what).map(Some) }
}

fn string(v: &LuaValue) -> mlua::Result<String> {
    Ok(match v {
        LuaValue::String(s) => s.to_string_lossy(),
        LuaValue::Nil => String::new(),
        v => v.to_string()?,
    })
}

impl Convert<'_> {
    fn get(&mut self, t: &Table, key: &str) -> mlua::Result<LuaValue> {
        let v = t.raw_get(key)?;
        read(self.st, &self.values, v)
    }

    fn fill(&mut self, v: &LuaValue, what: &str) -> mlua::Result<Option<Fill>> {
        Ok(match v {
            LuaValue::Nil | LuaValue::Boolean(false) => None,
            LuaValue::String(s) if s.to_string_lossy().starts_with('#') => Some(Fill::Color(color(v, what)?)),
            // { image = ..., tint = ... }: a tinted image.
            LuaValue::Table(t) if t.contains_key("image")? => {
                let img = image_arg(self.st, &t.get::<LuaValue>("image")?)?;
                let tint = match t.get::<LuaValue>("tint")? {
                    LuaValue::Nil => Color::WHITE,
                    v => color(&v, what)?,
                };
                Some(Fill::Image { texture: img.id, size: img.size.as_vec2(), tint })
            }
            LuaValue::Table(_) => Some(Fill::Color(color(v, what)?)),
            v => {
                let img = image_arg(self.st, v)?;
                Some(Fill::Image { texture: img.id, size: img.size.as_vec2(), tint: Color::WHITE })
            }
        })
    }

    /// The look props with `prefix` ("" or "hover_" etc.).
    fn look(&mut self, t: &Table, prefix: &str) -> mlua::Result<Look> {
        let k = |name: &str| format!("{prefix}{name}");
        let fill = |this: &mut Self, name: &str| -> mlua::Result<Option<Fill>> {
            let v = this.get(t, &k(name))?;
            this.fill(&v, &k(name))
        };
        let (background, image, bar, thumb) =
            (fill(self, "background")?, fill(self, "image")?, fill(self, "bar")?, fill(self, "thumb")?);
        let tint = match self.get(t, &k("tint"))? {
            LuaValue::Nil => None,
            v => Some(color(&v, &k("tint"))?),
        };
        let color = match self.get(t, &k("color"))? {
            LuaValue::Nil => None,
            v => Some(color(&v, &k("color"))?),
        };
        let opacity = opt_num(&self.get(t, &k("opacity"))?, &k("opacity"))?;
        let slice = match self.get(t, &k("slice"))? {
            LuaValue::Nil => None,
            LuaValue::Table(s) => {
                let n = |i: usize| -> mlua::Result<f32> { number(&s.raw_get(i)?, "slice") };
                Some([n(1)?, n(2)?, n(3)?, n(4)?])
            }
            v => Some([number(&v, "slice")?; 4]),
        };
        let slice_scale = opt_num(&self.get(t, &k("slice_scale"))?, &k("slice_scale"))?;
        let l = Look { background, slice, slice_scale, tint, opacity, color, image, bar, thumb };
        Ok(l)
    }

    fn element(&mut self, t: &Table) -> mlua::Result<Option<Element>> {
        if self.get(t, "visible")? == LuaValue::Boolean(false) {
            return Ok(None);
        }
        let kind_name = string(&t.raw_get::<LuaValue>("_kind")?)?;
        let (kind, layout) = match kind_name.as_str() {
            "box" => (Kind::Box, Layout::Overlay),
            "button" => (Kind::Box, Layout::Overlay),
            "row" => (Kind::Box, Layout::Stack { horizontal: true }),
            "column" => (Kind::Box, Layout::Stack { horizontal: false }),
            "grid" => (Kind::Box, Layout::Grid { columns: 1 }),
            "text" => (Kind::Text, Layout::Overlay),
            "image" => (Kind::Image, Layout::Overlay),
            "slider" => (Kind::Slider, Layout::Overlay),
            "scroll" => (Kind::Scroll, Layout::Stack { horizontal: false }),
            "node" => (Kind::Node, Layout::Overlay),
            "" => {
                return Err(mlua::Error::runtime("widgets are made with kulisa.ui.box, .text, .button, ..."));
            }
            k => return Err(mlua::Error::runtime(format!("unknown widget kind `{k}`"))),
        };
        let mut e = Element::new(kind);
        e.button = kind_name == "button";
        e.key = match self.get(t, "key")? {
            LuaValue::Nil => None,
            v => Some(string(&v)?),
        };
        e.id = match self.get(t, "id")? {
            LuaValue::Nil => None,
            v => Some(string(&v)?),
        };

        // Layout.
        let s = &mut e.style;
        let grow = opt_num(&t.raw_get("grow")?, "grow")?.unwrap_or(1.0);
        s.width = size(&self.get(t, "width")?, "width", grow)?;
        s.height = size(&self.get(t, "height")?, "height", grow)?;
        let s = &mut e.style;
        s.min = Vec2::new(
            opt_num(&t.raw_get("min_width")?, "min_width")?.unwrap_or(0.0),
            opt_num(&t.raw_get("min_height")?, "min_height")?.unwrap_or(0.0),
        );
        let (mw, mh) = (
            opt_num(&t.raw_get("max_width")?, "max_width")?,
            opt_num(&t.raw_get("max_height")?, "max_height")?,
        );
        if mw.is_some() || mh.is_some() {
            s.max = Some(Vec2::new(mw.unwrap_or(f32::INFINITY), mh.unwrap_or(f32::INFINITY)));
        }
        s.padding = sides(&t.raw_get("padding")?, "padding")?;
        s.margin = sides(&t.raw_get("margin")?, "margin")?;
        s.gap = opt_num(&t.raw_get("gap")?, "gap")?.unwrap_or(0.0);
        s.layout = match (layout, t.raw_get::<Option<u32>>("columns")?) {
            (Layout::Grid { .. }, c) => Layout::Grid { columns: c.unwrap_or(2) },
            (l, _) => l,
        };
        if kind == Kind::Box
            && let LuaValue::String(l) = t.raw_get::<LuaValue>("layout")?
        {
            s.layout = match l.to_string_lossy().as_str() {
                "row" => Layout::Stack { horizontal: true },
                "column" => Layout::Stack { horizontal: false },
                "overlay" => Layout::Overlay,
                _ => return Err(bad("layout", "\"row\", \"column\" or \"overlay\"")),
            };
        }
        if kind != Kind::Text {
            s.align = match string(&t.raw_get("align")?)?.as_str() {
                "" | "start" => Align::Start,
                "center" => Align::Center,
                "end" => Align::End,
                "stretch" => Align::Stretch,
                _ => return Err(bad("align", "\"start\", \"center\", \"end\" or \"stretch\"")),
            };
        }
        s.justify = match string(&t.raw_get("justify")?)?.as_str() {
            "" | "start" => Justify::Start,
            "center" => Justify::Center,
            "end" => Justify::End,
            "between" => Justify::Between,
            _ => return Err(bad("justify", "\"start\", \"center\", \"end\" or \"between\"")),
        };
        s.anchor = match t.raw_get::<LuaValue>("anchor")? {
            LuaValue::Nil => Anchor::default(),
            LuaValue::Table(a) => {
                Anchor(Vec2::new(number(&a.raw_get(1)?, "anchor")?, number(&a.raw_get(2)?, "anchor")?))
            }
            v => {
                let name = string(&v)?;
                Anchor::named(&name)
                    .ok_or_else(|| bad("anchor", "a name like \"bottom\" or \"top_right\", or { x, y }"))?
            }
        };
        let (x, y) = (self.get(t, "x")?, self.get(t, "y")?);
        e.style.offset = Vec2::new(opt_num(&x, "x")?.unwrap_or(0.0), opt_num(&y, "y")?.unwrap_or(0.0));

        // Looks.
        e.look = self.look(t, "")?;
        e.hover = self.look(t, "hover_")?;
        e.pressed = self.look(t, "pressed_")?;
        e.selected_look = self.look(t, "selected_")?;
        e.disabled_look = self.look(t, "disabled_")?;
        e.selected = self.get(t, "selected")?.as_boolean().unwrap_or(false);
        e.disabled = self.get(t, "disabled")?.as_boolean().unwrap_or(false);
        let callback = |this: &mut Self, key: &str| -> mlua::Result<Option<u64>> {
            match t.raw_get::<LuaValue>(key)? {
                LuaValue::Nil => Ok(None),
                LuaValue::Function(f) => {
                    this.callbacks.push(f);
                    Ok(Some(this.callbacks.len() as u64 - 1))
                }
                _ => Err(bad(key, "a function")),
            }
        };
        e.on_click = callback(self, "on_click")?;
        e.on_change = callback(self, "on_change")?;
        if e.on_click.is_some() {
            e.button = true;
        }

        match kind {
            Kind::Text => {
                let mut tl = TextLook { markup: string(&self.get(t, "text")?)?, ..Default::default() };
                if let Some(v) = opt_num(&self.get(t, "size")?, "size")? {
                    tl.size = v;
                }
                if let Some(v) = opt_num(&t.raw_get("line_height")?, "line_height")? {
                    tl.line_height = v;
                }
                tl.font = match self.get(t, "font")? {
                    LuaValue::Nil => None,
                    v => Some(string(&v)?),
                };
                tl.align = match string(&t.raw_get("align")?)?.as_str() {
                    "" | "left" => TextAlign::Left,
                    "center" => TextAlign::Center,
                    "right" => TextAlign::Right,
                    "justified" => TextAlign::Justified,
                    _ => {
                        return Err(bad(
                            "align",
                            "\"left\", \"center\", \"right\" or \"justified\" for text",
                        ));
                    }
                };
                tl.cps = opt_num(&self.get(t, "cps")?, "cps")?.unwrap_or(0.0).max(0.0);
                tl.fade =
                    opt_num(&self.get(t, "fade")?, "fade")?.unwrap_or(Typewriter::default().fade).max(0.0);
                e.text = Some(tl);
            }
            Kind::Slider => {
                let min = opt_num(&t.raw_get("min")?, "min")?.unwrap_or(0.0);
                let max = opt_num(&t.raw_get("max")?, "max")?.unwrap_or(1.0);
                e.slider = Some(SliderSpec {
                    value: opt_num(&self.get(t, "value")?, "value")?.unwrap_or(min),
                    min,
                    max,
                    step: opt_num(&t.raw_get("step")?, "step")?.unwrap_or(0.0).max(0.0),
                    vertical: t.raw_get::<Option<bool>>("vertical")?.unwrap_or(false),
                    thumb_size: opt_num(&t.raw_get("thumb_size")?, "thumb_size")?.unwrap_or(32.0),
                });
            }
            Kind::Scroll => {
                e.horizontal = string(&t.raw_get("direction")?)? == "horizontal";
                e.start_at_end = string(&t.raw_get("start")?)? == "end";
            }
            Kind::Node => {
                let ud = t.raw_get::<AnyUserData>("node").map_err(|_| bad("node", "a node"))?;
                let id = ud.borrow::<NodeRef>()?.0;
                let n = self.st.scene.get(id).ok_or_else(|| mlua::Error::runtime("`node` was removed"))?;
                e.node_size = match &n.kind {
                    NodeKind::Sprite(s) => s.size,
                    NodeKind::Target(t) => t.size.as_vec2(),
                    _ => self.st.texts.get(&id).map_or(Vec2::ZERO, |t| t.text.layout.size),
                };
                self.embeds.push(id);
                e.node = Some(self.embeds.len() as u64 - 1);
            }
            _ => {}
        }

        // Children: the table's list part, in order; nested lists are flattened, false and
        // nil entries skipped.
        let mut items: Vec<(i64, LuaValue)> = Vec::new();
        for pair in t.clone().pairs::<LuaValue, LuaValue>() {
            let (k, v) = pair?;
            if let LuaValue::Integer(i) = k {
                // mlua's Luau integer is i32 or i64 depending on the target.
                #[allow(clippy::unnecessary_cast)]
                items.push((i as i64, v));
            } else if let LuaValue::Number(n) = k
                && n.fract() == 0.0
            {
                items.push((n as i64, v));
            }
        }
        items.sort_by_key(|(i, _)| *i);
        for (_, v) in items {
            self.children(v, &mut e.children)?;
        }
        Ok(Some(e))
    }

    fn children(&mut self, v: LuaValue, out: &mut Vec<Element>) -> mlua::Result<()> {
        let LuaValue::Table(t) = v else { return Ok(()) };
        if t.raw_get::<LuaValue>("_kind")?.is_nil() {
            for item in t.sequence_values::<LuaValue>() {
                self.children(item?, out)?;
            }
            return Ok(());
        }
        out.extend(self.element(&t)?);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// Building, layout and scene sync

/// Measures text for layout with the game's fonts.
struct Meter<'a> {
    text: &'a mut TextSystem,
    renderer: &'a mut Renderer,
    scale: f32,
    cache: HashMap<String, Vec2>,
}

fn text_style(t: &TextLook, color: Color, width: Option<f32>) -> TextStyle {
    TextStyle {
        font: t.font.clone(),
        size: t.size,
        line_height: t.line_height,
        color,
        width,
        align: match t.align {
            TextAlign::Left => TextAlignment::Left,
            TextAlign::Center => TextAlignment::Center,
            TextAlign::Right => TextAlignment::Right,
            TextAlign::Justified => TextAlignment::Justified,
        },
    }
}

impl Measure for Meter<'_> {
    fn text(&mut self, t: &TextLook, max_width: Option<f32>) -> Vec2 {
        let key =
            format!("{}\u{1}{}\u{1}{:?}\u{1}{}\u{1}{:?}", t.markup, t.size, t.font, t.line_height, max_width);
        if let Some(s) = self.cache.get(&key) {
            return *s;
        }
        let rich = match kulisa_text::parse(&t.markup) {
            Ok(r) => r,
            Err(_) => return Vec2::ZERO,
        };
        // Wrapping at exactly a text's own width can break it on rounding: allow a hair more.
        let style = text_style(t, Color::WHITE, max_width.map(|w| w + 1.0));
        let mut s = self.text.layout(self.renderer, &rich, &style, self.scale).size;
        if let Some(w) = max_width {
            s.x = s.x.min(w);
        }
        self.cache.insert(key, s);
        s
    }
}

pub(crate) fn areas(st: &State) -> Areas {
    let size = st.renderer.size();
    let layout = st.viewport.layout(size);
    let screen = layout.visible;
    let inv = layout.to_pixels.inverse();
    let [l, t, r, b] = st.ui.safe_px;
    let a = inv.transform_point2(Vec2::new(l, t));
    let z = inv.transform_point2(size.as_vec2() - Vec2::new(r, b));
    let safe = Rect { min: screen.min.max(a), max: screen.max.min(z) };
    let safe = if safe.is_empty() { screen } else { safe };
    Areas { screen, safe, design: Rect { min: Vec2::ZERO, max: st.viewport.design } }
}

/// Removes the nodes of dropped widgets.
fn free(st: &mut State, removed: Vec<UiNodes>) {
    let root = st.scene.root();
    for n in removed {
        if let Some(e) = n.embedded
            && let Some(node) = st.scene.get_mut(e)
        {
            node.visible = false;
            st.scene.reparent(e, root);
        }
        if let Some(g) = n.group {
            st.scene.remove(g);
        }
    }
}

/// A converted tree with its callbacks and embedded nodes.
type Built = (Element, Vec<Function>, Vec<NodeId>);

/// Runs a screen's function and reconciles its tree.
fn rebuild(lua: &Lua, shared: &Shared, name: &str) -> Result<(), String> {
    let (build, props) = {
        let st = shared.borrow();
        let Some(def) = st.ui.defs.get(name) else { return Ok(()) };
        (def.build.clone(), st.ui.props.get(name).cloned().unwrap_or(LuaValue::Nil))
    };
    shared.borrow_mut().ui.deps.begin(name);
    let result = (|| -> mlua::Result<Option<Built>> {
        let v: LuaValue = build.call(props)?;
        let LuaValue::Table(t) = v else {
            return Err(mlua::Error::runtime("a screen function must return a widget (kulisa.ui.box{...})"));
        };
        let values = values(lua)?;
        let mut st = shared.borrow_mut();
        let mut c = Convert { st: &mut st, values, callbacks: Vec::new(), embeds: Vec::new() };
        Ok(c.element(&t)?.map(|e| (e, c.callbacks, c.embeds)))
    })();
    let mut st = shared.borrow_mut();
    st.ui.deps.end();
    let (root, callbacks, embeds) = match result {
        Ok(Some(r)) => r,
        Ok(None) => (Element::new(Kind::Box), Vec::new(), Vec::new()),
        Err(e) => return Err(format!("in screen `{name}`: {e}")),
    };
    st.ui.callbacks.insert(name.to_string(), callbacks);
    st.ui.embeds.insert(name.to_string(), embeds);
    let removed = st.ui.ui.set_tree(name, root);
    free(&mut st, removed);
    st.ui.ui.needs_layout = true;
    Ok(())
}

/// Rebuilds dirty screens, lays out and syncs nodes. Called every frame and by `flush`.
pub(crate) fn update(lua: &Lua, shared: &Shared) {
    let dirty = {
        let mut st = shared.borrow_mut();
        let a = areas(&st);
        if a != st.ui.ui.areas {
            st.ui.ui.areas = a;
            st.ui.ui.needs_layout = true;
        }
        let shown: Vec<String> = st.ui.ui.screens.iter().map(|s| s.name.clone()).collect();
        let mut d = st.ui.deps.take_dirty();
        d.retain(|n| shown.contains(n));
        d
    };
    for name in dirty {
        if let Err(e) = rebuild(lua, shared, &name) {
            report(shared, &e);
        }
    }
    let mut st = shared.borrow_mut();
    let st = &mut *st;
    if !st.ui.ui.needs_layout && !st.ui.ui.changed {
        return;
    }
    if st.ui.ui.needs_layout {
        let scale = st.scale();
        let State { ui, text, renderer, .. } = st;
        let mut meter = Meter { text, renderer, scale, cache: HashMap::new() };
        ui.ui.layout(&mut meter);
    }
    st.ui.ui.changed = false;
    let mut screens = std::mem::take(&mut st.ui.ui.screens);
    for s in &mut screens {
        let Some(&layer) = st.ui.layers.get(&s.name) else { continue };
        if let Some(l) = st.scene.get_mut(layer) {
            l.z = s.z;
        }
        let embeds = st.ui.embeds.get(&s.name).cloned().unwrap_or_default();
        if let Some(root) = &mut s.root {
            sync(st, root, layer, Vec2::ZERO, 0, WState::default(), &embeds);
        }
    }
    st.ui.ui.screens = screens;
}

fn ensure(st: &mut State, slot: &mut Option<NodeId>, parent: NodeId, make: impl FnOnce() -> Node) -> NodeId {
    match *slot {
        Some(id) if st.scene.contains(id) => id,
        _ => {
            let id = st.scene.add(parent, make());
            *slot = Some(id);
            id
        }
    }
}

fn drop_node(st: &mut State, slot: &mut Option<NodeId>) {
    if let Some(id) = slot.take() {
        st.scene.remove(id);
    }
}

/// Shows `fill` with a sprite node filling `rect` (local units).
fn paint(
    st: &mut State,
    slot: &mut Option<NodeId>,
    parent: NodeId,
    fill: Option<Fill>,
    rect: Rect,
    look: &Look,
    z: i32,
) {
    let Some(fill) = fill else { return drop_node(st, slot) };
    let white = st.white;
    let id = ensure(st, slot, parent, || Node::sprite(white.id, Vec2::ONE));
    let tint = look.tint.unwrap_or(Color::WHITE);
    let (texture, source, color) = match fill {
        Fill::Color(c) => (white.id, white.size.as_vec2(), c * tint),
        Fill::Image { texture, size, tint: own } => (texture, size, own * tint),
    };
    let n = &mut st.scene[id];
    n.z = z;
    n.transform.position = rect.min;
    n.tint = color;
    if let NodeKind::Sprite(s) = &mut n.kind {
        s.texture = texture;
        s.size = rect.size();
        s.slice = match (fill, look.slice) {
            (Fill::Image { .. }, Some([left, top, right, bottom])) => {
                Some(Slice { left, top, right, bottom, source, scale: look.slice_scale.unwrap_or(1.0) })
            }
            _ => None,
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn sync(
    st: &mut State,
    w: &mut Widget<UiNodes>,
    parent: NodeId,
    origin: Vec2,
    z: i32,
    inherited: WState,
    embeds: &[NodeId],
) {
    let group = ensure(st, &mut w.host.group, parent, Node::group);
    let state = if w.el.button {
        WState {
            hover: w.state.hover,
            pressed: w.state.pressed,
            selected: w.el.selected,
            disabled: w.el.disabled,
        }
    } else {
        WState {
            selected: inherited.selected || w.el.selected,
            disabled: inherited.disabled || w.el.disabled,
            ..inherited
        }
    };
    let look = w.look(state);
    let size = w.rect.size();
    {
        let n = &mut st.scene[group];
        n.transform.position = w.rect.min - origin;
        n.z = z;
        n.opacity = look.opacity.unwrap_or(1.0);
        n.clip = w.clip.map(|c| Rect { min: c.min - w.rect.min, max: c.max - w.rect.min });
    }
    let local = Rect { min: Vec2::ZERO, max: size };
    paint(st, &mut w.host.bg, group, look.background, local, &look, i32::MIN / 2);
    let pad = w.el.style.padding;
    let inner = Rect { min: pad.start(), max: (size - Vec2::new(pad.right, pad.bottom)).max(pad.start()) };
    match w.el.kind {
        Kind::Text => sync_text(st, w, group, inner, &look),
        Kind::Image => {
            let mut content = w.host.content;
            paint(st, &mut content, group, look.image, inner, &look, 0);
            w.host.content = content;
        }
        Kind::Slider => {
            let spec = w.el.slider.clone().unwrap_or_default();
            let value = w.state.drag_value.unwrap_or(spec.value);
            let f = spec.fraction(value);
            let t = spec.thumb_size;
            let (bar, thumb) = if spec.vertical {
                let y = local.max.y - t * 0.5 - (local.size().y - t) * f;
                (
                    Rect { min: Vec2::new(0.0, y), max: local.max },
                    Rect::new(local.size().x * 0.5 - t * 0.5, y - t * 0.5, t, t),
                )
            } else {
                let x = t * 0.5 + (local.size().x - t) * f;
                (
                    Rect { min: Vec2::ZERO, max: Vec2::new(x, local.max.y) },
                    Rect::new(x - t * 0.5, local.size().y * 0.5 - t * 0.5, t, t),
                )
            };
            let (mut b, mut th) = (w.host.bar, w.host.thumb);
            // The widget's tint is the track's; bars and thumbs carry their own.
            paint(st, &mut b, group, look.bar, bar, &Look { tint: None, ..look.clone() }, 1);
            paint(
                st,
                &mut th,
                group,
                look.thumb,
                thumb,
                &Look { tint: None, slice: None, ..look.clone() },
                2,
            );
            (w.host.bar, w.host.thumb) = (b, th);
        }
        Kind::Node => {
            if let Some(&id) = w.el.node.and_then(|i| embeds.get(i as usize)) {
                if w.host.embedded != Some(id) {
                    if let Some(old) = w.host.embedded.take()
                        && let Some(n) = st.scene.get_mut(old)
                    {
                        n.visible = false;
                    }
                    st.scene.reparent(id, group);
                    w.host.embedded = Some(id);
                }
                if let Some(n) = st.scene.get_mut(id) {
                    n.visible = true;
                    n.transform.position = inner.min;
                }
            }
        }
        _ => {}
    }
    let origin = w.rect.min;
    for (i, c) in w.children.iter_mut().enumerate() {
        sync(st, c, group, origin, i as i32, state, embeds);
    }
}

fn sync_text(st: &mut State, w: &mut Widget<UiNodes>, group: NodeId, inner: Rect, look: &Look) {
    let Some(t) = w.el.text.clone() else { return };
    let color = look.color.unwrap_or(Color::WHITE);
    // A hair wider than measured, so text sized to fit never wraps on rounding.
    let wrap = inner.size().x + 1.0;
    let id = match w.host.content {
        Some(id) if st.scene.contains(id) => id,
        _ => {
            let id = st.scene.add(group, Node::quads(Vec::new()));
            let text = TextBox::new(Default::default(), Typewriter { cps: t.cps, fade: t.fade });
            st.texts.insert(id, TextNode { markup: String::new(), style: TextStyle::default(), text });
            w.host.content = Some(id);
            w.host.text = None;
            id
        }
    };
    st.scene[id].transform.position = inner.min;
    let last = w.host.text.clone();
    if last.as_ref() == Some(&(t.clone(), color, wrap)) {
        return;
    }
    let restart = last.as_ref().is_none_or(|(l, ..)| l.markup != t.markup);
    if let Some(node) = st.texts.get_mut(&id) {
        node.markup = t.markup.clone();
        node.style = text_style(&t, color, Some(wrap));
        node.text.typewriter = Typewriter { cps: t.cps, fade: t.fade };
    }
    if let Err(e) = layout_text(st, id, restart) {
        log::warn!("{e}");
        if let Some(node) = st.texts.get_mut(&id) {
            node.markup = t.markup.replace('{', "{{").replace('}', "}}");
        }
        let _ = layout_text(st, id, restart);
    }
    w.host.text = Some((t, color, wrap));
}

/// Marks every shown screen for rebuilding (after story steps: they may read story state).
pub(crate) fn refresh_all(st: &mut State) {
    let names: Vec<String> = st.ui.ui.screens.iter().map(|s| s.name.clone()).collect();
    for n in names {
        st.ui.deps.mark(&n);
    }
}

/// Runs the callbacks for UI events.
pub(crate) fn run_events(shared: &Shared, events: Vec<UiEvent>) {
    for ev in events {
        let (screen, cb, value) = match ev {
            UiEvent::Click { screen, callback } => (screen, callback, None),
            UiEvent::Change { screen, callback, value } => (screen, callback, Some(value)),
        };
        let f = shared.borrow().ui.callbacks.get(&screen).and_then(|c| c.get(cb as usize)).cloned();
        if let Some(f) = f {
            let r = match value {
                Some(v) => f.call::<()>(v),
                None => f.call::<()>(()),
            };
            if let Err(e) = r {
                report(shared, &format!("in screen `{screen}`: {e}"));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// The `kulisa.ui` table

fn area(v: &LuaValue) -> mlua::Result<Area> {
    Ok(match string(v)?.as_str() {
        "" | "safe" => Area::Safe,
        "screen" => Area::Screen,
        "design" => Area::Design,
        _ => return Err(bad("area", "\"safe\", \"screen\" or \"design\"")),
    })
}

fn hide(st: &mut State, name: &str) {
    let removed = st.ui.ui.hide(name);
    free(st, removed);
    if let Some(l) = st.ui.layers.remove(name) {
        st.scene.remove(l);
    }
    st.ui.deps.forget(name);
    st.ui.callbacks.remove(name);
    st.ui.embeds.remove(name);
    st.ui.props.remove(name);
}

fn find(st: &State, screen: &str, id: &str) -> Option<(NodeId, Rect)> {
    let w = st.ui.ui.screen(screen)?.root.as_ref()?.find(id)?;
    let node = w.host.content.or(w.host.embedded).or(w.host.group)?;
    Some((node, w.rect))
}

pub(crate) fn api(lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set(
        "signal",
        lua.create_function(|lua, v: LuaValue| {
            let id = with(lua, |st| {
                st.ui.next_signal += 1;
                Ok(st.ui.next_signal)
            })?;
            let ud = lua.create_userdata(SignalRef(id))?;
            values(lua)?.raw_set(&ud, v)?;
            Ok(ud)
        })?,
    )?;
    t.set(
        "screen",
        lua.create_function(|lua, (name, build, opts): (String, Function, Option<Table>)| {
            let (z, modal, a) = match &opts {
                Some(o) => (
                    o.get::<Option<i32>>("z")?.unwrap_or(100),
                    o.get::<Option<bool>>("modal")?.unwrap_or(false),
                    area(&o.get::<LuaValue>("area")?)?,
                ),
                None => (100, false, Area::Safe),
            };
            with(lua, |st| {
                st.ui.defs.insert(name.clone(), ScreenDef { build, z, modal, area: a });
                if st.ui.ui.screen(&name).is_some() {
                    st.ui.ui.show(&name, z, modal, a);
                    st.ui.deps.mark(&name);
                }
                Ok(())
            })
        })?,
    )?;
    t.set(
        "show",
        lua.create_function(|lua, (name, props): (String, LuaValue)| {
            let sh = shared(lua);
            with(lua, |st| {
                let def = st.ui.defs.get(&name).ok_or_else(|| {
                    mlua::Error::runtime(format!(
                        "there is no screen `{name}`; define it with kulisa.ui.screen"
                    ))
                })?;
                let (z, modal, a) = (def.z, def.modal, def.area);
                st.ui.ui.show(&name, z, modal, a);
                if !st.ui.layers.contains_key(&name) {
                    let mut layer = Node::layer(0.0);
                    layer.z = z;
                    let id = st.scene.add(st.scene.root(), layer);
                    st.ui.layers.insert(name.clone(), id);
                }
                st.ui.props.insert(name.clone(), props);
                Ok(())
            })?;
            rt(rebuild(lua, &sh, &name))?;
            update(lua, &sh);
            Ok(())
        })?,
    )?;
    t.set(
        "hide",
        lua.create_function(|lua, name: String| {
            with(lua, |st| {
                hide(st, &name);
                Ok(())
            })
        })?,
    )?;
    t.set(
        "shown",
        lua.create_function(|lua, name: String| with(lua, |st| Ok(st.ui.ui.screen(&name).is_some())))?,
    )?;
    t.set(
        "refresh",
        lua.create_function(|lua, name: Option<String>| {
            with(lua, |st| {
                match name {
                    Some(n) => st.ui.deps.mark(&n),
                    None => refresh_all(st),
                }
                Ok(())
            })
        })?,
    )?;
    t.set(
        "flush",
        lua.create_function(|lua, ()| {
            update(lua, &shared(lua));
            Ok(())
        })?,
    )?;
    t.set(
        "find",
        lua.create_function(|lua, (screen, id): (String, String)| {
            with(lua, |st| Ok(find(st, &screen, &id).map(|(n, _)| NodeRef(n))))
        })?,
    )?;
    t.set(
        "bounds",
        lua.create_function(|lua, (screen, id): (String, String)| {
            with(lua, |st| {
                Ok(find(st, &screen, &id)
                    .map(|(_, r)| (r.min.x, r.min.y, r.size().x, r.size().y))
                    .unzip_tuple())
            })
        })?,
    )?;
    t.set(
        "layer",
        lua.create_function(|lua, name: String| {
            with(lua, |st| Ok(st.ui.layers.get(&name).map(|&l| NodeRef(l))))
        })?,
    )?;
    Ok(t)
}

trait UnzipTuple {
    fn unzip_tuple(self) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>);
}

impl UnzipTuple for Option<(f32, f32, f32, f32)> {
    fn unzip_tuple(self) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        match self {
            Some((a, b, c, d)) => (Some(a), Some(b), Some(c), Some(d)),
            None => (None, None, None, None),
        }
    }
}
