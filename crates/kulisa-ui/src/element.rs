//! Widget descriptions: what a screen function returns.

use kulisa_core::{Color, TextureId, Vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// A container (stack, grid or overlay), optionally a button.
    #[default]
    Box,
    Text,
    Image,
    Slider,
    /// A clipped viewport whose children scroll.
    Scroll,
    /// A scene node made by the game, placed by the layout.
    Node,
}

/// A length along one axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Size {
    /// Fits the content.
    #[default]
    Auto,
    Px(f32),
    /// A fraction (0..1) of the parent's content box.
    Percent(f32),
    /// Takes the space left in a stack, shared by weight; elsewhere the parent's whole size.
    Fill(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sides {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Sides {
    pub fn all(v: f32) -> Self {
        Self { left: v, top: v, right: v, bottom: v }
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.left + self.right, self.top + self.bottom)
    }

    pub fn start(&self) -> Vec2 {
        Vec2::new(self.left, self.top)
    }
}

/// How a box places its children.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Layout {
    /// Children on top of each other, each placed by its [`Style::anchor`].
    #[default]
    Overlay,
    /// A row (`horizontal`) or column.
    Stack { horizontal: bool },
    /// Rows of `columns` equal cells.
    Grid { columns: u32 },
}

/// Cross-axis placement in stacks (and in grid cells).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    /// Auto-sized children take the whole cross size.
    Stretch,
}

/// Main-axis placement in stacks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    /// Equal space between children.
    Between,
}

/// A point of the parent (and of the child) to line up: (0, 0) top left, (1, 1) bottom
/// right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor(pub Vec2);

impl Default for Anchor {
    fn default() -> Self {
        Anchor(Vec2::ZERO)
    }
}

impl Anchor {
    pub fn named(name: &str) -> Option<Anchor> {
        let v = match name {
            "top_left" => (0.0, 0.0),
            "top" => (0.5, 0.0),
            "top_right" => (1.0, 0.0),
            "left" => (0.0, 0.5),
            "center" => (0.5, 0.5),
            "right" => (1.0, 0.5),
            "bottom_left" => (0.0, 1.0),
            "bottom" => (0.5, 1.0),
            "bottom_right" => (1.0, 1.0),
            _ => return None,
        };
        Some(Anchor(Vec2::new(v.0, v.1)))
    }
}

/// Layout properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub width: Size,
    pub height: Size,
    pub min: Vec2,
    pub max: Option<Vec2>,
    pub padding: Sides,
    pub margin: Sides,
    pub layout: Layout,
    pub gap: f32,
    pub align: Align,
    pub justify: Justify,
    pub anchor: Anchor,
    /// Moves the widget after layout.
    pub offset: Vec2,
}

/// A background, bar or thumb: a color, or an image with its own tint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    Color(Color),
    Image { texture: TextureId, size: Vec2, tint: Color },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justified,
}

/// A text widget's content and style. The color is in [`Look`] so states can change it.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLook {
    pub markup: String,
    pub size: f32,
    pub font: Option<String>,
    pub line_height: f32,
    pub align: TextAlign,
    /// Typewriter characters per second (0 = all at once) and per-glyph fade.
    pub cps: f32,
    pub fade: f32,
}

impl Default for TextLook {
    fn default() -> Self {
        Self {
            markup: String::new(),
            size: 32.0,
            font: None,
            line_height: 1.4,
            align: TextAlign::Left,
            cps: 0.0,
            fade: 0.08,
        }
    }
}

/// Visual properties. Every field is optional so state overrides (`hover_*` etc.) can
/// replace just what they name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub background: Option<Fill>,
    /// Nine-slice borders of an image background, and their scale.
    pub slice: Option<[f32; 4]>,
    pub slice_scale: Option<f32>,
    /// Multiplies image colors (backgrounds and images).
    pub tint: Option<Color>,
    pub opacity: Option<f32>,
    /// Text color.
    pub color: Option<Color>,
    /// An image widget's image.
    pub image: Option<Fill>,
    /// A slider's filled part and its thumb.
    pub bar: Option<Fill>,
    pub thumb: Option<Fill>,
}

impl Look {
    /// Overrides the fields `other` sets.
    pub fn merge(&mut self, other: &Look) {
        macro_rules! take {
            ($($f:ident),*) => { $( if other.$f.is_some() { self.$f = other.$f.clone(); } )* };
        }
        take!(background, slice, slice_scale, tint, opacity, color, image, bar, thumb);
    }
}

/// States a button (and everything inside it) can be in, for look overrides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub hover: bool,
    pub pressed: bool,
    pub selected: bool,
    pub disabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SliderSpec {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    /// Snaps values to multiples of `step` from `min` (0 = continuous).
    pub step: f32,
    pub vertical: bool,
    /// Thumb size along the slider (and across it).
    pub thumb_size: f32,
}

impl SliderSpec {
    /// The value at fraction `t` (0..1) along the slider.
    pub fn value_at(&self, t: f32) -> f32 {
        let v = self.min + (self.max - self.min) * t.clamp(0.0, 1.0);
        if self.step > 0.0 {
            let v = self.min + ((v - self.min) / self.step).round() * self.step;
            v.clamp(self.min.min(self.max), self.max.max(self.min))
        } else {
            v
        }
    }

    /// Where `value` sits along the slider, 0..1.
    pub fn fraction(&self, value: f32) -> f32 {
        if self.max == self.min { 0.0 } else { ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0) }
    }
}

/// One widget description. Children are laid out by the box's [`Layout`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub kind: Kind,
    /// Matches widgets across rebuilds (lists that change order); otherwise position does.
    pub key: Option<String>,
    /// A name the game can look the widget up by.
    pub id: Option<String>,
    pub style: Style,
    pub look: Look,
    pub hover: Look,
    pub pressed: Look,
    pub selected_look: Look,
    pub disabled_look: Look,
    pub text: Option<TextLook>,
    pub slider: Option<SliderSpec>,
    /// A box that reacts to the pointer (hover and press states, clicks).
    pub button: bool,
    pub selected: bool,
    pub disabled: bool,
    /// Host callback ids.
    pub on_click: Option<u64>,
    pub on_change: Option<u64>,
    /// Scroll widgets: scroll sideways instead of down; start scrolled to the end.
    pub horizontal: bool,
    pub start_at_end: bool,
    /// Node widgets: the host's node, and its natural size.
    pub node: Option<u64>,
    pub node_size: Vec2,
    pub children: Vec<Element>,
}

impl Element {
    pub fn new(kind: Kind) -> Self {
        Self { kind, ..Default::default() }
    }

    pub fn with_children(mut self, children: Vec<Element>) -> Self {
        self.children = children;
        self
    }

    /// Whether the pointer can interact with it.
    pub fn interactive(&self) -> bool {
        (self.button || self.kind == Kind::Slider) && !self.disabled
    }

    /// The look for a state, overrides applied in order: selected, hover, pressed, disabled.
    pub fn look_for(&self, s: State) -> Look {
        let mut l = self.look.clone();
        if s.selected {
            l.merge(&self.selected_look);
        }
        if s.hover && !s.disabled {
            l.merge(&self.hover);
        }
        if s.pressed && !s.disabled {
            l.merge(&self.pressed);
        }
        if s.disabled {
            l.merge(&self.disabled_look);
        }
        l
    }
}
