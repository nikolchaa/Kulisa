//! Layout: sizes from the bottom up (content, with the available space), then positions
//! from the top down. Units are design units.

use kulisa_core::{Rect, Vec2};

use crate::{Align, Element, Fill, Justify, Kind, Layout, Size, TextLook, Widget};

/// What layout needs from the host: the size of a text.
pub trait Measure {
    /// The size of `text` wrapped at `max_width` (unwrapped when `None`).
    fn text(&mut self, text: &TextLook, max_width: Option<f32>) -> Vec2;
}

fn axis(v: Vec2, a: usize) -> f32 {
    if a == 0 { v.x } else { v.y }
}

fn set_axis(v: &mut Vec2, a: usize, x: f32) {
    if a == 0 { v.x = x } else { v.y = x }
}

fn clamp(el: &Element, s: Vec2) -> Vec2 {
    let s = s.max(el.style.min);
    match el.style.max {
        Some(m) => s.min(m),
        None => s,
    }
    .max(Vec2::ZERO)
}

fn finite(v: f32) -> Option<f32> {
    v.is_finite().then_some(v.max(0.0))
}

/// The size the content asks for, border box, given `avail` (border box, may be infinite).
fn natural<N>(w: &Widget<N>, avail: Vec2, m: &mut dyn Measure) -> Vec2 {
    let el = &w.el;
    let pad = el.style.padding.size();
    let inner = (avail - pad).max(Vec2::ZERO);
    let content = match el.kind {
        Kind::Text => match &el.text {
            Some(t) => m.text(t, finite(inner.x)),
            None => Vec2::ZERO,
        },
        Kind::Image => match el.look.image {
            Some(Fill::Image { size, .. }) => size,
            _ => Vec2::ZERO,
        },
        Kind::Node => el.node_size,
        Kind::Slider => {
            let t = el.slider.as_ref().map_or(32.0, |s| s.thumb_size);
            if el.slider.as_ref().is_some_and(|s| s.vertical) {
                Vec2::new(t, 240.0)
            } else {
                Vec2::new(240.0, t)
            }
        }
        Kind::Scroll => {
            let a = if el.horizontal { 0 } else { 1 };
            let c = stack_natural(w, inner, a, m);
            c.min(inner)
        }
        Kind::Box => match el.style.layout {
            Layout::Overlay => {
                let mut s = Vec2::ZERO;
                for c in &w.children {
                    s = s.max(size_in(c, inner, m) + c.el.style.margin.size());
                }
                s
            }
            Layout::Stack { horizontal } => stack_natural(w, inner, if horizontal { 0 } else { 1 }, m),
            Layout::Grid { columns } => {
                let cols = columns.max(1) as usize;
                let gap = el.style.gap;
                let cell = match finite(inner.x) {
                    Some(x) => (x - gap * (cols - 1) as f32) / cols as f32,
                    None => w.children.iter().map(|c| natural(c, Vec2::INFINITY, m).x).fold(0.0, f32::max),
                };
                let mut h = 0.0;
                for (r, row) in w.children.chunks(cols).enumerate() {
                    let rh =
                        row.iter().map(|c| grid_child(c, cell, inner.y, m).y + c.el.style.margin.size().y);
                    h += rh.fold(0.0, f32::max) + if r > 0 { gap } else { 0.0 };
                }
                Vec2::new(cell * cols as f32 + gap * (cols - 1) as f32, h)
            }
        },
    };
    clamp(el, content + pad)
}

fn stack_natural<N>(w: &Widget<N>, inner: Vec2, a: usize, m: &mut dyn Measure) -> Vec2 {
    let mut s = Vec2::ZERO;
    let n = w.children.len();
    let mut avail = inner;
    set_axis(&mut avail, a, f32::INFINITY);
    for c in &w.children {
        let cs = size_in(c, avail, m) + c.el.style.margin.size();
        let (main, cross) = (axis(s, a) + axis(cs, a), axis(s, 1 - a).max(axis(cs, 1 - a)));
        set_axis(&mut s, a, main);
        set_axis(&mut s, 1 - a, cross);
    }
    let main = axis(s, a) + w.el.style.gap * n.saturating_sub(1) as f32;
    set_axis(&mut s, a, main);
    s
}

