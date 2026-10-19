//! Diagnostics: file, line, column and a plain-English message, rendered with the
//! offending line and a caret.

use std::fmt;

/// A position in a story file. Lines and columns are 1-based; columns count characters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Loc {
    pub file: String,
    pub line: usize,
    pub col: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub loc: Loc,
    pub message: String,
    /// The source line, for the caret display.
    pub line_text: String,
}

impl Diagnostic {
    pub fn new(loc: Loc, message: impl Into<String>, line_text: &str) -> Self {
        Self { loc, message: message.into(), line_text: line_text.to_string() }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Loc { file, line, col } = &self.loc;
        writeln!(f, "{file}:{line}:{col}: error: {}", self.message)?;
        let gutter = line.to_string().len();
        writeln!(f, "{:gutter$} |", "")?;
        writeln!(f, "{line} | {}", self.line_text)?;
        write!(f, "{:gutter$} | {}^", "", " ".repeat(col.saturating_sub(1)))
    }
}

impl std::error::Error for Diagnostic {}

/// Several diagnostics, printed one after another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostics(pub Vec<Diagnostic>);

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, d) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f, "\n")?;
            }
            write!(f, "{d}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostics {}

/// Edit distance, for "did you mean" suggestions.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + (ca != *cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// The closest candidate within a small edit distance.
pub(crate) fn suggest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    candidates
        .map(|c| (levenshtein(name, c), c))
        .filter(|(d, c)| *d <= 2.max(c.len() / 4) && *d > 0)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_caret() {
        let d = Diagnostic::new(
            Loc { file: "intro.kulisa".into(), line: 12, col: 10 },
            "unknown label `cafe_tak`; did you mean `cafe_talk`?",
            "    jump cafe_tak",
        );
        let text = d.to_string();
        assert_eq!(
            text,
            "intro.kulisa:12:10: error: unknown label `cafe_tak`; did you mean `cafe_talk`?\n   |\n12 |     jump cafe_tak\n   |          ^"
        );
    }

    #[test]
    fn suggests_close_names() {
        let names = ["cafe_talk", "lost_route", "start"];
        assert_eq!(suggest("cafe_tak", names.iter().copied()), Some("cafe_talk"));
        assert_eq!(suggest("strat", names.iter().copied()), Some("start"));
        assert_eq!(suggest("zzz", names.iter().copied()), None);
    }
}
