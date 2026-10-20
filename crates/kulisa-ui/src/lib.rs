//! Declarative screens. A screen is rebuilt into a tree of [`Element`]s (by Luau, in the
//! engine), reconciled into retained [`Widget`]s that keep their state and host nodes, laid
//! out against an area of the screen, and hit tested for pointer input.
//!
//! The crate draws nothing: the host turns widgets into scene nodes (see `kulisa-lua`),
//! measures text for layout ([`Measure`]) and runs callbacks ([`UiEvent`]).

mod element;
mod focus;
mod gesture;
mod input;
mod layout;
mod signal;

pub use element::{
    Align, Anchor, Element, Fill, Justify, Kind, Layout, Look, Sides, Size, SliderSpec, State, Style,
    TextAlign, TextLook,
};
pub use focus::Nav;
pub use gesture::{Direction, Gesture, Gestures};
pub use input::{DragPhase, UiEvent};
pub use layout::Measure;
pub use signal::Deps;

use kulisa_core::{Rect, Vec2};

/// What a screen is laid out against.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Area {
    /// The part of the window the game shows.
    Screen,
    /// The screen minus notches, rounded corners and system bars.
    #[default]
    Safe,
    /// The design resolution's rectangle (stays readable on ultrawide screens).
    Design,
    /// A rectangle in design units (windows, panels, popups).
    Rect(Rect),
}

/// The rectangles of each [`Area`], in design units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Areas {
    pub screen: Rect,
    pub safe: Rect,
    pub design: Rect,
}

impl Areas {
    pub fn get(&self, a: Area) -> Rect {
        match a {
            Area::Screen => self.screen,
            Area::Safe => self.safe,
            Area::Design => self.design,
            Area::Rect(r) => r,
        }
    }
}

/// Interaction state of a widget.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interaction {
    pub hover: bool,
    pub pressed: bool,
    /// Focused with the keyboard or gamepad; looks hovered.
    pub focus: bool,
    /// A slider value set by dragging, shown until the screen rebuilds with a new value.
    pub drag_value: Option<f32>,
}

/// A retained widget: its current description, layout and state, plus the host's data `N`
/// (its scene nodes).
#[derive(Debug)]
pub struct Widget<N> {
    pub el: Element,
    pub children: Vec<Widget<N>>,
    /// Border box in design units.
    pub rect: Rect,
    /// Clip applied to the widget's children (scroll viewports), in design units.
    pub clip: Option<Rect>,
    pub state: Interaction,
    /// Scroll offset (scroll widgets).
    pub scroll: Vec2,
    /// Size of the scrolled content (scroll widgets).
    pub content: Vec2,
    /// Whether the widget was just created (the host makes its nodes).
    pub fresh: bool,
    pub host: N,
}

impl<N: Default> Widget<N> {
    fn new(mut el: Element) -> Self {
        let children = std::mem::take(&mut el.children).into_iter().map(Widget::new).collect();
        Self {
            el,
            children,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            clip: None,
            state: Interaction::default(),
            scroll: Vec2::ZERO,
            content: Vec2::ZERO,
            fresh: true,
            host: N::default(),
        }
    }
}

impl<N> Widget<N> {
    /// Visits the widget and its descendants, parents first.
    pub fn walk(&self, f: &mut impl FnMut(&Widget<N>)) {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }

    /// Moves the widget and its descendants by `d` (design units).
    pub fn translate(&mut self, d: Vec2) {
        self.rect.min += d;
        self.rect.max += d;
        if let Some(c) = &mut self.clip {
            c.min += d;
            c.max += d;
        }
        for c in &mut self.children {
            c.translate(d);
        }
    }

