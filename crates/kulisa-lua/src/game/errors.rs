//! The engine's error view: script and asset errors drawn over the game in a small built-in
//! bitmap font, so they show even when the game has no fonts or failed to start. It shows
//! every error in development runs, and otherwise the errors no `error` handler takes.

use kulisa_core::{Color, Node, NodeKind, Quad, Rect, Scene, Sprite, TextureId, UVec2, Vec2};
use kulisa_render::{Image, Renderer};

/// Errors kept on screen at most (the newest first).
const KEEP: usize = 6;
const COLS: usize = 16;
/// Glyph size in font pixels, and the gaps around each glyph in a cell.
const GW: usize = 5;
const GH: usize = 7;
const GAP_X: usize = 1;
const GAP_Y: usize = 3;

#[derive(Default)]
pub(crate) struct ErrorView {
    /// Newest last.
    messages: Vec<String>,
    /// More errors arrived than are kept.
    dropped: usize,
    pub visible: bool,
    /// Development run: every error shows, with reload hints.
    pub dev: bool,
    /// The pointer whose press hid the view: its release is not passed on.
    pub swallow_release: Option<u32>,
    dirty: bool,
    pub scene: Option<Scene>,
    /// The glyph sheet and the font pixel size it was drawn at.
    sheet: Option<(u32, TextureId)>,
    built_for: (UVec2, [f32; 4]),
}

impl ErrorView {
    pub fn push(&mut self, message: &str) {
        self.messages.push(message.trim_end().to_string());
        if self.messages.len() > KEEP {
            self.messages.remove(0);
            self.dropped += 1;
        }
        self.visible = true;
        self.dirty = true;
    }

    pub fn hide(&mut self) {
        self.messages.clear();
        self.dropped = 0;
        self.visible = false;
        self.scene = None;
    }

    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// Rebuilds the overlay scene when the errors, the window or the insets changed.
    pub fn update(&mut self, renderer: &mut Renderer, safe: [f32; 4], white: TextureId) {
        if !self.visible {
            self.scene = None;
            return;
        }
        let window = renderer.size();
        if !self.dirty && self.scene.is_some() && self.built_for == (window, safe) {
            return;
        }
        self.dirty = false;
        self.built_for = (window, safe);
        // Font pixels: 2 on a 720p window, 3 at 1080p, 4 at 1440p.
        let px = ((window.x.min(window.y) as f32 / 360.0).round() as u32).clamp(1, 6);
        let sheet = match self.sheet {
            Some((p, id)) if p == px => id,
            _ => {
                let id = renderer.create_texture(&glyph_sheet(px as usize));
                self.sheet = Some((px, id));
                id
            }
        };
        self.scene = Some(self.build(window, safe, px as f32, sheet, white));
    }

    fn build(&self, window: UVec2, safe: [f32; 4], px: f32, sheet: TextureId, white: TextureId) -> Scene {
        let size = window.as_vec2();
        let mut scene = Scene::new(size);
        let root = scene.root();
        let backdrop = Sprite { texture: white, size, uv: Rect::UNIT, slice: None };
        let mut bg = Node::new(NodeKind::Sprite(backdrop));
        bg.tint = Color::hex("#120d14e6").unwrap_or(Color::BLACK);
        scene.add(root, bg);

        let (cw, ch) = ((GW + GAP_X) as f32 * px, (GH + GAP_Y) as f32 * px);
        let margin = 2.0 * cw;
        let (left, top) = (safe[0] + margin, safe[1] + margin);
        let cols = (((size.x - safe[0] - safe[2] - 2.0 * margin) / cw) as usize).max(10);
        let rows = (((size.y - safe[1] - safe[3] - 2.0 * margin) / ch) as usize).max(4);

        let red = Color::hex("#ff6b6b").unwrap();
        let text = Color::hex("#f4f1ea").unwrap();
        let dim = Color::hex("#a59fb0").unwrap();
        let mut lines: Vec<(String, Color)> = Vec::new();
        let total = self.messages.len() + self.dropped;
        let title = if total == 1 { "Error".to_string() } else { format!("{total} errors (newest first)") };
        lines.push((title, red));
        lines.push((String::new(), text));
        for (i, m) in self.messages.iter().rev().enumerate() {
            if i > 0 {
                lines.push(("-".repeat(cols.min(40)), dim));
            }
            for l in m.lines() {
                for w in wrap(l, cols) {
                    lines.push((w, text));
                }
            }
        }
        let hint = if self.dev {
            "Save a file to reload; Shift+R reloads now. Esc, Enter or a click hides this."
        } else {
            "Esc, Enter or a click hides this and continues."
        };
        let footer: Vec<String> = wrap(hint, cols);
        let room = rows.saturating_sub(footer.len() + 1);
        if lines.len() > room {
            lines.truncate(room.saturating_sub(1));
            lines.push(("...".into(), dim));
        }
        lines.push((String::new(), text));
        lines.extend(footer.into_iter().map(|f| (f, dim)));

        let mut quads = Vec::new();
        let (sw, sh) = (sheet_size(px as usize).0 as f32, sheet_size(px as usize).1 as f32);
        for (row, (line, color)) in lines.iter().enumerate() {
            let y = top + row as f32 * ch;
            for (col, c) in line.chars().enumerate() {
                let Some(g) = glyph_index(c) else { continue };
                let (gx, gy) = ((g % COLS) as f32, (g / COLS) as f32);
                let (w, h) = (GW as f32 * px, GH as f32 * px);
                let (u, v) = (gx * (w + px) / sw, gy * (h + px) / sh);
                quads.push(Quad {
                    texture: sheet,
                    rect: Rect::new(left + col as f32 * cw, y, w, h),
                    uv: Rect { min: Vec2::new(u, v), max: Vec2::new(u + w / sw, v + h / sh) },
                    color: *color,
                });
            }
        }
        scene.add(root, Node::quads(quads));
        scene
    }
}

