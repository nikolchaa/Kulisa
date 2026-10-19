//! Lexing: source into logical lines with indentation, plus a cursor for scanning the
//! head of a statement (identifiers, strings, numbers, punctuation, rest of line).

use crate::diag::{Diagnostic, Loc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineKind {
    Code,
    /// Empty or whitespace only.
    Blank,
    /// Starts with `#` after indentation.
    Comment,
}

#[derive(Clone, Debug)]
pub(crate) struct Line {
    /// 1-based line number.
    pub no: usize,
    pub kind: LineKind,
    /// Indentation in columns.
    pub indent: usize,
    /// Content after indentation, without trailing whitespace.
    pub text: String,
    /// The whole line, for error display and raw `lua:` blocks.
    pub raw: String,
}

/// Splits `src` into lines. Tabs in indentation are an error (counted as 4 columns).
pub(crate) fn lines(file: &str, src: &str, errors: &mut Vec<Diagnostic>) -> Vec<Line> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    src.lines()
        .enumerate()
        .map(|(i, raw)| {
            let raw = raw.trim_end_matches('\r');
            let lead: String = raw.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            if lead.contains('\t') {
                errors.push(Diagnostic::new(
                    Loc { file: file.to_string(), line: i + 1, col: lead.find('\t').unwrap() + 1 },
                    "tab used for indentation; indent with spaces",
                    raw,
                ));
            }
            let indent = lead.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum();
            let text = raw[lead.len()..].trim_end().to_string();
            let kind = if text.is_empty() {
                LineKind::Blank
            } else if text.starts_with('#') {
                LineKind::Comment
            } else {
                LineKind::Code
            };
            Line { no: i + 1, kind, indent, text, raw: raw.to_string() }
        })
        .collect()
}