fn grid_child<N>(c: &Widget<N>, cell: f32, avail_h: f32, m: &mut dyn Measure) -> Vec2 {
    let margin = c.el.style.margin.size();
    let width = match c.el.style.width {
        Size::Auto | Size::Fill(_) => cell - margin.x,
        _ => size_in(c, Vec2::new(cell, avail_h), m).x,
    };
    let height = match c.el.style.height {
        Size::Auto => natural(c, Vec2::new(width, avail_h), m).y,
        _ => size_in(c, Vec2::new(cell, avail_h), m).y,
    };
    clamp(&c.el, Vec2::new(width, height))
}

/// The widget's border-box size inside a parent content box of `parent`.
pub(crate) fn size_in<N>(w: &Widget<N>, parent: Vec2, m: &mut dyn Measure) -> Vec2 {
    let el = &w.el;
    let room = (parent - el.style.margin.size()).max(Vec2::ZERO);
    let resolve = |size: Size, room: f32, parent: f32| -> Option<f32> {
        match size {
            Size::Px(v) => Some(v),
            Size::Percent(p) if parent.is_finite() => Some(parent * p),
            Size::Fill(_) if room.is_finite() => Some(room),
            _ => None,
        }
    };
    let width = match resolve(el.style.width, room.x, parent.x) {
        Some(v) => v,
        None => natural(w, room, m).x,
    };
    let width = clamp(el, Vec2::new(width, 0.0)).x;
    let height = match resolve(el.style.height, room.y, parent.y) {
        Some(v) => v,
        None => natural(w, Vec2::new(width, room.y), m).y,
    };
    clamp(el, Vec2::new(width, height))
}

pub(crate) fn layout_root<N>(root: &mut Widget<N>, area: Rect, m: &mut dyn Measure) {
    let el = &root.el;
    let room = (area.size() - el.style.margin.size()).max(Vec2::ZERO);
    let mut s = size_in(root, area.size(), m);
    // An auto-sized root fills its area.
    if el.style.width == Size::Auto {
        s.x = clamp(el, room).x;
    }
    if el.style.height == Size::Auto {
        s.y = clamp(el, Vec2::new(s.x, room.y)).y;
    }
    let pos = area.min + el.style.margin.start() + (room - s) * el.style.anchor.0 + el.style.offset;
    arrange(root, Rect { min: pos, max: pos + s }, m);
}

fn arrange<N>(w: &mut Widget<N>, rect: Rect, m: &mut dyn Measure) {
    w.rect = rect;
    w.clip = None;
    let pad = w.el.style.padding;
    let inner = Rect {
        min: rect.min + pad.start(),
        max: (rect.max - Vec2::new(pad.right, pad.bottom)).max(rect.min + pad.start()),
    };
    let isize = inner.size();
    match (w.el.kind, w.el.style.layout) {
        (Kind::Box, Layout::Overlay) => {
            for c in &mut w.children {
                let s = size_in(c, isize, m);
                let st = &c.el.style;
                let room = isize - st.margin.size();
                let pos = inner.min + st.margin.start() + (room - s) * st.anchor.0 + st.offset;
                arrange(c, Rect { min: pos, max: pos + s }, m);
            }
        }
        (Kind::Box, Layout::Stack { horizontal }) => {
            let a = if horizontal { 0 } else { 1 };
            stack(w, inner, a, m);
        }
        (Kind::Box, Layout::Grid { columns }) => {
            let cols = columns.max(1) as usize;
            let gap = w.el.style.gap;
            let align = w.el.style.align;
            let cell = ((isize.x - gap * (cols - 1) as f32) / cols as f32).max(0.0);
            let mut y = inner.min.y;
            for row in w.children.chunks_mut(cols) {
                let sizes: Vec<Vec2> = row.iter().map(|c| grid_child(c, cell, isize.y, m)).collect();
                let rh =
                    row.iter().zip(&sizes).map(|(c, s)| s.y + c.el.style.margin.size().y).fold(0.0, f32::max);
                for (i, (c, s)) in row.iter_mut().zip(sizes).enumerate() {
                    let st = &c.el.style;
                    let free = cell - s.x - st.margin.size().x;
                    let dx = match align {
                        Align::Center => free * 0.5,
                        Align::End => free,
                        _ => 0.0,
                    };
                    let pos = Vec2::new(inner.min.x + i as f32 * (cell + gap) + dx, y)
                        + st.margin.start()
                        + st.offset;
                    arrange(c, Rect { min: pos, max: pos + s }, m);
                }
                y += rh + gap;
            }
        }
        (Kind::Scroll, _) => {
            let a = if w.el.horizontal { 0 } else { 1 };
            let mut avail = isize;
            set_axis(&mut avail, a, f32::INFINITY);
            let mut content = stack_natural(w, avail, a, m);
            set_axis(&mut content, 1 - a, axis(isize, 1 - a));
            let max = (axis(content, a) - axis(isize, a)).max(0.0);
            let old_max = (axis(w.content, a) - axis(isize, a)).max(0.0);
            let at_end = axis(w.scroll, a) >= old_max - 1.0;
            let mut off = axis(w.scroll, a);
            if w.el.start_at_end && (w.fresh || at_end) {
                off = max;
            }
            let mut scroll = Vec2::ZERO;
            set_axis(&mut scroll, a, off.clamp(0.0, max));
            w.scroll = scroll;
            w.content = content;
            let mut moved = inner;
            moved.min -= scroll;
            moved.max = moved.min + content;
            stack(w, moved, a, m);
            w.clip = Some(inner);
        }
        _ => {}
    }
    if w.el.clip && w.clip.is_none() {
        w.clip = Some(rect);
    }
}

