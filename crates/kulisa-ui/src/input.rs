//! Pointer input: hit testing, hover and press states, clicks, slider drags and scrolling.

use kulisa_core::{Rect, Vec2};

use crate::{Kind, Ui, Widget};

/// What the host should run after input.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    Click {
        screen: String,
        callback: u64,
    },
    Change {
        screen: String,
        callback: u64,
        value: f32,
    },
    /// A press landed inside a widget with a press handler (`pos` in design units).
    Press {
        screen: String,
        callback: u64,
        pos: Vec2,
    },
    /// A draggable widget is being dragged: `start` is where the press was.
    Drag {
        screen: String,
        callback: u64,
        phase: DragPhase,
        pos: Vec2,
        start: Vec2,
    },
}

/// The steps of a drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPhase {
    /// The pointer moved past a small slop while pressed.
    Start,
    Move,
    /// Released.
    End,
}

impl DragPhase {
    pub fn name(self) -> &'static str {
        match self {
            DragPhase::Start => "start",
            DragPhase::Move => "move",
            DragPhase::End => "end",
        }
    }
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
    /// The pressed widget is being dragged (its `on_drag` started).
    dragging: bool,
    /// The cursor shape asked for by the widget under the pointer (or the pressed one).
    cursor: Option<String>,
}