/// Splits `line` into pieces of at most `cols` characters, at spaces where it can.
fn wrap(line: &str, cols: usize) -> Vec<String> {
    let line = line.replace('\t', "    ");
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= cols {
        return vec![line];
    }
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + cols).min(chars.len());
        if end < chars.len()
            && let Some(space) = chars[start..=end].iter().rposition(|c| *c == ' ')
            && space > cols / 2
            && space <= cols
        {
            end = start + space + 1;
        }
        out.push(chars[start..end].iter().collect::<String>().trim_end().to_string());
        start = end;
    }
    out
}

fn glyph_index(c: char) -> Option<usize> {
    match c {
        ' ' => None,
        '!'..='~' => Some(c as usize - 0x20),
        _ => Some('?' as usize - 0x20),
    }
}

fn sheet_size(px: usize) -> (usize, usize) {
    let rows = GLYPHS.len().div_ceil(COLS);
    (COLS * (GW + 1) * px, rows * (GH + 1) * px)
}

/// Every glyph at `px` screen pixels per font pixel, white on transparent, one font pixel
/// apart so linear filtering never bleeds a neighbour in.
fn glyph_sheet(px: usize) -> Image {
    let (w, h) = sheet_size(px);
    let mut rgba = vec![0u8; w * h * 4];
    for (g, rows) in GLYPHS.iter().enumerate() {
        let (ox, oy) = ((g % COLS) * (GW + 1) * px, (g / COLS) * (GH + 1) * px);
        for (r, bits) in rows.iter().enumerate() {
            for c in 0..GW {
                if bits & (0x10 >> c) == 0 {
                    continue;
                }
                for y in 0..px {
                    for x in 0..px {
                        let i = ((oy + r * px + y) * w + ox + c * px + x) * 4;
                        rgba[i..i + 4].copy_from_slice(&[255; 4]);
                    }
                }
            }
        }
    }
    Image::from_straight_rgba(w as u32, h as u32, rgba)
}