    /// The first widget (depth first) whose `id` is `id`.
    pub fn find(&self, id: &str) -> Option<&Widget<N>> {
        if self.el.id.as_deref() == Some(id) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    /// The look after the state overrides of the nearest button (`state`).
    pub fn look(&self, state: State) -> Look {
        self.el.look_for(state)
    }

    fn take_hosts(self, out: &mut Vec<N>) {
        out.push(self.host);
        for c in self.children {
            c.take_hosts(out);
        }
    }
}

/// Reconciles `old` children with `new` elements: widgets with the same `key` (or, without
/// keys, the same position among unkeyed siblings) and the same kind are kept with their
/// state and host data. The host data of dropped widgets goes to `removed`.
pub fn reconcile<N: Default>(old: Vec<Widget<N>>, new: Vec<Element>, removed: &mut Vec<N>) -> Vec<Widget<N>> {
    let mut keyed: Vec<Option<Widget<N>>> = Vec::new();
    let mut unkeyed: std::collections::VecDeque<Widget<N>> = Default::default();
    for w in old {
        if w.el.key.is_some() {
            keyed.push(Some(w));
        } else {
            unkeyed.push_back(w);
        }
    }
    let mut out = Vec::with_capacity(new.len());
    for el in new {
        let found = match &el.key {
            Some(k) => keyed
                .iter_mut()
                .find(|w| w.as_ref().is_some_and(|w| w.el.key.as_ref() == Some(k) && w.el.kind == el.kind))
                .and_then(Option::take),
            None => match unkeyed.pop_front() {
                Some(w) if w.el.kind == el.kind => Some(w),
                Some(w) => {
                    w.take_hosts(removed);
                    None
                }
                None => None,
            },
        };
        out.push(match found {
            Some(mut w) => {
                let mut el = el;
                let children = std::mem::take(&mut el.children);
                w.children = reconcile(std::mem::take(&mut w.children), children, removed);
                if el.kind == Kind::Slider
                    && el.slider.as_ref().map(|s| s.value) != w.el.slider.as_ref().map(|s| s.value)
                {
                    w.state.drag_value = None;
                }
                w.el = el;
                w.fresh = false;
                w
            }
            None => Widget::new(el),
        });
    }
    for w in keyed.into_iter().flatten().chain(unkeyed) {
        w.take_hosts(removed);
    }
    out
}

/// A shown screen.
#[derive(Debug)]
pub struct Screen<N> {
    pub name: String,
    /// Draw and input order: higher is on top. Ties keep the order screens were shown in.
    pub z: i32,
    /// Blocks pointer input to everything below it (game menus, dialogs).
    pub modal: bool,
    pub area: Area,
    /// Takes keyboard and gamepad navigation (modal screens always do).
    pub nav: bool,
    pub root: Option<Widget<N>>,
    /// Needs laying out (its tree or area changed).
    pub dirty: bool,
}

/// The shown screens and the pointer state.
pub struct Ui<N> {
    /// Sorted by `z` (stable).
    pub screens: Vec<Screen<N>>,
    pub areas: Areas,
    /// Set when interaction states or scroll offsets changed (the host redraws looks).
    pub changed: bool,
    /// Set when every screen needs laying out again (scrolls, areas); screens whose own tree
    /// or area changed are marked [`Screen::dirty`]. See [`Ui::pending_layout`].
    pub needs_layout: bool,
    pub(crate) pointer: input::Pointer,
}

impl<N: Default> Ui<N> {
    pub fn new(areas: Areas) -> Self {
        Self { screens: Vec::new(), areas, changed: false, needs_layout: false, pointer: Default::default() }
    }

    pub fn screen(&self, name: &str) -> Option<&Screen<N>> {
        self.screens.iter().find(|s| s.name == name)
    }

    pub fn screen_mut(&mut self, name: &str) -> Option<&mut Screen<N>> {
        self.screens.iter_mut().find(|s| s.name == name)
    }

    /// Shows a screen (or updates its options); its tree comes with [`Ui::set_tree`].
    pub fn show(&mut self, name: &str, z: i32, modal: bool, area: Area) {
        match self.screen_mut(name) {
            Some(s) => {
                s.z = z;
                s.modal = modal;
                if s.area != area {
                    s.area = area;
                    s.dirty = true;
                }
            }
            None => self.screens.push(Screen {
                name: name.to_string(),
                z,
                modal,
                area,
                nav: false,
                root: None,
                dirty: true,
            }),
        }
        self.screens.sort_by_key(|s| s.z);
    }