/// Places children in a row (`a` = 0) or column inside `inner`.
fn stack<N>(w: &mut Widget<N>, inner: Rect, a: usize, m: &mut dyn Measure) {
    let isize = inner.size();
    let st = w.el.style.clone();
    let n = w.children.len();
    let mut sizes = Vec::with_capacity(n);
    let mut used = st.gap * n.saturating_sub(1) as f32;
    let mut weights = 0.0;
    let mut avail = isize;
    set_axis(&mut avail, a, f32::INFINITY);
    for c in &w.children {
        let main = if a == 0 { c.el.style.width } else { c.el.style.height };
        let s = size_in(c, if matches!(main, Size::Fill(_)) { isize } else { avail }, m);
        used += axis(c.el.style.margin.size(), a);
        match main {
            Size::Fill(wt) => weights += wt.max(0.0),
            _ => used += axis(s, a),
        }
        sizes.push(s);
    }
    let free = (axis(isize, a) - used).max(0.0);
    for (c, s) in w.children.iter().zip(sizes.iter_mut()) {
        let cst = &c.el.style;
        let (main, cross) = if a == 0 { (cst.width, cst.height) } else { (cst.height, cst.width) };
        let cross_room = axis(isize, 1 - a) - axis(cst.margin.size(), 1 - a);
        if let Size::Fill(wt) = main {
            let share = if weights > 0.0 { free * wt.max(0.0) / weights } else { 0.0 };
            set_axis(s, a, share);
            if a == 0 && cross == Size::Auto {
                s.y = natural(c, Vec2::new(share, cross_room), m).y;
            }
        }
        if matches!(cross, Size::Fill(_)) || (cross == Size::Auto && st.align == Align::Stretch) {
            set_axis(s, 1 - a, cross_room.max(0.0));
            if a == 1 && cst.height == Size::Auto {
                s.y = natural(c, Vec2::new(s.x, f32::INFINITY), m).y;
            }
        }
        *s = clamp(&c.el, *s);
    }
    let total: f32 = w
        .children
        .iter()
        .zip(&sizes)
        .map(|(c, s)| axis(*s, a) + axis(c.el.style.margin.size(), a))
        .sum::<f32>()
        + st.gap * n.saturating_sub(1) as f32;
    let extra = (axis(isize, a) - total).max(0.0);
    let (mut pos, spacing) = match st.justify {
        Justify::Start => (0.0, st.gap),
        Justify::Center => (extra * 0.5, st.gap),
        Justify::End => (extra, st.gap),
        Justify::Between if n > 1 => (0.0, st.gap + extra / (n - 1) as f32),
        Justify::Between => (0.0, st.gap),
    };
    for (c, s) in w.children.iter_mut().zip(sizes) {
        let cst = &c.el.style;
        let margin = cst.margin;
        let cross_free = axis(isize, 1 - a) - axis(s, 1 - a) - axis(margin.size(), 1 - a);
        let cross = match st.align {
            Align::Center => cross_free * 0.5,
            Align::End => cross_free,
            _ => 0.0,
        };
        let mut p = Vec2::ZERO;
        set_axis(&mut p, a, pos + axis(margin.start(), a));
        set_axis(&mut p, 1 - a, cross + axis(margin.start(), 1 - a));
        let p = inner.min + p + cst.offset;
        pos += axis(s, a) + axis(margin.size(), a) + spacing;
        arrange(c, Rect { min: p, max: p + s }, m);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, Element, Kind, Look, Sides, TextLook, Ui};
    use kulisa_core::Color;

    /// Text is 10 units per character and 20 per line, wrapping at whole words.
    struct Fixed;
    impl Measure for Fixed {
        fn text(&mut self, t: &TextLook, max: Option<f32>) -> Vec2 {
            let chars = t.markup.chars().count() as f32 * 10.0;
            match max {
                Some(m) if chars > m && m > 0.0 => Vec2::new(m, 20.0 * (chars / m).ceil()),
                _ => Vec2::new(chars, 20.0),
            }
        }
    }

    fn text(s: &str) -> Element {
        Element {
            kind: Kind::Text,
            text: Some(TextLook { markup: s.into(), ..Default::default() }),
            ..Default::default()
        }
    }

    fn boxed(layout: Layout, children: Vec<Element>) -> Element {
        let mut e = Element::new(Kind::Box).with_children(children);
        e.style.layout = layout;
        e
    }

    fn lay(root: Element) -> Ui<()> {
        let area = Rect::new(0.0, 0.0, 1000.0, 500.0);
        let mut ui = Ui::new(crate::Areas { screen: area, safe: area, design: area });
        ui.show("s", 0, false, crate::Area::Screen);
        ui.set_tree("s", root);
        ui.layout(&mut Fixed);
        ui
    }

    fn rect(ui: &Ui<()>, path: &[usize]) -> Rect {
        let mut w = ui.screens[0].root.as_ref().unwrap();
        for &i in path {
            w = &w.children[i];
        }
        w.rect
    }

    #[test]
    fn columns_stack_and_fill() {
        let mut fill = Element::new(Kind::Box);
        fill.style.height = Size::Fill(1.0);
        let mut col = boxed(Layout::Stack { horizontal: false }, vec![text("Hello"), fill, text("End")]);
        col.style.gap = 10.0;
        col.style.padding = Sides::all(5.0);
        col.style.align = Align::Stretch;
        let ui = lay(col);
        assert_eq!(rect(&ui, &[]), Rect::new(0.0, 0.0, 1000.0, 500.0), "the root fills its area");
        assert_eq!(rect(&ui, &[0]), Rect::new(5.0, 5.0, 990.0, 20.0));
        // 490 inner - 2 texts - 2 gaps = 430 for the filler.
        assert_eq!(rect(&ui, &[1]), Rect::new(5.0, 35.0, 990.0, 430.0));
        assert_eq!(rect(&ui, &[2]).min.y, 475.0);
    }

    #[test]
    fn rows_justify_align_and_share_by_weight() {
        let mut a = Element::new(Kind::Box);
        a.style.width = Size::Fill(1.0);
        let mut b = Element::new(Kind::Box);
        b.style.width = Size::Fill(3.0);
        let mut row = boxed(Layout::Stack { horizontal: true }, vec![text("ab"), a, b]);
        row.style.align = Align::Center;
        let ui = lay(row);
        assert_eq!(rect(&ui, &[0]), Rect::new(0.0, 240.0, 20.0, 20.0));
        assert_eq!(rect(&ui, &[1]).size().x, 245.0);
        assert_eq!(rect(&ui, &[2]).min.x, 265.0);

        let mut row = boxed(Layout::Stack { horizontal: true }, vec![text("a"), text("b"), text("c")]);
        row.style.justify = Justify::Between;
        let ui = lay(row);
        assert_eq!(rect(&ui, &[1]).min.x, 495.0);
        assert_eq!(rect(&ui, &[2]).max.x, 1000.0);
    }

    #[test]
    fn overlay_anchors_and_text_wraps() {
        let mut bottom = boxed(Layout::Overlay, vec![text(&"x".repeat(150))]);
        bottom.style.width = Size::Percent(0.8);
        bottom.style.padding = Sides::all(10.0);
        bottom.style.anchor = Anchor::named("bottom").unwrap();
        bottom.style.offset = Vec2::new(0.0, -20.0);
        let mut corner = text("menu");
        corner.style.anchor = Anchor::named("top_right").unwrap();
        corner.style.margin = Sides::all(8.0);
        let ui = lay(boxed(Layout::Overlay, vec![bottom, corner]));
        // 800 wide box, 780 inner: 1500 units of text wrap into 2 lines.
        assert_eq!(rect(&ui, &[0]), Rect::new(100.0, 500.0 - 20.0 - 60.0, 800.0, 60.0));
        assert_eq!(rect(&ui, &[0, 0]).size(), Vec2::new(780.0, 40.0));
        assert_eq!(rect(&ui, &[1]), Rect::new(1000.0 - 8.0 - 40.0, 8.0, 40.0, 20.0));
    }

    #[test]
    fn grids_fill_cells_row_by_row() {
        let cells: Vec<Element> = (0..5).map(|i| text(&"y".repeat(i + 1))).collect();
        let mut g = boxed(Layout::Grid { columns: 2 }, cells);
        g.style.gap = 20.0;
        g.style.width = Size::Px(420.0);
        let mut root = boxed(Layout::Overlay, vec![g]);
        root.style.padding = Sides::all(0.0);
        let ui = lay(root);
        assert_eq!(rect(&ui, &[0, 1]), Rect::new(220.0, 0.0, 200.0, 20.0));
        assert_eq!(rect(&ui, &[0, 4]).min, Vec2::new(0.0, 80.0));
        assert_eq!(rect(&ui, &[0]).size(), Vec2::new(420.0, 100.0));
    }

    #[test]
    fn scroll_clips_and_can_start_at_the_end() {
        let lines: Vec<Element> = (0..40).map(|_| text("line")).collect();
        let mut sc = Element::new(Kind::Scroll).with_children(lines);
        sc.style.height = Size::Px(100.0);
        sc.style.width = Size::Px(300.0);
        sc.start_at_end = true;
        let mut ui = lay(boxed(Layout::Overlay, vec![sc.clone()]));
        let w = &ui.screens[0].root.as_ref().unwrap().children[0];
        assert_eq!(w.content.y, 800.0);
        assert_eq!(w.scroll.y, 700.0, "starts at the end");
        assert_eq!(w.clip, Some(Rect::new(0.0, 0.0, 300.0, 100.0)));
        assert_eq!(w.children[39].rect.max.y, 100.0);
        // More content keeps it at the end.
        let mut more = sc;
        more.children.push(text("new"));
        ui.set_tree("s", boxed(Layout::Overlay, vec![more]));
        ui.layout(&mut Fixed);
        assert_eq!(ui.screens[0].root.as_ref().unwrap().children[0].scroll.y, 720.0);
    }

    #[test]
    fn states_override_looks() {
        let mut e = Element::new(Kind::Box);
        e.look.background = Some(Fill::Color(Color::BLACK));
        e.look.opacity = Some(0.5);
        e.hover.background = Some(Fill::Color(Color::WHITE));
        e.disabled_look.opacity = Some(0.2);
        let l = e.look_for(crate::State { hover: true, ..Default::default() });
        assert_eq!((l.background, l.opacity), (Some(Fill::Color(Color::WHITE)), Some(0.5)));
        let l = e.look_for(crate::State { hover: true, disabled: true, ..Default::default() });
        assert_eq!((l.background, l.opacity), (Some(Fill::Color(Color::BLACK)), Some(0.2)));
        let _ = Look::default();
    }
}
