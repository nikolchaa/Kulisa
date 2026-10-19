//! Pointer input: hit testing, hover and press states, clicks, slider drags and scrolling.

use kulisa_core::{Rect, Vec2};

use crate::{Kind, Ui, Widget};

/// What the host should run after input.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    Click { screen: String, callback: u64 },
    Change { screen: String, callback: u64, value: f32 },
}

/// A widget, by screen and child-index path from the root.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Target {
    screen: String,
    path: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct Pointer {
    hover: Option<Target>,
    pressed: Option<Target>,
    /// A scroll being dragged, and the last pointer position.
    scrolling: Option<(Target, Vec2)>,
    /// The scroll under the press, which takes over if the pointer moves far enough.
    scroll_under: Option<Target>,
    down_at: Vec2,
}

impl Pointer {
    pub(crate) fn forget_screen(&mut self, name: &str) {
        let keep = |t: &Option<Target>| t.as_ref().is_some_and(|t| t.screen != name);
        if !keep(&self.hover) {
            self.hover = None;
        }
        if !keep(&self.pressed) {
            self.pressed = None;
        }
        if !keep(&self.scroll_under) {
            self.scroll_under = None;
        }
        if self.scrolling.as_ref().is_some_and(|(t, _)| t.screen == name) {
            self.scrolling = None;
        }
    }
}

/// Distance the pointer must travel before a press inside a scroll becomes a drag.
const DRAG_SLOP: f32 = 12.0;
/// Design units scrolled per wheel line.
const WHEEL_STEP: f32 = 60.0;

/// The topmost interactive widget and innermost scroll at `p`.
fn hit<N>(
    w: &Widget<N>,
    p: Vec2,
    clip: Option<Rect>,
    path: &mut Vec<usize>,
    out: &mut (Option<Vec<usize>>, Option<Vec<usize>>),
) {
    let visible = clip.is_none_or(|c| c.contains(p));
    let inside = visible && w.rect.contains(p);
    if inside && w.el.kind == Kind::Scroll {
        out.1 = Some(path.clone());
    }
    if inside && w.el.interactive() {
        out.0 = Some(path.clone());
    }
    let clip = match (clip, w.clip) {
        (Some(a), Some(b)) => Some(a.intersect(&b)),
        (a, b) => a.or(b),
    };
    for (i, c) in w.children.iter().enumerate() {
        path.push(i);
        hit(c, p, clip, path, out);
        path.pop();
    }
}

fn walk_path<'a, N>(root: &'a mut Widget<N>, path: &[usize]) -> Option<&'a mut Widget<N>> {
    let mut w = root;
    for &i in path {
        w = w.children.get_mut(i)?;
    }
    Some(w)
}

impl<N: Default> Ui<N> {
    fn widget_mut(&mut self, t: &Target) -> Option<&mut Widget<N>> {
        let s = self.screens.iter_mut().find(|s| s.name == t.screen)?;
        walk_path(s.root.as_mut()?, &t.path)
    }

    /// Hit test from the top screen down. Returns the interactive target, the scroll target
    /// and whether a modal screen (or a hit) stops the pointer from reaching the game.
    fn pick(&self, p: Vec2) -> (Option<Target>, Option<Target>, bool) {
        for s in self.screens.iter().rev() {
            let Some(root) = &s.root else { continue };
            let mut out = (None, None);
            hit(root, p, None, &mut Vec::new(), &mut out);
            let wrap = |path: Option<Vec<usize>>| path.map(|path| Target { screen: s.name.clone(), path });
            if out.0.is_some() || out.1.is_some() || s.modal {
                return (wrap(out.0), wrap(out.1), true);
            }
        }
        (None, None, false)
    }

