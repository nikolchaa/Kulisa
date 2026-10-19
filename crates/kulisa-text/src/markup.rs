//! Inline text markup: `{b}bold{/b}`, `{color=#e8a0bf}…{/color}`, `{wave}…{/wave}`,
//! `{ruby=ふりがな}漢字{/ruby}`, `{w=0.5}` pauses and more. `{{` is a literal brace.

use std::ops::Range;

use kulisa_core::Color;

/// Font size: relative to the text's base size, or absolute pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    Scale(f32),
    Px(f32),
}

/// Style of a run of text. Defaults mean "inherit from the text box".
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub color: Option<Color>,
    pub size: Size,
    pub font: Option<String>,
    /// Vertical wave amplitude in ems.
    pub wave: Option<f32>,
    /// Random jitter amplitude in ems.
    pub shake: Option<f32>,
    /// Typewriter speed multiplier.
    pub speed: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            color: None,
            size: Size::Scale(1.0),
            font: None,
            wave: None,
            shake: None,
            speed: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    /// Byte range in [`Rich::text`].
    pub range: Range<usize>,
    pub style: Style,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ruby {
    /// Byte range of the base text in [`Rich::text`].
    pub base: Range<usize>,
    pub text: String,
}

/// Parsed markup: plain text plus styled spans, ruby annotations and typewriter pauses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rich {
    pub text: String,
    /// Cover all of `text`, in order, without gaps.
    pub spans: Vec<Span>,
    pub ruby: Vec<Ruby>,
    /// `(byte offset, seconds)`: the typewriter waits before revealing that byte.
    pub pauses: Vec<(usize, f32)>,
}

/// A markup error with the byte offset in the source.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkupError {
    pub offset: usize,
    pub message: String,
}

impl std::fmt::Display for MarkupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for MarkupError {}

/// A tag waiting for its closing `{/name}`.
struct OpenTag {
    name: String,
    /// Byte offset of the tag in the source, for errors.
    offset: usize,
    /// Style to restore when the tag closes.
    previous: Style,
    /// For `{ruby}`: where the base text starts and the reading.
    ruby: Option<(usize, String)>,
}

const TAGS: &str = "b, i, color, size, font, wave, shake, speed, ruby, w";

fn number(value: Option<&str>, tag: &str, offset: usize) -> Result<f32, MarkupError> {
    let v = value.ok_or_else(|| MarkupError {
        offset,
        message: format!("`{{{tag}}}` needs a value, e.g. `{{{tag}=1.5}}`"),
    })?;
    v.trim().parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(|| MarkupError {
        offset,
        message: format!("`{v}` is not a valid number for `{{{tag}}}`"),
    })
}

