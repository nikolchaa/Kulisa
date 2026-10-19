//! Keyboard and gamepad navigation: one focused widget at a time, moved spatially.
//!
//! Only the topmost navigable screen takes part (a screen with `nav` set, or a modal one), so a
//! menu over the game is navigated without reaching widgets underneath. A focused widget
//! looks hovered; moving the mouse drops the focus.

use kulisa_core::{Rect, Vec2};

use crate::{Direction, Kind, Ui, UiEvent, Widget};

/// What a direction key did.
#[derive(Clone, Debug, PartialEq)]
pub enum Nav {
    /// Focus moved to another widget.
    Moved,
    /// A focused slider changed (the events carry its new value).
    Adjusted(Vec<UiEvent>),
    /// Nothing to focus that way, but a scroll moved.
    Scrolled,
    /// Nothing happened.
    Blocked,
}

/// Fraction of a slider's range one key press moves it when it has no `step`.
const SLIDER_STEPS: f32 = 20.0;
/// Design units a key press scrolls when nothing is focusable.
const SCROLL_STEP: f32 = 120.0;

fn focusables<N>(w: &Widget<N>, path: &mut Vec<usize>, out: &mut Vec<(Vec<usize>, Rect)>) {
    if w.el.interactive() {
        out.push((path.clone(), w.rect));
    }
    for (i, c) in w.children.iter().enumerate() {
        path.push(i);
        focusables(c, path, out);
        path.pop();
    }
}