impl Pointer {
    pub(crate) fn forget_screen(&mut self, name: &str) {
        let keep = |t: &Option<Target>| t.as_ref().is_some_and(|t| t.screen != name);
        if !keep(&self.hover) {
            self.hover = None;
        }
        if !keep(&self.pressed) {
            self.pressed = None;
            self.dragging = false;
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

/// Distance the pointer must travel before a press on a draggable widget starts a drag.
const DRAG_START: f32 = 4.0;

/// What is under a point in one screen.
#[derive(Default)]
struct Hit {
    /// The topmost interactive widget and the innermost scroll.
    target: Option<Vec<usize>>,
    scroll: Option<Vec<usize>>,
    /// A widget that takes presses (blocking, or with a press handler) is under the point.
    takes: bool,
    /// Widgets with press handlers under the point, outermost first.
    presses: Vec<Vec<usize>>,
    cursor: Option<String>,
}

fn hit<N>(w: &Widget<N>, p: Vec2, clip: Option<Rect>, path: &mut Vec<usize>, out: &mut Hit) {
    let visible = clip.is_none_or(|c| c.contains(p));
    if visible && w.rect.contains(p) {
        if w.el.kind == Kind::Scroll {
            out.scroll = Some(path.clone());
        }
        if w.el.interactive() {
            out.target = Some(path.clone());
        }
        if w.el.takes_presses() {
            out.takes = true;
        }
        if w.el.on_press.is_some() && !w.el.disabled {
            out.presses.push(path.clone());
        }
        if w.el.cursor.is_some() {
            out.cursor = w.el.cursor.clone();
        }
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

/// The result of hit testing the screens at a point.
#[derive(Default)]
struct Picked {
    target: Option<Target>,
    scroll: Option<Target>,
    /// A screen took the point (a widget under it, or a modal screen): it does not reach the
    /// screens below or the game.
    taken: bool,
    presses: Vec<Target>,
    cursor: Option<String>,
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

    /// Hit test from the top screen down, stopping at the first screen that takes the point.
    fn pick(&self, p: Vec2) -> Picked {
        for s in self.screens.iter().rev() {
            let Some(root) = &s.root else { continue };
            let mut out = Hit::default();
            hit(root, p, None, &mut Vec::new(), &mut out);
            let wrap = |path: Vec<usize>| Target { screen: s.name.clone(), path };
            if out.target.is_some() || out.scroll.is_some() || out.takes || s.modal {
                return Picked {
                    target: out.target.map(wrap),
                    scroll: out.scroll.map(wrap),
                    taken: true,
                    presses: out.presses.into_iter().map(wrap).collect(),
                    cursor: out.cursor,
                };
            }
        }
        Picked::default()
    }

    /// The cursor shape the widget under the pointer (or the one being pressed) asks for.
    pub fn cursor(&self) -> Option<&str> {
        self.pointer.cursor.as_deref()
    }

    /// A drag event for the pressed widget, if it is draggable.
    fn drag_event(&mut self, t: &Target, phase: DragPhase, p: Vec2) -> Option<UiEvent> {
        let start = self.pointer.down_at;
        let screen = t.screen.clone();
        let callback = self.widget_mut(t)?.el.on_drag?;
        Some(UiEvent::Drag { screen, callback, phase, pos: p, start })
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
            if let Some(s) = self.screen_mut(&t.screen) {
                s.dirty = true;
            }
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
            let (is_slider, draggable) = self
                .widget_mut(&t)
                .map_or((false, false), |w| (w.el.kind == Kind::Slider, w.el.on_drag.is_some()));
            if is_slider {
                events.extend(self.slider_value(&t, p));
                return events;
            }
            if draggable {
                if self.pointer.dragging {
                    events.extend(self.drag_event(&t, DragPhase::Move, p));
                } else if (p - self.pointer.down_at).length() > DRAG_START {
                    self.pointer.dragging = true;
                    events.extend(self.drag_event(&t, DragPhase::Start, p));
                }
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
        let picked = self.pick(p);
        let target = match &self.pointer.pressed {
            // While pressed, only the pressed widget can be hovered (and keeps its cursor).
            Some(pressed) => picked.target.filter(|t| t == pressed),
            None => {
                self.pointer.cursor = picked.cursor;
                picked.target
            }
        };
        self.set_hover(target);
        events
    }

    /// A press (left button or touch). Returns whether the UI took it, and any events.
    pub fn pointer_down(&mut self, p: Vec2) -> (bool, Vec<UiEvent>) {
        let Picked { target, scroll, taken: blocked, presses, cursor } = self.pick(p);
        self.pointer.down_at = p;
        self.pointer.dragging = false;
        self.pointer.cursor = cursor;
        let mut events = Vec::new();
        // Press handlers run innermost first, before the widget under the press reacts.
        for t in presses.into_iter().rev() {
            let screen = t.screen.clone();
            if let Some(callback) = self.widget_mut(&t).and_then(|w| w.el.on_press) {
                events.push(UiEvent::Press { screen, callback, pos: p });
            }
        }
        // A draggable widget keeps its press: a scroll around it does not take over.
        let draggable =
            target.as_ref().is_some_and(|t| self.widget_mut(t).is_some_and(|w| w.el.on_drag.is_some()));
        self.pointer.scroll_under = if draggable { None } else { scroll.clone() };
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
            return (self.pick(p).taken, events);
        };
        let picked = self.pick(p);
        let now = picked.target;
        self.pointer.cursor = picked.cursor;
        let screen = t.screen.clone();
        let dragged = std::mem::take(&mut self.pointer.dragging);
        if dragged {
            events.extend(self.drag_event(&t, DragPhase::End, p));
        }
        if let Some(w) = self.widget_mut(&t) {
            w.state.pressed = false;
            if w.el.kind != Kind::Slider
                && !dragged
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
        let Picked { scroll, taken: blocked, .. } = self.pick(p);
        if let Some(s) = scroll {
            let horizontal = self.widget_mut(&s).is_some_and(|w| w.el.horizontal);
            let d = -lines * WHEEL_STEP;
            self.scroll_by(&s, if horizontal { Vec2::new(d, 0.0) } else { Vec2::new(0.0, d) });
        }
        blocked
    }

    /// Whether a press at `p` would be taken by the UI (a widget under it, or a modal screen).
    pub fn covers(&self, p: Vec2) -> bool {
        self.pick(p).taken
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
        assert!(ui.pending_layout());
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

    /// A 300x200 window at (100, 100) whose title bar drags and whose body blocks.
    fn window() -> Element {
        let mut bar = Element::new(Kind::Box);
        bar.on_drag = Some(5);
        bar.on_click = Some(6);
        bar.cursor = Some("move".into());
        bar.style.width = Size::Fill(1.0);
        bar.style.height = Size::Px(30.0);
        let mut b = button(7, 0.0);
        b.style.offset = Vec2::new(0.0, 60.0);
        let mut w = root(vec![bar, b]);
        w.block = true;
        w.on_press = Some(4);
        w.style.width = Size::Px(300.0);
        w.style.height = Size::Px(200.0);
        w
    }

    #[test]
    fn blocking_widgets_and_press_handlers_take_presses() {
        let mut ui = ui();
        show(&mut ui, "game", 0, false, root(vec![button(1, 0.0)]));
        ui.show("win", 10, false, Area::Rect(Rect::new(0.0, 0.0, 300.0, 200.0)));
        ui.set_tree("win", window());
        ui.layout(&mut NoText);
        // The window's empty body is over the game's button: it takes the press, and its
        // press handler runs.
        let (taken, ev) = ui.pointer_down(Vec2::new(150.0, 150.0));
        assert!(taken);
        assert_eq!(
            ev,
            vec![UiEvent::Press { screen: "win".into(), callback: 4, pos: Vec2::new(150.0, 150.0) }]
        );
        assert_eq!(ui.pointer_up(Vec2::new(150.0, 150.0)).1, vec![]);
        // A press on the button inside runs the press handler, then the click.
        let (_, ev) = ui.pointer_down(Vec2::new(10.0, 70.0));
        assert_eq!(ev.len(), 1);
        let (_, ev) = ui.pointer_up(Vec2::new(10.0, 70.0));
        assert_eq!(ev, vec![UiEvent::Click { screen: "win".into(), callback: 7 }]);
        // Outside the window the game's screen gets it.
        assert!(!ui.covers(Vec2::new(500.0, 400.0)));
    }

    #[test]
    fn drags_start_past_a_slop_and_clicks_need_no_drag() {
        let mut ui = ui();
        ui.show("win", 10, false, Area::Rect(Rect::new(100.0, 100.0, 300.0, 200.0)));
        ui.set_tree("win", window());
        ui.layout(&mut NoText);
        ui.pointer_move(Vec2::new(120.0, 110.0));
        assert_eq!(ui.cursor(), Some("move"));
        ui.pointer_down(Vec2::new(120.0, 110.0));
        assert!(ui.pointer_move(Vec2::new(121.0, 111.0)).is_empty(), "inside the slop");
        let ev = ui.pointer_move(Vec2::new(140.0, 120.0));
        let drag = |phase, pos: Vec2| UiEvent::Drag {
            screen: "win".into(),
            callback: 5,
            phase,
            pos,
            start: Vec2::new(120.0, 110.0),
        };
        assert_eq!(ev, vec![drag(DragPhase::Start, Vec2::new(140.0, 120.0))]);
        assert_eq!(
            ui.pointer_move(Vec2::new(160.0, 130.0)),
            vec![drag(DragPhase::Move, Vec2::new(160.0, 130.0))]
        );
        // Moving the screen shifts its widgets without a layout pass.
        ui.place("win", Rect::new(140.0, 120.0, 300.0, 200.0));
        assert!(!ui.pending_layout());
        assert_eq!(ui.screens[0].root.as_ref().unwrap().children[0].rect.min, Vec2::new(140.0, 120.0));
        let (_, ev) = ui.pointer_up(Vec2::new(160.0, 130.0));
        assert_eq!(ev, vec![drag(DragPhase::End, Vec2::new(160.0, 130.0))], "a drag is not a click");
        // Without moving, it clicks.
        ui.pointer_down(Vec2::new(150.0, 125.0));
        assert_eq!(
            ui.pointer_up(Vec2::new(150.0, 125.0)).1,
            vec![UiEvent::Click { screen: "win".into(), callback: 6 }]
        );
        // A new size lays the screen out again.
        ui.place("win", Rect::new(140.0, 120.0, 400.0, 200.0));
        assert!(ui.pending_layout());
        ui.layout(&mut NoText);
        assert_eq!(ui.screens[0].root.as_ref().unwrap().rect.size(), Vec2::new(300.0, 200.0));
        assert_eq!(ui.screens[0].root.as_ref().unwrap().children[0].rect.size().x, 300.0);
    }

    #[test]
    fn clipped_boxes_hide_children_from_the_pointer() {
        let mut ui = ui();
        let mut b = root(vec![button(1, 150.0)]);
        b.clip = true;
        b.style.width = Size::Px(100.0);
        b.style.height = Size::Px(100.0);
        show(&mut ui, "a", 0, false, root(vec![b]));
        assert_eq!(
            ui.screens[0].root.as_ref().unwrap().children[0].clip,
            Some(Rect::new(0.0, 0.0, 100.0, 100.0))
        );
        assert!(!ui.covers(Vec2::new(200.0, 10.0)), "the button is outside the clip");
        ui.set_z("a", 5);
        assert_eq!(ui.screens[0].z, 5);
    }
}