/// Parses `source` into plain text and styles.
pub fn parse(source: &str) -> Result<Rich, MarkupError> {
    let mut rich = Rich::default();
    let mut stack: Vec<OpenTag> = Vec::new();
    let mut style = Style::default();
    let mut span_start = 0;

    let close_span = |rich: &mut Rich, style: &Style, start: &mut usize| {
        if rich.text.len() > *start {
            rich.spans.push(Span { range: *start..rich.text.len(), style: style.clone() });
            *start = rich.text.len();
        }
    };

    let mut chars = source.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' if chars.peek().map(|&(_, c)| c) == Some('{') => {
                chars.next();
                rich.text.push('{');
            }
            '}' if chars.peek().map(|&(_, c)| c) == Some('}') => {
                chars.next();
                rich.text.push('}');
            }
            '{' => {
                let end = source[i..].find('}').ok_or_else(|| MarkupError {
                    offset: i,
                    message: "unclosed `{` (write `{{` for a literal brace)".into(),
                })?;
                let tag = &source[i + 1..i + end];
                while chars.peek().is_some_and(|&(j, _)| j <= i + end) {
                    chars.next();
                }
                let (name, value) = match tag.split_once('=') {
                    Some((n, v)) => (n.trim(), Some(v.trim())),
                    None => (tag.trim(), None),
                };

                if let Some(closing) = name.strip_prefix('/') {
                    let Some(OpenTag { name: open, .. }) = stack.last() else {
                        return Err(MarkupError {
                            offset: i,
                            message: format!("`{{/{closing}}}` closes nothing"),
                        });
                    };
                    if open != closing {
                        return Err(MarkupError {
                            offset: i,
                            message: format!("`{{/{closing}}}` does not match the open `{{{open}}}`"),
                        });
                    }
                    close_span(&mut rich, &style, &mut span_start);
                    let OpenTag { previous, ruby, .. } = stack.pop().unwrap();
                    style = previous;
                    if let Some((start, text)) = ruby {
                        rich.ruby.push(Ruby { base: start..rich.text.len(), text });
                    }
                    continue;
                }

                if name == "w" {
                    let secs = number(value, "w", i)?;
                    rich.pauses.push((rich.text.len(), secs));
                    continue;
                }

                close_span(&mut rich, &style, &mut span_start);
                let previous = style.clone();
                let mut ruby = None;
                match name {
                    "b" => style.bold = true,
                    "i" => style.italic = true,
                    "color" => {
                        let v = value.unwrap_or_default();
                        style.color = Some(Color::hex(v).ok_or_else(|| MarkupError {
                            offset: i,
                            message: format!("`{v}` is not a color; use `#rgb`, `#rrggbb` or `#rrggbbaa`"),
                        })?);
                    }
                    "size" => {
                        style.size = match value.and_then(|v| v.strip_suffix("px")) {
                            Some(px) => Size::Px(number(Some(px), "size", i)?),
                            None => Size::Scale(number(value, "size", i)?),
                        }
                    }
                    "font" => {
                        style.font =
                            Some(value.filter(|v| !v.is_empty()).map(str::to_string).ok_or_else(|| {
                                MarkupError {
                                    offset: i,
                                    message: "`{font}` needs a family name, e.g. `{font=Inter}`".into(),
                                }
                            })?)
                    }
                    "wave" => style.wave = Some(value.map_or(Ok(0.12), |_| number(value, "wave", i))?),
                    "shake" => style.shake = Some(value.map_or(Ok(0.04), |_| number(value, "shake", i))?),
                    "speed" => style.speed = number(value, "speed", i)?.max(0.01),
                    "ruby" => {
                        let text = value.filter(|v| !v.is_empty()).ok_or_else(|| MarkupError {
                            offset: i,
                            message: "`{ruby}` needs its reading, e.g. `{ruby=かんじ}漢字{/ruby}`".into(),
                        })?;
                        if stack.iter().any(|t| t.name == "ruby") {
                            return Err(MarkupError { offset: i, message: "ruby cannot be nested".into() });
                        }
                        ruby = Some((rich.text.len(), text.to_string()));
                    }
                    _ => {
                        return Err(MarkupError {
                            offset: i,
                            message: format!("unknown tag `{{{name}}}` (known: {TAGS})"),
                        });
                    }
                }
                stack.push(OpenTag { name: name.to_string(), offset: i, previous, ruby });
            }
            c => rich.text.push(c),
        }
    }
    if let Some(OpenTag { name, offset, .. }) = stack.last() {
        return Err(MarkupError {
            offset: *offset,
            message: format!("`{{{name}}}` is never closed with `{{/{name}}}`"),
        });
    }
    close_span(&mut rich, &style, &mut span_start);
    Ok(rich)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_one_span() {
        let r = parse("Hello, world.").unwrap();
        assert_eq!(r.text, "Hello, world.");
        assert_eq!(r.spans, vec![Span { range: 0..13, style: Style::default() }]);
    }

    #[test]
    fn nested_styles_split_spans() {
        let r = parse("a{b}b{color=#f00}c{/color}d{/b}e").unwrap();
        assert_eq!(r.text, "abcde");
        let ranges: Vec<_> = r.spans.iter().map(|s| s.range.clone()).collect();
        assert_eq!(ranges, vec![0..1, 1..2, 2..3, 3..4, 4..5]);
        assert!(r.spans[2].style.bold && r.spans[2].style.color.is_some());
        assert!(r.spans[3].style.bold && r.spans[3].style.color.is_none());
        assert!(!r.spans[4].style.bold);
    }

    #[test]
    fn sizes_effects_and_speed() {
        let r = parse(
            "{size=2}x{/size}{size=40px}y{/size}{wave}z{/wave}{shake=0.1}!{/shake}{speed=0.5}s{/speed}",
        )
        .unwrap();
        assert_eq!(r.spans[0].style.size, Size::Scale(2.0));
        assert_eq!(r.spans[1].style.size, Size::Px(40.0));
        assert_eq!(r.spans[2].style.wave, Some(0.12));
        assert_eq!(r.spans[3].style.shake, Some(0.1));
        assert_eq!(r.spans[4].style.speed, 0.5);
    }

    #[test]
    fn ruby_pauses_and_escapes() {
        let r = parse("{ruby=かんじ}漢字{/ruby}を{w=0.5}読む {{x}}").unwrap();
        assert_eq!(r.text, "漢字を読む {x}");
        assert_eq!(r.ruby, vec![Ruby { base: 0.."漢字".len(), text: "かんじ".into() }]);
        assert_eq!(r.pauses, vec![("漢字を".len(), 0.5)]);
    }

    #[test]
    fn errors_are_specific() {
        let err = |s: &str| parse(s).unwrap_err();
        assert!(err("{blink}x{/blink}").message.contains("unknown tag `{blink}`"));
        assert_eq!(err("ab{b}c").offset, 2);
        assert!(err("ab{b}c").message.contains("never closed"));
        assert!(err("{b}x{/i}").message.contains("does not match"));
        assert!(err("x{/b}").message.contains("closes nothing"));
        assert!(err("{color=pink}x{/color}").message.contains("not a color"));
        assert!(err("{size=big}x{/size}").message.contains("not a valid number"));
        assert!(err("{w}").message.contains("needs a value"));
        assert!(err("oops {").message.contains("unclosed"));
        assert!(err("{ruby=a}{ruby=b}x{/ruby}{/ruby}").message.contains("nested"));
    }
}
