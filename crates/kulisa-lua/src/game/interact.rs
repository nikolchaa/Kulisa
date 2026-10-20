//! Scene interaction: draggable nodes, drop zones and hotspots, built on picking.
//!
//! The engine only decides *what* is grabbed, hovered or dropped; what happens then is the
//! game's, through callbacks (`on_start`, `on_drop`, `on_click`, ...). Callbacks run after the
//! state borrow ends, like every other handler.

use std::collections::{BTreeMap, HashMap};

use kulisa_core::{Ease, NodeId, Prop, Rect, Tween, Vec2};
use mlua::{
    AnyUserData, Function, IntoLua, Lua, MetaMethod, Table, UserData, UserDataMethods, Value as LuaValue,
};

use super::api::{NodeRef, node_size, number, with};
use super::{Shared, State, hit, report};

/// Added to a lifted node's `z` while it is dragged.
const LIFT: i32 = 10_000;
/// Distance (design units) a press must travel before a drag counts as moved rather than a click.
const SLOP: f32 = 6.0;

#[derive(Clone, Debug)]
enum Shape {
    /// The node's drawn area, or any of its descendants'.
    Area,
    /// A rectangle in the node's local space.
    Rect(Rect),
    /// A polygon in the node's local space.
    Polygon(Vec<Vec2>),
}

#[derive(Clone, Debug)]
struct Area {
    node: NodeId,
    shape: Shape,
    /// Pixel opacity the point must reach (areas only).
    alpha: Option<f32>,
    enabled: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Home {
    Rejected,
    Always,
    Never,
}

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    Both,
    X,
    Y,
}

#[derive(Clone, Copy, PartialEq)]
enum By {
    Pointer,
    Center,
}

#[derive(Clone)]
enum Accept {
    Any,
    Tags(Vec<String>),
    Func(Function),
}

#[derive(Clone, Copy)]
enum Snap {
    Center,
    Offset(Vec2),
    Off,
}

type Callbacks = HashMap<&'static str, Function>;

struct Draggable {
    id: u64,
    area: Area,
    tag: Option<String>,
    bounds: Option<Rect>,
    axis: Axis,
    lift: bool,
    home: Home,
    snap_time: f32,
    on: Callbacks,
}

struct Zone {
    id: u64,
    area: Area,
    accept: Accept,
    snap: Snap,
    by: By,
    capacity: Option<usize>,
    occupants: Vec<NodeId>,
    on: Callbacks,
}

struct Spot {
    id: u64,
    area: Area,
    priority: i32,
    block: bool,
    cursor: Option<&'static str>,
    on: Callbacks,
}

struct Active {
    drag: u64,
    node: NodeId,
    /// Node position minus the pointer, both in the parent's space.
    offset: Vec2,
    start: Vec2,
    down: Vec2,
    moved: bool,
    over: Option<u64>,
}

impl Interact {
    /// The cursor of the hotspot under the mouse, if it asks for one.
    pub(crate) fn hover_cursor(&self) -> Option<&'static str> {
        let h = self.hover?;
        self.spots.iter().find(|s| s.id == h && s.area.enabled).and_then(|s| s.cursor)
    }
}

#[derive(Default)]
pub(crate) struct Interact {
    next: u64,
    drags: Vec<Draggable>,
    zones: Vec<Zone>,
    spots: Vec<Spot>,
    active: BTreeMap<u32, Active>,
    /// The hotspot each pointer pressed.
    pressing: BTreeMap<u32, u64>,
    hover: Option<u64>,
}

impl Interact {
    fn idle(&self) -> bool {
        self.drags.is_empty() && self.spots.is_empty() && self.active.is_empty()
    }

    /// Forgets everything attached to nodes that no longer exist.
    fn purge(&mut self, st_scene: &kulisa_core::Scene) {
        let alive = |n: NodeId| st_scene.contains(n);
        self.drags.retain(|d| alive(d.area.node));
        self.zones.retain(|z| alive(z.area.node));
        self.spots.retain(|s| alive(s.area.node));
        for z in &mut self.zones {
            z.occupants.retain(|&n| alive(n));
        }
        let drags = &self.drags;
        self.active.retain(|_, a| alive(a.node) && drags.iter().any(|d| d.id == a.drag));
        let spots = &self.spots;
        self.pressing.retain(|_, s| spots.iter().any(|x| x.id == *s));
        if self.hover.is_some_and(|h| !spots.iter().any(|x| x.id == h)) {
            self.hover = None;
        }
    }
}

