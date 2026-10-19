//! Text: markup parsing, shaping, glyph caching, typewriter and per-glyph effects.

pub mod layout;
pub mod markup;

pub use layout::{Align, Glyph, GlyphSink, TextLayout, TextStyle, TextSystem};
pub use markup::{MarkupError, Rich, parse};