fn focused_path<N>(w: &Widget<N>, path: &mut Vec<usize>) -> bool {
    if w.state.focus {
        return true;
    }
    for (i, c) in w.children.iter().enumerate() {
        path.push(i);
        if focused_path(c, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn at<'a, N>(root: &'a mut Widget<N>, path: &[usize]) -> Option<&'a mut Widget<N>> {
    let mut w = root;
    for &i in path {
        w = w.children.get_mut(i)?;
    }
    Some(w)
}

fn score(cur: Rect, cand: Rect, dir: Direction) -> Option<f32> {
    let (c, n) = ((cur.min + cur.max) * 0.5, (cand.min + cand.max) * 0.5);
    let (along, across) = match dir {
        Direction::Right => (n.x - c.x, n.y - c.y),
        Direction::Left => (c.x - n.x, n.y - c.y),
        Direction::Down => (n.y - c.y, n.x - c.x),
        Direction::Up => (c.y - n.y, n.x - c.x),
    };
    (along > 1.0).then(|| along + 2.0 * across.abs())
}

impl<N: Default> Ui<N> {
    /// Marks a screen as navigable with the keyboard and gamepad (modal screens always are).
    pub fn set_nav(&mut self, name: &str, nav: bool) {
        if let Some(s) = self.screen_mut(name) {
            s.nav = nav;
        }
    }

    /// The topmost screen focus works in.
    fn nav_screen(&self) -> Option<usize> {
        self.screens.iter().rposition(|s| s.root.is_some() && (s.nav || s.modal))
    }

    /// Whether a screen takes keyboard and gamepad navigation now.
    pub fn nav_active(&self) -> bool {
        self.nav_screen().is_some()
    }

    /// The screen and path of the focused widget, in the navigable screen.
    fn focus_target(&self) -> Option<(usize, Vec<usize>)> {
        let i = self.nav_screen()?;
        let mut path = Vec::new();
        focused_path(self.screens[i].root.as_ref()?, &mut path).then_some((i, path))
    }

    /// The screen name and widget `id` of the focused widget.
    pub fn focused(&self) -> Option<(String, Option<String>)> {
        let (i, path) = self.focus_target()?;
        let mut w = self.screens[i].root.as_ref()?;
        for &k in &path {
            w = &w.children[k];
        }
        Some((self.screens[i].name.clone(), w.el.id.clone()))
    }

    /// Drops the focus everywhere.
    pub fn clear_focus(&mut self) {
        for s in &mut self.screens {
            if let Some(r) = &mut s.root {
                clear(r);
            }
        }
        self.changed = true;
    }

    fn set_focus(&mut self, screen: usize, path: &[usize]) {
        self.clear_focus();
        let Some(root) = self.screens[screen].root.as_mut() else { return };
        if let Some(w) = at(root, path) {
            w.state.focus = true;
        }
        self.reveal(screen, path);
    }

    /// Scrolls the scroll viewports around the widget at `path` so it is on show.
    fn reveal(&mut self, screen: usize, path: &[usize]) {
        let Some(root) = self.screens[screen].root.as_mut() else { return };
        let target = match at(root, path) {
            Some(w) => w.rect,
            None => return,
        };
        for depth in 0..path.len() {
            let Some(w) = at(root, &path[..depth]) else { continue };
            if w.el.kind != Kind::Scroll {
                continue;
            }
            let view = w.rect;
            let mut d = Vec2::ZERO;
            if target.min.y < view.min.y {
                d.y = target.min.y - view.min.y;
            } else if target.max.y > view.max.y {
                d.y = target.max.y - view.max.y;
            }
            if target.min.x < view.min.x {
                d.x = target.min.x - view.min.x;
            } else if target.max.x > view.max.x {
                d.x = target.max.x - view.max.x;
            }
            if d != Vec2::ZERO {
                let max = (w.content - w.rect.size() + w.el.style.padding.size()).max(Vec2::ZERO);
                w.scroll = (w.scroll + d).clamp(Vec2::ZERO, max);
                self.needs_layout = true;
            }
        }
    }

    /// Focuses the widget marked `autofocus`, or else the first focusable one, when nothing
    /// is focused. Returns whether something is focused afterwards.
    pub fn focus_default(&mut self) -> bool {
        let Some(i) = self.nav_screen() else { return false };
        if self.focus_target().is_some() {
            return true;
        }
        let mut all = Vec::new();
        if let Some(root) = &self.screens[i].root {
            focusables(root, &mut Vec::new(), &mut all);
        }
        let root = self.screens[i].root.as_ref();
        let preferred = all.iter().position(|(p, _)| {
            let mut w = root.expect("screen has a tree");
            for &k in p {
                w = &w.children[k];
            }
            w.el.autofocus
        });
        let Some((path, _)) = all.get(preferred.unwrap_or(0)).cloned() else { return false };
        self.set_focus(i, &path);
        true
    }

    /// Focuses the widget with `id` on the named screen.
    pub fn focus_id(&mut self, screen: &str, id: &str) -> bool {
        let Some(i) = self.screens.iter().position(|s| s.name == screen) else { return false };
        let mut found = None;
        fn find<N>(w: &Widget<N>, id: &str, path: &mut Vec<usize>) -> bool {
            if w.el.id.as_deref() == Some(id) && w.el.interactive() {
                return true;
            }
            for (k, c) in w.children.iter().enumerate() {
                path.push(k);
                if find(c, id, path) {
                    return true;
                }
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        if self.screens[i].root.as_ref().is_some_and(|r| find(r, id, &mut path)) {
            found = Some(path);
        }
        match found {
            Some(p) => {
                self.set_focus(i, &p);
                true
            }
            None => false,
        }
    }

    /// A direction key or d-pad press. Without focus it focuses the default widget; a
    /// focused slider changes along its axis; otherwise focus jumps to the nearest widget that
    /// way, or the enclosing (or only) scroll moves.
    pub fn focus_move(&mut self, dir: Direction) -> Nav {
        let Some(i) = self.nav_screen() else { return Nav::Blocked };
        let Some((_, path)) = self.focus_target() else {
            return if self.focus_default() { Nav::Moved } else { self.scroll_any(i, dir) };
        };
        // A slider takes the keys along its own axis.
        let root = self.screens[i].root.as_mut().expect("screen has a tree");
        let w = at(root, &path).expect("focused path exists");
        if let Some(spec) = w.el.slider.clone() {
            let along = if spec.vertical {
                matches!(dir, Direction::Up | Direction::Down)
            } else {
                matches!(dir, Direction::Left | Direction::Right)
            };
            if along {
                let up = matches!(dir, Direction::Right | Direction::Up);
                let step = if spec.step > 0.0 { spec.step } else { (spec.max - spec.min) / SLIDER_STEPS };
                let now = w.state.drag_value.unwrap_or(spec.value);
                let next = if up { now + step } else { now - step };
                let next = next.clamp(spec.min.min(spec.max), spec.max.max(spec.min));
                if next == now {
                    return Nav::Blocked;
                }
                w.state.drag_value = Some(next);
                let callback = w.el.on_change;
                let screen = self.screens[i].name.clone();
                self.changed = true;
                let events = callback.map(|cb| UiEvent::Change { screen, callback: cb, value: next });
                return Nav::Adjusted(events.into_iter().collect());
            }
        }
        let cur = w.rect;
        let mut all = Vec::new();
        focusables(self.screens[i].root.as_ref().expect("screen has a tree"), &mut Vec::new(), &mut all);
        let best = all
            .into_iter()
            .filter(|(p, _)| *p != path)
            .filter_map(|(p, r)| score(cur, r, dir).map(|s| (s, p)))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match best {
            Some((_, p)) => {
                self.set_focus(i, &p);
                Nav::Moved
            }
            None => self.scroll_any(i, dir),
        }
    }

    /// Scrolls the first scroll viewport of the screen (the history log, say).
    fn scroll_any(&mut self, screen: usize, dir: Direction) -> Nav {
        let horizontal = self.first_scroll(screen).is_some_and(|s| s.el.horizontal);
        let d = match (dir, horizontal) {
            (Direction::Up, false) | (Direction::Left, true) => -SCROLL_STEP,
            (Direction::Down, false) | (Direction::Right, true) => SCROLL_STEP,
            _ => return Nav::Blocked,
        };
        if self.scroll_first(screen, |_| d) { Nav::Scrolled } else { Nav::Blocked }
    }

    fn first_scroll(&mut self, screen: usize) -> Option<&mut Widget<N>> {
        fn first<N>(w: &mut Widget<N>) -> Option<&mut Widget<N>> {
            if w.el.kind == Kind::Scroll {
                return Some(w);
            }
            w.children.iter_mut().find_map(first)
        }
        first(self.screens[screen].root.as_mut()?)
    }

    /// Scrolls the first scroll by `delta(viewport size)` along its axis; false if it did not
    /// move (or there is none).
    fn scroll_first(&mut self, screen: usize, delta: impl FnOnce(f32) -> f32) -> bool {
        let Some(s) = self.first_scroll(screen) else { return false };
        let along = if s.el.horizontal { s.rect.size().x } else { s.rect.size().y };
        let d = delta(along);
        let max = (s.content - s.rect.size() + s.el.style.padding.size()).max(Vec2::ZERO);
        let before = s.scroll;
        s.scroll = if s.el.horizontal {
            Vec2::new((s.scroll.x + d).clamp(0.0, max.x), s.scroll.y)
        } else {
            Vec2::new(s.scroll.x, (s.scroll.y + d).clamp(0.0, max.y))
        };
        if s.scroll == before {
            return false;
        }
        self.needs_layout = true;
        self.changed = true;
        true
    }

    /// Page Up and Page Down (`pages` of -1 and 1): scrolls the navigable screen's scroll by
    /// that many viewports. Returns whether there was a scroll to move.
    pub fn scroll_pages(&mut self, pages: f32) -> bool {
        let Some(i) = self.nav_screen() else { return false };
        self.first_scroll(i).is_some() && {
            self.scroll_first(i, |view| view * 0.9 * pages);
            true
        }
    }

    /// Scrolls the navigable screen's scroll by `amount` design units (a thumb stick).
    pub fn scroll_amount(&mut self, amount: f32) -> bool {
        let Some(i) = self.nav_screen() else { return false };
        self.scroll_first(i, |_| amount)
    }

    /// The confirm key or A button: clicks the focused button. `None` when nothing is focused
    /// (the host then focuses the default widget instead).
    pub fn focus_activate(&mut self) -> Option<Vec<UiEvent>> {
        let (i, path) = self.focus_target()?;
        let screen = self.screens[i].name.clone();
        let w = at(self.screens[i].root.as_mut()?, &path)?;
        self.changed = true;
        Some(match (w.el.kind, w.el.on_click) {
            (Kind::Slider, _) | (_, None) => Vec::new(),
            (_, Some(callback)) => vec![UiEvent::Click { screen, callback }],
        })
    }
}

fn clear<N>(w: &mut Widget<N>) {
    w.state.focus = false;
    for c in &mut w.children {
        clear(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Area, Areas, Element, Layout, Size, SliderSpec};

    struct NoText;
    impl crate::Measure for NoText {
        fn text(&mut self, _: &crate::TextLook, _: Option<f32>) -> Vec2 {
            Vec2::ZERO
        }
    }

    fn button(cb: u64, x: f32, y: f32) -> Element {
        let mut b = Element::new(Kind::Box);
        b.button = true;
        b.on_click = Some(cb);
        b.style.width = Size::Px(100.0);
        b.style.height = Size::Px(50.0);
        b.style.offset = Vec2::new(x, y);
        b
    }

    fn ui_with(nav: bool, children: Vec<Element>) -> Ui<()> {
        let area = Rect::new(0.0, 0.0, 1000.0, 500.0);
        let mut ui = Ui::new(Areas { screen: area, safe: area, design: area });
        ui.show("menu", 0, false, Area::Screen);
        ui.set_nav("menu", nav);
        let mut root = Element::new(Kind::Box).with_children(children);
        root.style.layout = Layout::Overlay;
        ui.set_tree("menu", root);
        ui.layout(&mut NoText);
        ui
    }

    fn focus_index(ui: &Ui<()>) -> Option<usize> {
        ui.screens[0].root.as_ref().unwrap().children.iter().position(|c| c.state.focus)
    }

    #[test]
    fn moves_between_buttons_in_the_direction_pressed() {
        // A 2x2 grid: (0,0) (200,0) / (0,100) (200,100).
        let mut ui = ui_with(
            true,
            vec![button(1, 0.0, 0.0), button(2, 200.0, 0.0), button(3, 0.0, 100.0), button(4, 200.0, 100.0)],
        );
        assert_eq!(focus_index(&ui), None);
        assert_eq!(ui.focus_move(Direction::Down), Nav::Moved, "the first press focuses the first widget");
        assert_eq!(focus_index(&ui), Some(0));
        assert_eq!(ui.focus_move(Direction::Right), Nav::Moved);
        assert_eq!(focus_index(&ui), Some(1));
        assert_eq!(ui.focus_move(Direction::Down), Nav::Moved);
        assert_eq!(focus_index(&ui), Some(3));
        assert_eq!(ui.focus_move(Direction::Down), Nav::Blocked, "the bottom edge");
        assert_eq!(focus_index(&ui), Some(3));
        ui.focus_move(Direction::Left);
        ui.focus_move(Direction::Up);
        assert_eq!(focus_index(&ui), Some(0));
        assert_eq!(ui.focused(), Some(("menu".into(), None)));
        // Activating clicks it.
        assert_eq!(ui.focus_activate(), Some(vec![UiEvent::Click { screen: "menu".into(), callback: 1 }]));
        ui.clear_focus();
        assert_eq!(ui.focus_activate(), None);
    }

    #[test]
    fn prefers_autofocus_and_ignores_screens_without_nav() {
        let mut second = button(2, 200.0, 0.0);
        second.autofocus = true;
        let mut ui = ui_with(true, vec![button(1, 0.0, 0.0), second]);
        assert!(ui.focus_default());
        assert_eq!(focus_index(&ui), Some(1));
        let mut ui = ui_with(false, vec![button(1, 0.0, 0.0)]);
        assert!(!ui.nav_active());
        assert_eq!(ui.focus_move(Direction::Down), Nav::Blocked);
        // Modal screens are navigable without asking.
        ui.show("menu", 0, true, Area::Screen);
        assert!(ui.nav_active());
    }

    #[test]
    fn disabled_buttons_are_skipped_and_sliders_take_their_axis() {
        let mut off = button(2, 200.0, 0.0);
        off.disabled = true;
        let mut slider = Element::new(Kind::Slider);
        slider.slider = Some(SliderSpec {
            value: 0.5,
            min: 0.0,
            max: 1.0,
            step: 0.25,
            vertical: false,
            thumb_size: 20.0,
        });
        slider.on_change = Some(9);
        slider.style.width = Size::Px(200.0);
        slider.style.height = Size::Px(30.0);
        slider.style.offset = Vec2::new(0.0, 100.0);
        let mut ui = ui_with(true, vec![button(1, 0.0, 0.0), off, slider]);
        ui.focus_move(Direction::Down);
        ui.focus_move(Direction::Right);
        assert_eq!(focus_index(&ui), Some(2), "the disabled button is no target; the slider is");
        assert_eq!(
            ui.focus_move(Direction::Right),
            Nav::Adjusted(vec![UiEvent::Change { screen: "menu".into(), callback: 9, value: 0.75 }])
        );
        ui.focus_move(Direction::Right);
        assert_eq!(ui.focus_move(Direction::Right), Nav::Blocked, "clamped at the end");
        assert_eq!(
            ui.focus_move(Direction::Left),
            Nav::Adjusted(vec![UiEvent::Change { screen: "menu".into(), callback: 9, value: 0.75 }])
        );
        assert_eq!(ui.focus_move(Direction::Up), Nav::Moved, "across the slider's axis focus moves on");
        assert_eq!(focus_index(&ui), Some(0));
    }

    #[test]
    fn scrolls_when_nothing_is_focusable_and_reveals_focus() {
        // A scroll of 20 buttons: focus moves down it and the viewport follows.
        let items: Vec<Element> = (0..20).map(|i| button(i, 0.0, 0.0)).collect();
        let mut sc = Element::new(Kind::Scroll).with_children(items);
        sc.style.width = Size::Px(100.0);
        sc.style.height = Size::Px(200.0);
        let mut ui = ui_with(true, vec![sc]);
        ui.focus_move(Direction::Down);
        for _ in 0..8 {
            ui.focus_move(Direction::Down);
            ui.layout(&mut NoText);
        }
        let scroll = ui.screens[0].root.as_ref().unwrap().children[0].scroll.y;
        assert!(scroll > 0.0, "the viewport followed the focus: {scroll}");

        // Text only: the arrows scroll the log.
        let lines: Vec<Element> = (0..20)
            .map(|_| {
                let mut e = Element::new(Kind::Box);
                e.style.width = Size::Px(100.0);
                e.style.height = Size::Px(50.0);
                e
            })
            .collect();
        let mut sc = Element::new(Kind::Scroll).with_children(lines);
        sc.style.width = Size::Px(100.0);
        sc.style.height = Size::Px(200.0);
        let mut ui = ui_with(true, vec![sc]);
        assert_eq!(ui.focus_move(Direction::Down), Nav::Scrolled);
        assert_eq!(ui.screens[0].root.as_ref().unwrap().children[0].scroll.y, SCROLL_STEP);
        assert_eq!(ui.focus_move(Direction::Up), Nav::Scrolled);
        assert_eq!(ui.focus_move(Direction::Up), Nav::Blocked);
    }
}