// ---------------------------------------------------------------------------------------
// Geometry

fn under(scene: &kulisa_core::Scene, mut n: NodeId, root: NodeId) -> bool {
    loop {
        if n == root {
            return true;
        }
        match scene.get(n).and_then(|x| x.parent()) {
            Some(p) => n = p,
            None => return false,
        }
    }
}

fn in_polygon(poly: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Where `area` is hit by design-space `p`: the node whose paint position ranks it (the
/// topmost hit drawn node for areas, the node itself for shapes). `picked` is the shape-only
/// pick list at `p`, topmost first.
fn area_hit(st: &State, a: &Area, p: Vec2, picked: &[NodeId]) -> Option<NodeId> {
    if !a.enabled || !st.scene.contains(a.node) {
        return None;
    }
    match &a.shape {
        Shape::Area => picked.iter().copied().find(|&n| {
            under(&st.scene, n, a.node) && a.alpha.is_none_or(|t| hit::contains(st, n, p, Some(t)))
        }),
        shape => {
            let local = st.scene.to_design(a.node)?.inverse().transform_point2(p);
            let inside = match shape {
                Shape::Rect(r) => r.contains(local),
                Shape::Polygon(poly) => in_polygon(poly, local),
                Shape::Area => unreachable!(),
            };
            inside.then_some(a.node)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Cand {
    Drag(u64),
    Spot(u64),
}

/// The topmost enabled draggable or hotspot at `p` (hotspots only when `drags` is false).
fn topmost(st: &State, p: Vec2, drags: bool) -> Option<Cand> {
    let it = &st.interact;
    if (!drags || it.drags.is_empty()) && it.spots.is_empty() {
        return None;
    }
    let picked = hit::pick_all(st, p, None, None);
    let paint = st.scene.paint_order();
    let rank = |a: &Area| area_hit(st, a, p, &picked).and_then(|n| paint.get(&n).copied());
    let mut best: Option<(i32, usize, Cand)> = None;
    let mut consider = |priority: i32, r: Option<usize>, c: Cand| {
        if let Some(r) = r
            && best.is_none_or(|(bp, br, _)| (priority, r) > (bp, br))
        {
            best = Some((priority, r, c));
        }
    };
    if drags {
        for d in &it.drags {
            consider(0, rank(&d.area), Cand::Drag(d.id));
        }
    }
    for s in &it.spots {
        consider(s.priority, rank(&s.area), Cand::Spot(s.id));
    }
    best.map(|(_, _, c)| c)
}

/// Zones that test with `by` at design-space `at`, with their paint rank. The dragged node's
/// own subtree never counts as a zone's area.
fn zones_at(st: &State, at: Vec2, dragged: NodeId, by: By) -> Vec<(usize, u64)> {
    if !st.interact.zones.iter().any(|z| z.by == by) {
        return Vec::new();
    }
    let picked: Vec<NodeId> =
        hit::pick_all(st, at, None, None).into_iter().filter(|&n| !under(&st.scene, n, dragged)).collect();
    let paint = st.scene.paint_order();
    st.interact
        .zones
        .iter()
        .filter(|z| z.by == by)
        .filter_map(|z| {
            let n = area_hit(st, &z.area, at, &picked)?;
            Some((paint.get(&n).copied()?, z.id))
        })
        .collect()
}

/// The zones an item is over, topmost first: each zone is tested with the point its `by` names
/// (the pointer, or the item's center).
fn zones_under(st: &State, dragged: NodeId, pointer: Vec2) -> Vec<u64> {
    let mut all = zones_at(st, pointer, dragged, By::Pointer);
    if let Some(c) = center_design(st, dragged) {
        all.extend(zones_at(st, c, dragged, By::Center));
    }
    all.sort_by_key(|&(r, _)| std::cmp::Reverse(r));
    all.into_iter().map(|(_, id)| id).collect()
}

/// The dragged node's center in design space.
fn center_design(st: &State, node: NodeId) -> Option<Vec2> {
    let c = node_size(st, node).ok()? * 0.5;
    Some(st.scene.to_design(node)?.transform_point2(c))
}

fn parent_local(st: &State, node: NodeId, p: Vec2) -> Option<Vec2> {
    let parent = st.scene.get(node)?.parent()?;
    Some(st.scene.to_design(parent)?.inverse().transform_point2(p))
}

fn move_to(st: &mut State, node: NodeId, pos: Vec2, secs: f32) {
    if secs <= 0.0 {
        if let Some(n) = st.scene.get_mut(node) {
            n.transform.position = pos;
        }
        return;
    }
    let mut t = Tween::new(node, vec![(Prop::X, pos.x), (Prop::Y, pos.y)], secs);
    t.ease = Ease::OutCubic;
    take_over(st, node);
    let id = st.next_id();
    st.tweens.push((id, t));
}

/// Stops tweens of a node's position (a drag takes over).
fn take_over(st: &mut State, node: NodeId) {
    for (_, t) in &mut st.tweens {
        if t.node == node {
            t.props.retain(|(p, ..)| !matches!(p, Prop::X | Prop::Y));
        }
    }
    st.tweens.retain(|(_, t)| !t.props.is_empty());
}

// ---------------------------------------------------------------------------------------
// Callbacks

#[derive(Clone, Copy)]
struct Info {
    node: NodeId,
    zone: Option<NodeId>,
    p: Vec2,
    pointer: u32,
    from: Vec2,
    moved: bool,
    accepted: bool,
}

impl Info {
    /// Callback info for a pointer at `p` over `node`, with nothing dropped anywhere.
    fn at(node: NodeId, p: Vec2, pointer: u32) -> Info {
        Info { node, zone: None, p, pointer, from: p, moved: false, accepted: false }
    }
}
type Calls = Vec<(Function, Info)>;

fn info_table(lua: &Lua, i: &Info) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("node", NodeRef(i.node))?;
    if let Some(z) = i.zone {
        t.set("zone", NodeRef(z))?;
    }
    t.set("x", i.p.x)?;
    t.set("y", i.p.y)?;
    t.set("pointer", i.pointer)?;
    t.set("from_x", i.from.x)?;
    t.set("from_y", i.from.y)?;
    t.set("moved", i.moved)?;
    t.set("accepted", i.accepted)?;
    Ok(t)
}

fn fire(lua: &Lua, shared: &Shared, calls: Calls) {
    for (f, info) in calls {
        let result = info_table(lua, &info).and_then(|t| f.call::<()>(t));
        if let Err(e) = result {
            report(shared, &format!("in an interaction callback: {e}"));
        }
    }
}

fn call(calls: &mut Calls, on: &Callbacks, name: &str, info: Info) {
    if let Some(f) = on.get(name) {
        calls.push((f.clone(), info));
    }
}

// ---------------------------------------------------------------------------------------
// Pointer input

/// A press at `p` by pointer `id`. Returns whether a draggable or hotspot took it.
pub(crate) fn down(lua: &Lua, shared: &Shared, id: u32, p: Vec2) -> bool {
    let mut calls = Vec::new();
    let taken = {
        let mut st = shared.borrow_mut();
        let st = &mut *st;
        st.interact.purge(&st.scene);
        if st.interact.idle() {
            return false;
        }
        match topmost(st, p, true) {
            Some(Cand::Drag(d)) => start_drag(st, id, d, p, &mut calls),
            Some(Cand::Spot(s)) => {
                let spot = st.interact.spots.iter().find(|x| x.id == s).expect("hotspot exists");
                let (node, block) = (spot.area.node, spot.block);
                call(&mut calls, &spot.on, "press", Info::at(node, p, id));
                st.interact.pressing.insert(id, s);
                block
            }
            None => false,
        }
    };
    fire(lua, shared, calls);
    taken
}

fn start_drag(st: &mut State, id: u32, drag: u64, p: Vec2, calls: &mut Calls) -> bool {
    let Some(d) = st.interact.drags.iter().find(|d| d.id == drag) else { return false };
    let (node, lift, on) = (d.area.node, d.lift, d.on.clone());
    let Some(pl) = parent_local(st, node, p) else { return false };
    let Some(n) = st.scene.get_mut(node) else { return false };
    let start = n.transform.position;
    if lift {
        n.z += LIFT;
    }
    take_over(st, node);
    for z in &mut st.interact.zones {
        z.occupants.retain(|&o| o != node);
    }
    st.interact
        .active
        .insert(id, Active { drag, node, offset: start - pl, start, down: p, moved: false, over: None });
    call(calls, &on, "start", Info { from: start, ..Info::at(node, p, id) });
    true
}

/// Pointer `id` moved to `p`: drags follow it, and zones under the item report enter/leave.
pub(crate) fn moved(lua: &Lua, shared: &Shared, id: u32, p: Vec2) {
    let mut calls = Vec::new();
    {
        let mut st = shared.borrow_mut();
        drag_to(&mut st, id, p, &mut calls);
    }
    fire(lua, shared, calls);
}

fn drag_to(st: &mut State, id: u32, p: Vec2, calls: &mut Calls) {
    let Some(a) = st.interact.active.get(&id) else { return };
    let (node, offset, start, drag, over_before) = (a.node, a.offset, a.start, a.drag, a.over);
    let Some(d) = st.interact.drags.iter().find(|d| d.id == drag) else { return };
    let (axis, bounds, on) = (d.axis, d.bounds, d.on.clone());
    let Some(pl) = parent_local(st, node, p) else { return };
    let mut pos = pl + offset;
    match axis {
        Axis::X => pos.y = start.y,
        Axis::Y => pos.x = start.x,
        Axis::Both => {}
    }
    if let Some(b) = bounds {
        pos = pos.clamp(b.min, b.max);
    }
    if let Some(n) = st.scene.get_mut(node) {
        n.transform.position = pos;
    }
    let a = st.interact.active.get_mut(&id).expect("checked above");
    a.moved |= (p - a.down).length() > SLOP;
    let moved = a.moved;
    let info = Info { from: start, moved, ..Info::at(node, p, id) };
    call(calls, &on, "move", info);
    if !moved {
        return;
    }
    let over = zones_under(st, node, p).first().copied();
    if over == over_before {
        return;
    }
    let zone = |z: Option<u64>| z.and_then(|z| st.interact.zones.iter().find(|x| x.id == z));
    if let Some(old) = zone(over_before) {
        call(calls, &old.on, "leave", Info { zone: Some(old.area.node), ..info });
    }
    if let Some(new) = zone(over) {
        call(calls, &new.on, "enter", Info { zone: Some(new.area.node), ..info });
    }
    st.interact.active.get_mut(&id).expect("checked above").over = over;
}

/// What a zone says about an item.
enum Verdict {
    Yes,
    No,
    /// Ask the zone's `accept` function.
    Ask(Function),
}

/// A release that needs Luau to decide (zone `accept` functions) before it can be applied.
struct Pending {
    active: Active,
    candidates: Vec<(u64, NodeId, Verdict)>,
    info: Info,
}

enum Release {
    Done { taken: bool, calls: Calls },
    Decide(Box<Pending>, Calls),
}

/// Pointer `id` went up at `p`. Returns whether a draggable or hotspot took it.
pub(crate) fn up(lua: &Lua, shared: &Shared, id: u32, p: Vec2) -> bool {
    let release = {
        let mut st = shared.borrow_mut();
        release(&mut st, id, p)
    };
    let (pending, mut calls) = match release {
        Release::Done { taken, calls } => {
            fire(lua, shared, calls);
            return taken;
        }
        Release::Decide(pending, calls) => (pending, calls),
    };
    // The accept functions are Luau: ask them with the state free.
    let (mut chosen, mut refused) = (None, None);
    for (z, zone_node, verdict) in &pending.candidates {
        let ok = match verdict {
            Verdict::Yes => true,
            Verdict::No => false,
            Verdict::Ask(f) => {
                let probe = Info { zone: Some(*zone_node), ..pending.info };
                match info_table(lua, &probe).and_then(|t| f.call::<bool>(t)) {
                    Ok(b) => b,
                    Err(e) => {
                        report(shared, &format!("in a drop zone's `accept`: {e}"));
                        false
                    }
                }
            }
        };
        if ok {
            chosen = Some((*z, *zone_node));
            break;
        }
        refused.get_or_insert((*z, *zone_node));
    }
    finish_drop(&mut shared.borrow_mut(), &pending, chosen, refused, &mut calls);
    fire(lua, shared, calls);
    true
}

fn release(st: &mut State, id: u32, p: Vec2) -> Release {
    let mut calls = Vec::new();
    st.interact.purge(&st.scene);
    if let Some(spot) = st.interact.pressing.remove(&id) {
        let still = topmost(st, p, true) == Some(Cand::Spot(spot));
        let Some(s) = st.interact.spots.iter().find(|s| s.id == spot) else {
            return Release::Done { taken: false, calls };
        };
        let info = Info::at(s.area.node, p, id);
        call(&mut calls, &s.on, "release", info);
        if still {
            call(&mut calls, &s.on, "click", info);
        }
        return Release::Done { taken: s.block, calls };
    }
    let Some(active) = st.interact.active.remove(&id) else {
        return Release::Done { taken: false, calls };
    };
    let Some(d) = st.interact.drags.iter().find(|d| d.id == active.drag) else {
        return Release::Done { taken: true, calls };
    };
    let (on, lift, tag) = (d.on.clone(), d.lift, d.tag.clone());
    if lift && let Some(n) = st.scene.get_mut(active.node) {
        n.z -= LIFT;
    }
    let info = Info { from: active.start, moved: active.moved, ..Info::at(active.node, p, id) };
    if !active.moved {
        call(&mut calls, &on, "click", info);
        call(&mut calls, &on, "end", info);
        return Release::Done { taken: true, calls };
    }
    if let Some(z) = active.over.and_then(|z| st.interact.zones.iter().find(|x| x.id == z)) {
        call(&mut calls, &z.on, "leave", Info { zone: Some(z.area.node), ..info });
    }
    let candidates = zones_under(st, active.node, p)
        .into_iter()
        .filter_map(|z| st.interact.zones.iter().find(|x| x.id == z))
        .map(|zone| {
            let full = zone.capacity.is_some_and(|c| zone.occupants.len() >= c);
            let verdict = match &zone.accept {
                _ if full => Verdict::No,
                Accept::Any => Verdict::Yes,
                Accept::Tags(tags) if tag.as_ref().is_some_and(|t| tags.contains(t)) => Verdict::Yes,
                Accept::Tags(_) => Verdict::No,
                Accept::Func(f) => Verdict::Ask(f.clone()),
            };
            (zone.id, zone.area.node, verdict)
        })
        .collect();
    Release::Decide(Box::new(Pending { active, candidates, info }), calls)
}

fn finish_drop(
    st: &mut State,
    pending: &Pending,
    chosen: Option<(u64, NodeId)>,
    refused: Option<(u64, NodeId)>,
    calls: &mut Calls,
) {
    let (a, info) = (&pending.active, pending.info);
    let Some(d) = st.interact.drags.iter().find(|d| d.id == a.drag) else { return };
    let (on, home, snap_time) = (d.on.clone(), d.home, d.snap_time);
    if let Some((zid, zone_node)) = chosen {
        let info = Info { zone: Some(zone_node), accepted: true, ..info };
        let zone = st.interact.zones.iter_mut().find(|z| z.id == zid).expect("zone exists");
        zone.occupants.push(a.node);
        let (snap, zone_on) = (zone.snap, zone.on.clone());
        if let Some(target) = snap_target(st, a.node, zone_node, snap) {
            move_to(st, a.node, target, snap_time);
        }
        call(calls, &on, "drop", info);
        call(calls, &zone_on, "drop", info);
    } else if refused.is_some() || home == Home::Always {
        if home != Home::Never {
            move_to(st, a.node, a.start, snap_time);
        }
        let info = Info { zone: refused.map(|(_, n)| n), ..info };
        call(calls, &on, "reject", info);
        if let Some((zid, _)) = refused
            && let Some(z) = st.interact.zones.iter().find(|z| z.id == zid)
        {
            call(calls, &z.on, "reject", info);
        }
    } else {
        call(calls, &on, "drop", info);
    }
    let end = Info { accepted: chosen.is_some(), zone: chosen.map(|(_, n)| n), ..info };
    call(calls, &on, "end", end);
}

/// The position (in the node's parent) that puts the node's center on the zone's center.
fn snap_target(st: &State, node: NodeId, zone: NodeId, snap: Snap) -> Option<Vec2> {
    let offset = match snap {
        Snap::Off => return None,
        Snap::Center => Vec2::ZERO,
        Snap::Offset(o) => o,
    };
    let zc = node_size(st, zone).ok()? * 0.5 + offset;
    let design = st.scene.to_design(zone)?.transform_point2(zc);
    let target = parent_local(st, node, design)?;
    let n = st.scene.get(node)?;
    let c = node_size(st, node).ok()? * 0.5;
    let off = n.transform.to_affine().transform_point2(c) - n.transform.position;
    Some(target - off)
}

/// Moves hover on hotspots to the one under `p` (the mouse), calling `enter`/`leave`.
pub(crate) fn hover(lua: &Lua, shared: &Shared, p: Option<Vec2>) {
    let mut calls = Vec::new();
    {
        let mut st = shared.borrow_mut();
        let st = &mut *st;
        if st.interact.spots.is_empty() && st.interact.hover.is_none() {
            return;
        }
        st.interact.purge(&st.scene);
        let covered = p.is_some_and(|p| st.ui.ui.covers(p));
        let want = match p {
            Some(p) if !covered && st.interact.active.is_empty() => match topmost(st, p, false) {
                Some(Cand::Spot(s)) => Some(s),
                _ => None,
            },
            _ => None,
        };
        if want != st.interact.hover {
            let at = p.unwrap_or_default();
            let spot = |h: Option<u64>| h.and_then(|h| st.interact.spots.iter().find(|s| s.id == h));
            if let Some(old) = spot(st.interact.hover) {
                call(&mut calls, &old.on, "leave", Info::at(old.area.node, at, 0));
            }
            if let Some(new) = spot(want) {
                call(&mut calls, &new.on, "enter", Info::at(new.area.node, at, 0));
            }
            st.interact.hover = want;
        }
    }
    fire(lua, shared, calls);
}

// ---------------------------------------------------------------------------------------
// Lua API

#[derive(Clone, Copy)]
enum Kind {
    Drag,
    Zone,
    Spot,
}

struct Handle {
    kind: Kind,
    id: u64,
}

fn callbacks(o: &Table, names: &[&'static str]) -> mlua::Result<Callbacks> {
    let mut on = HashMap::new();
    for &name in names {
        match o.get::<LuaValue>(format!("on_{name}"))? {
            LuaValue::Nil => {}
            LuaValue::Function(f) => {
                on.insert(name, f);
            }
            _ => return Err(mlua::Error::runtime(format!("`on_{name}` must be a function"))),
        }
    }
    Ok(on)
}

fn rect_of(v: &LuaValue, what: &str) -> mlua::Result<Rect> {
    let LuaValue::Table(t) = v else {
        return Err(mlua::Error::runtime(format!("`{what}` must be {{ x, y, width, height }}")));
    };
    let c = |i: usize| -> mlua::Result<f32> { number(&t.raw_get(i)?, what) };
    Ok(Rect::new(c(1)?, c(2)?, c(3)?, c(4)?))
}

fn area_of(st: &State, node: &NodeRef, o: &Table, polygon: bool) -> mlua::Result<Area> {
    if !st.scene.contains(node.0) {
        return Err(mlua::Error::runtime("this node was removed"));
    }
    let shape = match (o.get::<LuaValue>("rect")?, o.get::<LuaValue>("polygon")?) {
        (LuaValue::Nil, LuaValue::Nil) => Shape::Area,
        (r, LuaValue::Nil) => Shape::Rect(rect_of(&r, "rect")?),
        (LuaValue::Nil, LuaValue::Table(t)) if polygon => {
            let nums: Vec<f32> = t
                .sequence_values::<LuaValue>()
                .map(|v| number(&v?, "polygon"))
                .collect::<mlua::Result<_>>()?;
            if nums.len() < 6 || !nums.len().is_multiple_of(2) {
                return Err(mlua::Error::runtime(
                    "`polygon` is a flat list of x, y pairs, at least three points",
                ));
            }
            Shape::Polygon(nums.as_chunks::<2>().0.iter().map(|c| Vec2::new(c[0], c[1])).collect())
        }
        _ => return Err(mlua::Error::runtime("give at most one of `rect` and `polygon`")),
    };
    let alpha = hit::threshold(&o.get::<LuaValue>("alpha")?)?;
    let enabled = o.get::<Option<bool>>("enabled")?.unwrap_or(true);
    Ok(Area { node: node.0, shape, alpha, enabled })
}

fn pick<T: Copy>(o: &Table, key: &str, names: &[(&str, T)]) -> mlua::Result<Option<T>> {
    let Some(s) = o.get::<Option<String>>(key)? else { return Ok(None) };
    match names.iter().find(|(n, _)| *n == s) {
        Some((_, v)) => Ok(Some(*v)),
        None => {
            let list: Vec<&str> = names.iter().map(|(n, _)| *n).collect();
            Err(mlua::Error::runtime(format!("`{key}` must be one of: {}", list.join(", "))))
        }
    }
}

fn create_drag(lua: &Lua, (node, o): (AnyUserData, Option<Table>)) -> mlua::Result<Handle> {
    let node = NodeRef(node.borrow::<NodeRef>()?.0);
    let o = match o {
        Some(o) => o,
        None => lua.create_table()?,
    };
    let on = callbacks(&o, &["start", "move", "drop", "reject", "click", "end"])?;
    let bounds = match o.get::<LuaValue>("bounds")? {
        LuaValue::Nil => None,
        v => Some(rect_of(&v, "bounds")?),
    };
    let home =
        pick(&o, "home", &[("rejected", Home::Rejected), ("always", Home::Always), ("never", Home::Never)])?
            .unwrap_or(Home::Rejected);
    let axis =
        pick(&o, "axis", &[("both", Axis::Both), ("x", Axis::X), ("y", Axis::Y)])?.unwrap_or(Axis::Both);
    let tag = o.get::<Option<String>>("tag")?;
    let lift = o.get::<Option<bool>>("lift")?.unwrap_or(true);
    let snap_time = o.get::<Option<f32>>("snap_time")?.unwrap_or(0.15);
    with(lua, |st| {
        let area = area_of(st, &node, &o, true)?;
        let id = {
            st.interact.next += 1;
            st.interact.next
        };
        st.interact.drags.push(Draggable {
            id,
            area,
            tag,
            bounds: bounds.map(|b| Rect { min: b.min, max: b.max }),
            axis,
            lift,
            home,
            snap_time,
            on,
        });
        Ok(Handle { kind: Kind::Drag, id })
    })
}

fn create_zone(lua: &Lua, (node, o): (AnyUserData, Option<Table>)) -> mlua::Result<Handle> {
    let node = NodeRef(node.borrow::<NodeRef>()?.0);
    let o = match o {
        Some(o) => o,
        None => lua.create_table()?,
    };
    let on = callbacks(&o, &["enter", "leave", "drop", "reject"])?;
    let accept = match o.get::<LuaValue>("accept")? {
        LuaValue::Nil => Accept::Any,
        LuaValue::String(s) => Accept::Tags(vec![s.to_string_lossy().to_string()]),
        LuaValue::Function(f) => Accept::Func(f),
        LuaValue::Table(t) => Accept::Tags(t.sequence_values::<String>().collect::<mlua::Result<_>>()?),
        _ => {
            return Err(mlua::Error::runtime(
                "`accept` is a tag, a list of tags or a function(info) -> boolean",
            ));
        }
    };
    let snap = match o.get::<LuaValue>("snap")? {
        LuaValue::Nil => Snap::Center,
        LuaValue::String(s) => match &*s.to_string_lossy() {
            "center" => Snap::Center,
            "none" => Snap::Off,
            _ => return Err(mlua::Error::runtime("`snap` must be \"center\", \"none\" or { x, y }")),
        },
        LuaValue::Table(t) => Snap::Offset(Vec2::new(
            t.get::<Option<f32>>("x")?.unwrap_or(0.0),
            t.get::<Option<f32>>("y")?.unwrap_or(0.0),
        )),
        _ => return Err(mlua::Error::runtime("`snap` must be \"center\", \"none\" or { x, y }")),
    };
    let by = pick(&o, "by", &[("pointer", By::Pointer), ("center", By::Center)])?.unwrap_or(By::Pointer);
    let capacity = o.get::<Option<usize>>("capacity")?;
    with(lua, |st| {
        let area = area_of(st, &node, &o, true)?;
        st.interact.next += 1;
        let id = st.interact.next;
        st.interact.zones.push(Zone { id, area, accept, snap, by, capacity, occupants: Vec::new(), on });
        Ok(Handle { kind: Kind::Zone, id })
    })
}

fn create_spot(lua: &Lua, (node, o): (AnyUserData, Option<Table>)) -> mlua::Result<Handle> {
    let node = NodeRef(node.borrow::<NodeRef>()?.0);
    let o = match o {
        Some(o) => o,
        None => lua.create_table()?,
    };
    let on = callbacks(&o, &["click", "press", "release", "enter", "leave"])?;
    let priority = o.get::<Option<i32>>("priority")?.unwrap_or(0);
    let block = o.get::<Option<bool>>("block")?.unwrap_or(true);
    let cursor = match o.get::<Option<String>>("cursor")? {
        Some(c) => Some(super::display::cursor_name(&c)?),
        None => None,
    };
    with(lua, |st| {
        let area = area_of(st, &node, &o, true)?;
        st.interact.next += 1;
        let id = st.interact.next;
        st.interact.spots.push(Spot { id, area, priority, block, cursor, on });
        Ok(Handle { kind: Kind::Spot, id })
    })
}

fn area_mut<'a>(st: &'a mut State, h: &Handle) -> Option<&'a mut Area> {
    let it = &mut st.interact;
    match h.kind {
        Kind::Drag => it.drags.iter_mut().find(|x| x.id == h.id).map(|x| &mut x.area),
        Kind::Zone => it.zones.iter_mut().find(|x| x.id == h.id).map(|x| &mut x.area),
        Kind::Spot => it.spots.iter_mut().find(|x| x.id == h.id).map(|x| &mut x.area),
    }
}

impl UserData for Handle {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            with(lua, |st| {
                let id = this.id;
                match (this.kind, key.as_str()) {
                    (_, "enabled") => Ok(LuaValue::Boolean(area_mut(st, this).is_some_and(|a| a.enabled))),
                    (Kind::Drag, "dragging") => {
                        Ok(LuaValue::Boolean(st.interact.active.values().any(|a| a.drag == id)))
                    }
                    (Kind::Spot, "hovered") => Ok(LuaValue::Boolean(st.interact.hover == Some(id))),
                    (Kind::Zone, "occupants") => {
                        let nodes = st
                            .interact
                            .zones
                            .iter()
                            .find(|z| z.id == id)
                            .map(|z| z.occupants.clone())
                            .unwrap_or_default();
                        lua.create_sequence_from(nodes.into_iter().map(NodeRef))?.into_lua(lua)
                    }
                    _ => Err(mlua::Error::runtime(format!("no property `{key}` here"))),
                }
            })
        });
        m.add_meta_method(MetaMethod::NewIndex, |lua, this, (key, v): (String, LuaValue)| {
            with(lua, |st| match key.as_str() {
                "enabled" => {
                    let on =
                        v.as_boolean().ok_or_else(|| mlua::Error::runtime("`enabled` must be a boolean"))?;
                    if let Some(a) = area_mut(st, this) {
                        a.enabled = on;
                    }
                    if !on && matches!(this.kind, Kind::Drag) {
                        abort(st, this.id);
                    }
                    Ok(())
                }
                _ => Err(mlua::Error::runtime(format!("`{key}` cannot be set"))),
            })
        });
        m.add_method("remove", |lua, this, ()| {
            with(lua, |st| {
                let (it, id) = (&mut st.interact, this.id);
                match this.kind {
                    Kind::Drag => {
                        abort(st, id);
                        st.interact.drags.retain(|x| x.id != id);
                    }
                    Kind::Zone => it.zones.retain(|x| x.id != id),
                    Kind::Spot => it.spots.retain(|x| x.id != id),
                }
                Ok(())
            })
        });
        // drag:cancel(): lets go and sends the item home.
        m.add_method("cancel", |lua, this, ()| {
            with(lua, |st| {
                abort(st, this.id);
                Ok(())
            })
        });
        // zone:clear(): forgets what was dropped (a slot that emptied).
        m.add_method("clear", |lua, this, ()| {
            with(lua, |st| {
                if let Some(z) = st.interact.zones.iter_mut().find(|z| z.id == this.id) {
                    z.occupants.clear();
                }
                Ok(())
            })
        });
    }
}

/// Ends the drags of draggable `id` without callbacks and sends the item back.
fn abort(st: &mut State, id: u64) {
    let ids: Vec<u32> = st.interact.active.iter().filter(|(_, a)| a.drag == id).map(|(p, _)| *p).collect();
    for p in ids {
        let Some(a) = st.interact.active.remove(&p) else { continue };
        let Some(d) = st.interact.drags.iter().find(|d| d.id == id) else { continue };
        let (lift, secs) = (d.lift, d.snap_time);
        if lift && let Some(n) = st.scene.get_mut(a.node) {
            n.z -= LIFT;
        }
        move_to(st, a.node, a.start, secs);
    }
}

pub(super) fn install(lua: &Lua, k: &Table) -> mlua::Result<()> {
    k.set("drag", lua.create_function(create_drag)?)?;
    k.set("drop_zone", lua.create_function(create_zone)?)?;
    k.set("hotspot", lua.create_function(create_spot)?)?;
    Ok(())
}