/// Scans one line's content.
pub(crate) struct Cursor<'a> {
    file: &'a str,
    line: &'a Line,
    pos: usize,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl<'a> Cursor<'a> {
    pub fn new(file: &'a str, line: &'a Line) -> Self {
        Self { file, line, pos: 0 }
    }

    pub fn rest_str(&self) -> &'a str {
        &self.line.text[self.pos..]
    }

    /// Location of the cursor (1-based column, counting indentation).
    pub fn loc(&self) -> Loc {
        Loc {
            file: self.file.to_string(),
            line: self.line.no,
            col: self.line.indent + self.line.text[..self.pos].chars().count() + 1,
        }
    }

    pub fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(self.loc(), message, &self.line.raw)
    }

    pub fn skip_ws(&mut self) {
        let rest = self.rest_str();
        self.pos += rest.len() - rest.trim_start().len();
    }

    pub fn at_end(&mut self) -> bool {
        self.skip_ws();
        self.pos >= self.line.text.len()
    }

    pub fn peek(&mut self) -> Option<char> {
        self.skip_ws();
        self.rest_str().chars().next()
    }

    /// Consumes `c` if it is next.
    pub fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    /// An identifier; with `dotted`, also `a.b.c` (label names).
    pub fn ident(&mut self, dotted: bool) -> Option<String> {
        self.skip_ws();
        let rest = self.rest_str();
        let first = rest.chars().next()?;
        if !is_ident_start(first) {
            return None;
        }
        let len = rest.find(|c: char| !(is_ident(c) || (dotted && c == '.'))).unwrap_or(rest.len());
        let id = rest[..len].trim_end_matches('.');
        self.pos += id.len();
        Some(id.to_string())
    }

    /// Consumes `word` if it is the next whole identifier.
    pub fn keyword(&mut self, word: &str) -> bool {
        self.skip_ws();
        let rest = self.rest_str();
        if rest.starts_with(word) && !rest[word.len()..].starts_with(is_ident) {
            self.pos += word.len();
            true
        } else {
            false
        }
    }

    /// A double-quoted string with `\"`, `\\`, `\n` and `\t` escapes, if one is next.
    pub fn string(&mut self) -> Result<Option<String>, Diagnostic> {
        if self.peek() != Some('"') {
            return Ok(None);
        }
        let start = self.loc();
        self.pos += 1;
        let mut out = String::new();
        let mut chars = self.rest_str().char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => {
                    self.pos += i + 1;
                    return Ok(Some(out));
                }
                '\\' => match chars.next() {
                    Some((_, '"')) => out.push('"'),
                    Some((_, '\\')) => out.push('\\'),
                    Some((_, 'n')) => out.push('\n'),
                    Some((_, 't')) => out.push('\t'),
                    Some((j, other)) => {
                        self.pos += j;
                        return Err(
                            self.error(format!("unknown escape `\\{other}` (use \\\" \\\\ \\n or \\t)"))
                        );
                    }
                    None => break,
                },
                c => out.push(c),
            }
        }
        Err(Diagnostic::new(start, "string is never closed with `\"`", &self.line.raw))
    }

    /// A non-negative decimal number, if one is next.
    pub fn number(&mut self) -> Option<f32> {
        self.skip_ws();
        let rest = self.rest_str();
        let len = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
        let n = rest[..len].parse().ok()?;
        self.pos += len;
        Some(n)
    }

    /// Everything left on the line, trimmed.
    pub fn rest(&mut self) -> String {
        self.skip_ws();
        let r = self.rest_str().trim().to_string();
        self.pos = self.line.text.len();
        r
    }

    /// Splits off a trailing `:` that ends the line; returns the text before it.
    pub fn until_block_colon(&mut self) -> Option<String> {
        self.skip_ws();
        let rest = self.rest_str().trim_end();
        let body = rest.strip_suffix(':')?;
        self.pos = self.line.text.len();
        Some(body.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> Line {
        let mut errors = Vec::new();
        lines("t.kulisa", text, &mut errors).remove(0)
    }

    #[test]
    fn splits_lines_with_indent_and_kinds() {
        let mut errors = Vec::new();
        let ls = lines("t", "\u{feff}label a:\r\n    # note\n\n    \tx \"y\"\n", &mut errors);
        assert_eq!(ls.len(), 4);
        assert_eq!((ls[0].kind, ls[0].indent, ls[0].text.as_str()), (LineKind::Code, 0, "label a:"));
        assert_eq!(ls[1].kind, LineKind::Comment);
        assert_eq!(ls[2].kind, LineKind::Blank);
        assert_eq!(ls[3].indent, 8);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].loc.col, 5);
    }

    #[test]
    fn scans_tokens() {
        let l = line("    mila \"Hi, \\\"you\\\".\" with fade");
        let mut c = Cursor::new("t", &l);
        assert_eq!(c.ident(false).as_deref(), Some("mila"));
        assert_eq!(c.string().unwrap().as_deref(), Some("Hi, \"you\"."));
        assert!(c.keyword("with"));
        assert_eq!(c.loc().col, 29, "4 indent + 24 chars before the cursor");
        assert_eq!(c.ident(false).as_deref(), Some("fade"));
        assert!(c.at_end());
    }

    #[test]
    fn keywords_need_whole_words() {
        let l = line("without");
        let mut c = Cursor::new("t", &l);
        assert!(!c.keyword("with"));
        assert_eq!(c.ident(false).as_deref(), Some("without"));
    }

    #[test]
    fn reports_unclosed_strings_and_bad_escapes() {
        let l = line("  \"oops");
        let err = Cursor::new("t", &l).string().unwrap_err();
        assert!(err.message.contains("never closed"));
        assert_eq!(err.loc.col, 3);
        let l = line("\"a\\qb\"");
        assert!(Cursor::new("t", &l).string().unwrap_err().message.contains("unknown escape"));
    }

    #[test]
    fn block_colon_and_numbers() {
        let l = line("if player:has(\"key\") and n > 2:");
        let mut c = Cursor::new("t", &l);
        assert!(c.keyword("if"));
        assert_eq!(c.until_block_colon().as_deref(), Some("player:has(\"key\") and n > 2"));
        let l = line("pause 1.5");
        let mut c = Cursor::new("t", &l);
        assert!(c.keyword("pause"));
        assert_eq!(c.number(), Some(1.5));
    }
}