    fn slider_value(&mut self, t: &Target, p: Vec2) -> Option<UiEvent> {
        let screen = t.screen.clone();
        let w = self.widget_mut(t)?;
        let spec = w.el.slider.clone()?;
        let r = w.rect;
        let half = spec.thumb_size * 0.5;
        let frac = if spec.vertical {
            1.0 - (p.y - r.min.y - half) / (r.size().y - spec.thumb_size).max(1.0)
        } else {
            (p.x - r.min.x - half) / (r.size().x - spec.thumb_size).max(1.0)
        };
        let v = spec.value_at(frac);
        let old = w.state.drag_value.unwrap_or(spec.value);
        w.state.drag_value = Some(v);
        let cb = w.el.on_change;
        self.changed = true;
        (v != old).then_some(UiEvent::Change { screen, callback: cb?, value: v })
    }

    fn set_hover(&mut self, t: Option<Target>) {
        if self.pointer.hover == t {
            return;
        }
        if let Some(old) = self.pointer.hover.take()
            && let Some(w) = self.widget_mut(&old)
        {
            w.state.hover = false;
        }
        if let Some(new) = &t
            && let Some(w) = self.widget_mut(new)
        {
            w.state.hover = true;
        }
        self.pointer.hover = t;
        self.changed = true;
    }

    fn scroll_by(&mut self, t: &Target, delta: Vec2) {
        if let Some(w) = self.widget_mut(t) {
            let max = (w.content - w.rect.size() + w.el.style.padding.size()).max(Vec2::ZERO);
            w.scroll = (w.scroll + delta).clamp(Vec2::ZERO, max);
            self.changed = true;
            self.needs_layout = true;
        }
    }

    /// The pointer moved (design units).
    pub fn pointer_move(&mut self, p: Vec2) -> Vec<UiEvent> {
        let mut events = Vec::new();
        if let Some((t, last)) = self.pointer.scrolling.clone() {
            let w = self.widget_mut(&t).map(|w| w.el.horizontal);
            let d = last - p;
            let d = if w == Some(true) { Vec2::new(d.x, 0.0) } else { Vec2::new(0.0, d.y) };
            self.scroll_by(&t, d);
            self.pointer.scrolling = Some((t, p));
            return events;
        }
        if let Some(t) = self.pointer.pressed.clone() {
            let is_slider = self.widget_mut(&t).is_some_and(|w| w.el.kind == Kind::Slider);
            if is_slider {
                events.extend(self.slider_value(&t, p));
                return events;
            }
            // A press inside a scroll that travels far becomes a drag of the scroll.
            if let Some(s) = self.pointer.scroll_under.clone()
                && (p - self.pointer.down_at).length() > DRAG_SLOP
            {
                if let Some(w) = self.widget_mut(&t) {
                    w.state.pressed = false;
                }
                self.pointer.pressed = None;
                let horizontal = self.widget_mut(&s).is_some_and(|w| w.el.horizontal);
                let d = self.pointer.down_at - p;
                self.scroll_by(&s, if horizontal { Vec2::new(d.x, 0.0) } else { Vec2::new(0.0, d.y) });
                self.pointer.scrolling = Some((s, p));
                return events;
            }
        }
        let (target, _, _) = self.pick(p);
        let target = match &self.pointer.pressed {
            // While pressed, only the pressed widget can be hovered.
            Some(pressed) => target.filter(|t| t == pressed),
            None => target,
        };
        self.set_hover(target);
        events
    }

    /// A press (left button or touch). Returns whether the UI took it, and any events.
    pub fn pointer_down(&mut self, p: Vec2) -> (bool, Vec<UiEvent>) {
        let (target, scroll, blocked) = self.pick(p);
        self.pointer.down_at = p;
        self.pointer.scroll_under = scroll.clone();
        let mut events = Vec::new();
        match target {
            Some(t) => {
                if let Some(w) = self.widget_mut(&t) {
                    w.state.pressed = true;
                    w.state.hover = true;
                }
                self.changed = true;
                self.pointer.hover = Some(t.clone());
                self.pointer.pressed = Some(t.clone());
                events.extend(self.slider_value(&t, p));
            }
            None => {
                if let Some(s) = scroll {
                    self.pointer.scrolling = Some((s, p));
                }
            }
        }
        (blocked, events)
    }