    /// Changes a screen's draw and input order (higher is on top).
    pub fn set_z(&mut self, name: &str, z: i32) {
        if let Some(s) = self.screen_mut(name) {
            s.z = z;
            self.screens.sort_by_key(|s| s.z);
            self.changed = true;
        }
    }

    /// Lays a screen out in `rect` (design units). A move that keeps the size shifts the laid
    /// out widgets instead of laying them out again.
    pub fn place(&mut self, name: &str, rect: Rect) {
        let Some(s) = self.screen_mut(name) else { return };
        let old = s.area;
        s.area = Area::Rect(rect);
        match (old, &mut s.root) {
            (Area::Rect(o), Some(root)) if o.size() == rect.size() && !s.dirty => {
                let d = rect.min - o.min;
                if d != Vec2::ZERO {
                    root.translate(d);
                    self.changed = true;
                }
            }
            _ => s.dirty = true,
        }
    }

    /// Whether [`Ui::layout`] has work to do.
    pub fn pending_layout(&self) -> bool {
        self.needs_layout || self.screens.iter().any(|s| s.dirty)
    }

    /// Hides a screen; returns the host data of all its widgets.
    pub fn hide(&mut self, name: &str) -> Vec<N> {
        let mut removed = Vec::new();
        if let Some(i) = self.screens.iter().position(|s| s.name == name) {
            let s = self.screens.remove(i);
            if let Some(root) = s.root {
                root.take_hosts(&mut removed);
            }
            self.pointer.forget_screen(name);
        }
        removed
    }

    /// Replaces a screen's tree, keeping matching widgets. Returns removed host data.
    pub fn set_tree(&mut self, name: &str, root: Element) -> Vec<N> {
        let mut removed = Vec::new();
        if let Some(s) = self.screen_mut(name) {
            let old = s.root.take().into_iter().collect();
            s.root = reconcile(old, vec![root], &mut removed).pop();
            s.dirty = true;
        }
        removed
    }

    /// Lays out the screens that need it (all of them after [`Ui::needs_layout`]).
    pub fn layout(&mut self, measure: &mut dyn Measure) {
        let areas = self.areas;
        let all = std::mem::take(&mut self.needs_layout);
        for s in &mut self.screens {
            if (all || s.dirty)
                && let Some(root) = &mut s.root
            {
                layout::layout_root(root, areas.get(s.area), measure);
                self.changed = true;
            }
            s.dirty = false;
        }
    }

    /// Whether a modal screen is shown.
    pub fn modal(&self) -> bool {
        self.screens.iter().any(|s| s.modal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyed(k: &str, kind: Kind) -> Element {
        Element { key: Some(k.into()), kind, ..Default::default() }
    }

    #[test]
    fn reconcile_keeps_matching_widgets_and_drops_the_rest() {
        let mut removed = Vec::new();
        let first = reconcile::<u32>(
            Vec::new(),
            vec![keyed("a", Kind::Box), keyed("b", Kind::Text), Element::new(Kind::Image)],
            &mut removed,
        );
        let mut first = first;
        for (i, w) in first.iter_mut().enumerate() {
            w.host = i as u32 + 1;
        }
        // Reordered keys keep their host data; the unkeyed image changes kind and is replaced.
        let next = reconcile(
            first,
            vec![keyed("b", Kind::Text), keyed("a", Kind::Box), Element::new(Kind::Text)],
            &mut removed,
        );
        assert_eq!(next.iter().map(|w| w.host).collect::<Vec<_>>(), [2, 1, 0]);
        assert!(!next[0].fresh && next[2].fresh);
        assert_eq!(removed, [3]);
        // A key whose kind changed is a new widget.
        let mut removed = Vec::new();
        let last = reconcile(next, vec![keyed("a", Kind::Text)], &mut removed);
        assert_eq!(last[0].host, 0);
        removed.sort();
        assert_eq!(removed, [0, 1, 2]);
    }
}