/// ASCII 0x20..=0x7e, 5x7 pixels, one row per byte (bit 4 is the left column).
#[rustfmt::skip]
const GLYPHS: [[u8; 7]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // space
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04], // !
    [0x0a, 0x0a, 0x0a, 0x00, 0x00, 0x00, 0x00], // "
    [0x0a, 0x0a, 0x1f, 0x0a, 0x1f, 0x0a, 0x0a], // #
    [0x04, 0x0f, 0x14, 0x0e, 0x05, 0x1e, 0x04], // $
    [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03], // %
    [0x0c, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0d], // &
    [0x04, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00], // '
    [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02], // (
    [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08], // )
    [0x00, 0x04, 0x15, 0x0e, 0x15, 0x04, 0x00], // *
    [0x00, 0x04, 0x04, 0x1f, 0x04, 0x04, 0x00], // +
    [0x00, 0x00, 0x00, 0x00, 0x0c, 0x04, 0x08], // ,
    [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00], // -
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c], // .
    [0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x00], // /
    [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e], // 0
    [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e], // 1
    [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f], // 2
    [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e], // 3
    [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02], // 4
    [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e], // 5
    [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e], // 6
    [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08], // 7
    [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e], // 8
    [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c], // 9
    [0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x00], // :
    [0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x04, 0x08], // ;
    [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02], // <
    [0x00, 0x00, 0x1f, 0x00, 0x1f, 0x00, 0x00], // =
    [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08], // >
    [0x0e, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04], // ?
    [0x0e, 0x11, 0x01, 0x0d, 0x15, 0x15, 0x0e], // @
    [0x0e, 0x11, 0x11, 0x11, 0x1f, 0x11, 0x11], // A
    [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e], // B
    [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e], // C
    [0x1c, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1c], // D
    [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f], // E
    [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10], // F
    [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f], // G
    [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11], // H
    [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e], // I
    [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0c], // J
    [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11], // K
    [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f], // L
    [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11], // M
    [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11], // N
    [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e], // O
    [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10], // P
    [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d], // Q
    [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11], // R
    [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e], // S
    [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // T
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e], // U
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04], // V
    [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a], // W
    [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11], // X
    [0x11, 0x11, 0x11, 0x0a, 0x04, 0x04, 0x04], // Y
    [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f], // Z
    [0x0e, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0e], // [
    [0x00, 0x10, 0x08, 0x04, 0x02, 0x01, 0x00], // backslash
    [0x0e, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0e], // ]
    [0x04, 0x0a, 0x11, 0x00, 0x00, 0x00, 0x00], // ^
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1f], // _
    [0x08, 0x04, 0x02, 0x00, 0x00, 0x00, 0x00], // `
    [0x00, 0x00, 0x0e, 0x01, 0x0f, 0x11, 0x0f], // a
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x1e], // b
    [0x00, 0x00, 0x0e, 0x10, 0x10, 0x11, 0x0e], // c
    [0x01, 0x01, 0x0d, 0x13, 0x11, 0x11, 0x0f], // d
    [0x00, 0x00, 0x0e, 0x11, 0x1f, 0x10, 0x0e], // e
    [0x06, 0x09, 0x08, 0x1c, 0x08, 0x08, 0x08], // f
    [0x00, 0x0f, 0x11, 0x11, 0x0f, 0x01, 0x0e], // g
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11], // h
    [0x04, 0x00, 0x0c, 0x04, 0x04, 0x04, 0x0e], // i
    [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0c], // j
    [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12], // k
    [0x0c, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e], // l
    [0x00, 0x00, 0x1a, 0x15, 0x15, 0x11, 0x11], // m
    [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11], // n
    [0x00, 0x00, 0x0e, 0x11, 0x11, 0x11, 0x0e], // o
    [0x00, 0x00, 0x1e, 0x11, 0x1e, 0x10, 0x10], // p
    [0x00, 0x00, 0x0d, 0x13, 0x0f, 0x01, 0x01], // q
    [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10], // r
    [0x00, 0x00, 0x0e, 0x10, 0x0e, 0x01, 0x1e], // s
    [0x08, 0x08, 0x1c, 0x08, 0x08, 0x09, 0x06], // t
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0d], // u
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x0a, 0x04], // v
    [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0a], // w
    [0x00, 0x00, 0x11, 0x0a, 0x04, 0x0a, 0x11], // x
    [0x00, 0x00, 0x11, 0x11, 0x0f, 0x01, 0x0e], // y
    [0x00, 0x00, 0x1f, 0x02, 0x04, 0x08, 0x1f], // z
    [0x02, 0x04, 0x04, 0x08, 0x04, 0x04, 0x02], // {
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // |
    [0x08, 0x04, 0x04, 0x02, 0x04, 0x04, 0x08], // }
    [0x00, 0x00, 0x08, 0x15, 0x02, 0x00, 0x00], // ~
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_at_spaces() {
        assert_eq!(wrap("short", 10), ["short"]);
        assert_eq!(wrap("attempt to index nil with 'x'", 16), ["attempt to index", "nil with 'x'"]);
        assert_eq!(wrap("abcdefghijkl", 5), ["abcde", "fghij", "kl"]);
    }

    #[test]
    fn keeps_the_newest_errors() {
        let mut v = ErrorView::default();
        for i in 0..10 {
            v.push(&format!("error {i}\n"));
        }
        assert_eq!(v.messages().len(), KEEP);
        assert_eq!(v.messages().last().unwrap(), "error 9");
        assert!(v.visible);
        v.hide();
        assert!(v.messages().is_empty() && !v.visible);
    }

    #[test]
    fn sheet_has_glyph_pixels() {
        let img = glyph_sheet(2);
        let (w, h) = sheet_size(2);
        assert_eq!((img.width as usize, img.height as usize), (w, h));
        // `!` (glyph 1) has its column 2, row 0 set: pixel (6 + 4, 0) at 2x.
        let i = (12 + 4) * 4;
        assert_eq!(img.pixels[i + 3], 255);
    }
}