    /// The press ended. Returns whether the UI took it, and any clicks.
    pub fn pointer_up(&mut self, p: Vec2) -> (bool, Vec<UiEvent>) {
        let mut events = Vec::new();
        self.pointer.scroll_under = None;
        if self.pointer.scrolling.take().is_some() {
            return (true, events);
        }
        let Some(t) = self.pointer.pressed.take() else {
            return (self.pick(p).2, events);
        };
        let (now, _, _) = self.pick(p);
        let screen = t.screen.clone();
        if let Some(w) = self.widget_mut(&t) {
            w.state.pressed = false;
            if w.el.kind != Kind::Slider
                && now.as_ref() == Some(&t)
                && let Some(cb) = w.el.on_click
            {
                events.push(UiEvent::Click { screen, callback: cb });
            }
        }
        self.changed = true;
        if now.as_ref() != Some(&t) {
            self.set_hover(now);
        }
        (true, events)
    }

    /// The wheel turned by `lines` (positive is up). Returns whether the UI took it.
    pub fn wheel(&mut self, p: Vec2, lines: f32) -> bool {
        let (_, scroll, blocked) = self.pick(p);
        if let Some(s) = scroll {
            let horizontal = self.widget_mut(&s).is_some_and(|w| w.el.horizontal);
            let d = -lines * WHEEL_STEP;
            self.scroll_by(&s, if horizontal { Vec2::new(d, 0.0) } else { Vec2::new(0.0, d) });
        }
        blocked
    }

    /// The pointer left the window.
    pub fn pointer_left(&mut self) {
        self.set_hover(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Area, Areas, Element, Layout, Size, SliderSpec};

    fn button(cb: u64, x: f32) -> Element {
        let mut b = Element::new(Kind::Box);
        b.button = true;
        b.on_click = Some(cb);
        b.style.width = Size::Px(100.0);
        b.style.height = Size::Px(50.0);
        b.style.offset = Vec2::new(x, 0.0);
        b
    }

    struct NoText;
    impl crate::Measure for NoText {
        fn text(&mut self, _: &crate::TextLook, _: Option<f32>) -> Vec2 {
            Vec2::ZERO
        }
    }

    fn ui() -> Ui<()> {
        let area = Rect::new(0.0, 0.0, 1000.0, 500.0);
        Ui::new(Areas { screen: area, safe: area, design: area })
    }

    fn show(ui: &mut Ui<()>, name: &str, z: i32, modal: bool, root: Element) {
        ui.show(name, z, modal, Area::Screen);
        ui.set_tree(name, root);
        ui.layout(&mut NoText);
    }

    fn root(children: Vec<Element>) -> Element {
        let mut e = Element::new(Kind::Box).with_children(children);
        e.style.layout = Layout::Overlay;
        e
    }

    #[test]
    fn clicks_need_press_and_release_on_the_same_button() {
        let mut ui = ui();
        show(&mut ui, "a", 0, false, root(vec![button(1, 0.0), button(2, 200.0)]));
        assert_eq!(ui.pointer_down(Vec2::new(10.0, 10.0)), (true, vec![]));
        assert!(ui.screens[0].root.as_ref().unwrap().children[0].state.pressed);
        let (taken, ev) = ui.pointer_up(Vec2::new(20.0, 10.0));
        assert!(taken);
        assert_eq!(ev, vec![UiEvent::Click { screen: "a".into(), callback: 1 }]);
        // Released elsewhere: no click.
        ui.pointer_down(Vec2::new(10.0, 10.0));
        assert_eq!(ui.pointer_up(Vec2::new(250.0, 10.0)).1, vec![]);
        // Empty space is not taken; the game gets it.
        assert!(!ui.pointer_down(Vec2::new(500.0, 400.0)).0);
        assert!(!ui.pointer_up(Vec2::new(500.0, 400.0)).0);
        // Hover follows the pointer.
        ui.pointer_move(Vec2::new(210.0, 10.0));
        assert!(ui.screens[0].root.as_ref().unwrap().children[1].state.hover);
    }

    #[test]
    fn modal_screens_block_what_is_below() {
        let mut ui = ui();
        show(&mut ui, "game", 0, false, root(vec![button(1, 0.0)]));
        show(&mut ui, "menu", 10, true, root(vec![button(2, 300.0)]));
        let (taken, _) = ui.pointer_down(Vec2::new(10.0, 10.0));
        assert!(taken, "the modal takes clicks outside its buttons");
        assert_eq!(ui.pointer_up(Vec2::new(10.0, 10.0)).1, vec![], "the button below gets nothing");
        assert!(ui.wheel(Vec2::new(10.0, 10.0), 1.0));
        ui.hide("menu");
        ui.pointer_down(Vec2::new(10.0, 10.0));
        assert_eq!(ui.pointer_up(Vec2::new(10.0, 10.0)).1.len(), 1);
    }

    #[test]
    fn sliders_report_snapped_values_while_dragged() {
        let mut ui = ui();
        let mut s = Element::new(Kind::Slider);
        s.slider = Some(SliderSpec {
            value: 0.0,
            min: 0.0,
            max: 100.0,
            step: 10.0,
            vertical: false,
            thumb_size: 20.0,
        });
        s.on_change = Some(7);
        s.style.width = Size::Px(220.0);
        show(&mut ui, "prefs", 0, false, root(vec![s]));
        let (_, ev) = ui.pointer_down(Vec2::new(110.0, 10.0));
        assert_eq!(ev, vec![UiEvent::Change { screen: "prefs".into(), callback: 7, value: 50.0 }]);
        let ev = ui.pointer_move(Vec2::new(500.0, 10.0));
        assert_eq!(ev, vec![UiEvent::Change { screen: "prefs".into(), callback: 7, value: 100.0 }]);
        assert!(ui.pointer_move(Vec2::new(600.0, 10.0)).is_empty(), "no event when the value stays");
        assert_eq!(ui.pointer_up(Vec2::new(600.0, 10.0)).1, vec![], "sliders do not click");
        assert_eq!(ui.screens[0].root.as_ref().unwrap().children[0].state.drag_value, Some(100.0));
    }

    #[test]
    fn scrolls_with_the_wheel_and_by_dragging() {
        let mut ui = ui();
        let items: Vec<Element> = (0..20).map(|i| button(i, 0.0)).collect();
        let mut sc = Element::new(Kind::Scroll).with_children(items);
        sc.style.height = Size::Px(200.0);
        sc.style.width = Size::Px(100.0);
        show(&mut ui, "log", 0, false, root(vec![sc]));
        let scroll = |ui: &Ui<()>| ui.screens[0].root.as_ref().unwrap().children[0].scroll.y;
        assert!(ui.wheel(Vec2::new(50.0, 50.0), -2.0));
        assert_eq!(scroll(&ui), 120.0);
        assert!(ui.needs_layout);
        ui.layout(&mut NoText);
        ui.wheel(Vec2::new(50.0, 50.0), 10.0);
        assert_eq!(scroll(&ui), 0.0, "clamped at the start");
        // A press on a button inside, then a long move, scrolls instead of clicking.
        ui.pointer_down(Vec2::new(50.0, 150.0));
        ui.pointer_move(Vec2::new(50.0, 140.0));
        ui.pointer_move(Vec2::new(50.0, 100.0));
        ui.pointer_move(Vec2::new(50.0, 50.0));
        assert_eq!(scroll(&ui), 100.0, "follows the pointer once past the slop");
        assert_eq!(ui.pointer_up(Vec2::new(50.0, 50.0)), (true, vec![]));
    }
}
