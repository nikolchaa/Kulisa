//! Translations: one plain text file per language (`lang/ja.txt`), which `kulisa translate`
//! writes and translators fill in. A source line is followed by its translation:
//!
//! ```text
//! language = 日本語
//! font = fonts/NotoSansJP.ttf
//!
//! # story/script.kulisa:12
//! @kitty
//! You're new here.
//! = 新入りだね。
//! ```
//!
//! `@kitty` says who speaks, so one line can read differently per character. An entry without
//! it also serves any character. Lines are found by their text, not their place, so saves and
//! rollback are not involved and moving lines around keeps their translations.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ast::{File, Stmt, StmtKind};
use crate::{Diagnostic, Diagnostics, Loc};

/// Where a translated line is looked up: `who` and `source` joined.
pub fn lang_key(who: Option<&str>, source: &str) -> String {
    format!("{}\u{1f}{source}", who.unwrap_or(""))
}

#[derive(Clone, Debug, PartialEq)]
pub struct LangEntry {
    pub who: Option<String>,
    pub source: String,
    pub text: String,
    pub loc: Loc,
}

/// One parsed language file.
#[derive(Clone, Debug, PartialEq)]
pub struct Language {
    /// From the file name (`ja`, `pt-BR`).
    pub code: String,
    /// What players see in a language menu (`language = 日本語`).
    pub name: String,
    /// A font in the game to load for this language (`font = ...`).
    pub font: Option<String>,
    pub file: String,
    pub source: String,
    /// Translated entries by [`lang_key`]; empty translations are left out.
    pub entries: HashMap<String, LangEntry>,
}

impl Language {
    /// The translation of interface text (`kulisa.t`), if it has one.
    pub fn text(&self, source: &str) -> Option<&str> {
        self.entries.get(&lang_key(None, source)).map(|e| e.text.as_str())
    }
}

fn escape(s: &str) -> String {
    let s = s.replace('\\', "\\\\").replace('\n', "\\n");
    if s.starts_with(['#', '@', '=', ' ']) { format!("\\{s}") } else { s }
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Language codes name files and travel to platforms: `ja`, `pt-BR`, `zh_Hans`.
pub fn valid_lang_code(code: &str) -> bool {
    !code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parses `src`, the language file `file` (`lang/ja.txt`).
pub fn parse_language(file: &str, src: &str) -> Result<Language, Diagnostics> {
    let stem = file.rsplit('/').next().unwrap_or(file);
    let code = stem.strip_suffix(".txt").unwrap_or(stem).to_string();
    let mut language = Language {
        name: code.clone(),
        code,
        font: None,
        file: file.to_string(),
        source: src.to_string(),
        entries: HashMap::new(),
    };
    let mut errors = Vec::new();
    let mut who: Option<String> = None;
    let mut pending: Option<(String, Loc, &str)> = None;
    let mut in_header = true;
    let at = |line: usize, col: usize| Loc { file: file.to_string(), line, col };
    if !valid_lang_code(&language.code) {
        errors.push(Diagnostic::new(
            at(1, 1),
            format!("`{}` cannot name a language; use a code like `ja` or `pt-BR`", language.code),
            src.lines().next().unwrap_or(""),
        ));
    }
    for (i, raw) in src.lines().enumerate() {
        let number = i + 1;
        let line = raw.trim_end();
        let line = line.strip_prefix('\u{feff}').unwrap_or(line);
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if in_header && let Some((key, value)) = line.split_once(" = ") {
            match key.trim() {
                "language" => {
                    language.name = value.trim().to_string();
                    continue;
                }
                "font" => {
                    language.font = Some(value.trim().to_string());
                    continue;
                }
                _ => {}
            }
        }
        in_header = false;
        if let Some(text) = line.strip_prefix('=') {
            let Some((source, loc, _)) = pending.take() else {
                errors.push(Diagnostic::new(
                    at(number, 1),
                    "this translation has no source line above it",
                    raw,
                ));
                continue;
            };
            let who = who.take();
            let text = unescape(text.strip_prefix(' ').unwrap_or(text));
            if !text.is_empty() {
                language.entries.insert(
                    lang_key(who.as_deref(), &source),
                    LangEntry { who, source, text, loc: at(number, loc.col) },
                );
            }
            continue;
        }
        if let Some((_, loc, prev)) = &pending {
            errors.push(Diagnostic::new(
                loc.clone(),
                "this line needs a `= translation` line after it",
                prev,
            ));
            pending = None;
        }
        if let Some(name) = line.strip_prefix('@') {
            who = Some(name.trim().to_string());
            continue;
        }
        pending = Some((unescape(line), at(number, 1), raw));
    }
    if let Some((_, loc, prev)) = &pending {
        errors.push(Diagnostic::new(loc.clone(), "this line needs a `= translation` line after it", prev));
    }
    if errors.is_empty() { Ok(language) } else { Err(Diagnostics(errors)) }
}

/// A text a game shows, found by [`story_strings`] or [`script_strings`].
#[derive(Clone, Debug, PartialEq)]
pub struct LangSource {
    pub who: Option<String>,
    pub text: String,
    /// `story/script.kulisa:12`.
    pub place: String,
}

fn walk(body: &[Stmt], out: &mut Vec<LangSource>) {
    fn add(out: &mut Vec<LangSource>, who: Option<String>, text: &str, loc: &Loc) {
        out.push(LangSource { who, text: text.to_string(), place: format!("{}:{}", loc.file, loc.line) });
    }
    for s in body {
        match &s.kind {
            StmtKind::Say(say) => {
                add(out, say.who.as_ref().map(|(w, _)| w.clone()), &say.text, &say.text_loc);
            }
            StmtKind::Menu { caption, choices } => {
                if let Some(c) = caption {
                    add(out, c.who.as_ref().map(|(w, _)| w.clone()), &c.text, &c.text_loc);
                }
                for c in choices {
                    add(out, None, &c.text, &c.text_loc);
                }
                for c in choices {
                    walk(&c.body, out);
                }
            }
            StmtKind::If { branches } => {
                for b in branches {
                    walk(&b.body, out);
                }
            }
            _ => {}
        }
    }
}

/// Dialogue, menu text and character names of a story, in order.
pub fn story_strings(file: &File) -> Vec<LangSource> {
    let mut out = Vec::new();
    for d in &file.defines {
        let place = format!("{}:{}", d.loc.file, d.loc.line);
        for text in calls(&d.expr, &["Character"]) {
            out.push(LangSource { who: None, text, place: place.clone() });
        }
    }
    for l in &file.labels {
        walk(&l.body, &mut out);
    }
    out
}

/// The strings a Luau script passes to `t("...")`, `kulisa.t("...")` or `mark("...")`.
pub fn script_strings(file: &str, src: &str) -> Vec<LangSource> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        for text in calls(line, &["t", "mark"]) {
            out.push(LangSource { who: None, text, place: format!("{file}:{}", i + 1) });
        }
    }
    out
}

/// The literal first arguments of calls to `names` in Luau source. Only a scan: it needs no
/// valid script, and computed strings are for the game to pass through `kulisa.t` itself.
fn calls(src: &str, names: &[&str]) -> Vec<String> {
    let bytes = src.as_bytes();
    let mut found = Vec::new();
    for name in names {
        let mut from = 0;
        while let Some(i) = src[from..].find(name).map(|i| i + from) {
            from = i + name.len();
            let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
            if i > 0 && word(bytes[i - 1]) {
                continue;
            }
            let rest = src[from..].trim_start();
            let Some(rest) = rest.strip_prefix('(') else { continue };
            let rest = rest.trim_start();
            let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else { continue };
            let mut text = String::new();
            let mut chars = rest[1..].chars();
            let closed = loop {
                match chars.next() {
                    None => break false,
                    Some(c) if c == quote => break true,
                    Some('\\') => match chars.next() {
                        Some('n') => text.push('\n'),
                        Some(c) => text.push(c),
                        None => break false,
                    },
                    Some(c) => text.push(c),
                }
            };
            if closed && !text.is_empty() {
                found.push(text);
            }
        }
    }
    found
}

/// What [`update_language`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LangReport {
    pub total: usize,
    pub missing: usize,
    /// Translated entries the game no longer shows, kept as comments.
    pub unused: usize,
}

/// A language file for `sources`, with what `old` has translated kept. Translations nothing
/// uses any more stay in the file as comments, since lines that merely changed need them.
pub fn update_language(code: &str, old: Option<&Language>, sources: &[LangSource]) -> (String, LangReport) {
    let mut out = format!(
        "# Translation into `{code}`. Fill in the lines after `=`; an empty one shows the original.\n\
         # Keep [expressions] and {{tags}} as they are. Run `kulisa translate {code}` after changing the game.\n\
         language = {}\n",
        old.map_or(code, |l| l.name.as_str())
    );
    if let Some(font) = old.and_then(|l| l.font.as_deref()) {
        out.push_str(&format!("font = {font}\n"));
    }
    let mut report = LangReport::default();
    let mut seen = HashSet::new();
    let mut used = HashSet::new();
    for s in sources {
        let key = lang_key(s.who.as_deref(), &s.text);
        if !seen.insert(key.clone()) {
            continue;
        }
        report.total += 1;
        // An entry for any speaker serves this line; keep using it rather than asking again.
        let shared = lang_key(None, &s.text);
        let entries = old.map(|l| &l.entries);
        let found = entries
            .and_then(|e| e.get(&key).map(|f| (&key, f)).or_else(|| e.get(&shared).map(|f| (&shared, f))));
        let text = found.map_or("", |(k, e)| {
            used.insert(k.clone());
            e.text.as_str()
        });
        if text.is_empty() {
            report.missing += 1;
        }
        out.push_str(&format!("\n# {}\n", s.place));
        if let Some(who) = &s.who {
            out.push_str(&format!("@{who}\n"));
        }
        out.push_str(&format!("{}\n= {}\n", escape(&s.text), escape(text)));
    }
    // Sorted, so runs of the tool give the same file.
    let unused: BTreeMap<&String, &LangEntry> =
        old.map(|l| l.entries.iter().filter(|(k, _)| !used.contains(*k)).collect()).unwrap_or_default();
    if !unused.is_empty() {
        out.push_str("\n# No longer in the game. Reuse these or delete them:\n");
    }
    for e in unused.values() {
        report.unused += 1;
        out.push_str(&format!("#   {}\n#   = {}\n", escape(&e.source), escape(&e.text)));
    }
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "language = Srpski\nfont = fonts/x.ttf\n\n# a comment\n@kitty\nHello.\n= Zdravo.\n\nLook = around\n= Pogledaj [name]\n\nUntranslated\n=\n\\# not a comment\n= \\= ni ovo\nTwo\\nlines\n= Dva\\nreda\n";

    #[test]
    fn parses_headers_speakers_and_escapes() {
        let l = parse_language("lang/sr.txt", FILE).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (l.code.as_str(), l.name.as_str(), l.font.as_deref()),
            ("sr", "Srpski", Some("fonts/x.ttf"))
        );
        assert_eq!(l.entries[&lang_key(Some("kitty"), "Hello.")].text, "Zdravo.");
        assert_eq!(l.text("Look = around"), Some("Pogledaj [name]"), "` = ` is only special in the header");
        assert_eq!(l.text("Hello."), None, "that entry belongs to kitty");
        assert_eq!(l.text("Untranslated"), None);
        assert_eq!(l.text("# not a comment"), Some("= ni ovo"));
        assert_eq!(l.text("Two\nlines"), Some("Dva\nreda"));
    }

    #[test]
    fn errors_name_the_line() {
        let e = parse_language("lang/sr.txt", "= orphan\n").unwrap_err().to_string();
        assert!(e.contains("lang/sr.txt:1:1") && e.contains("no source line"), "{e}");
        let e = parse_language("lang/sr.txt", "First\nSecond\n= Drugi\n").unwrap_err().to_string();
        assert!(e.contains("lang/sr.txt:1:1") && e.contains("needs a `= translation`"), "{e}");
        let e = parse_language("lang/sr.txt", "Last\n").unwrap_err().to_string();
        assert!(e.contains("needs a `= translation`"), "{e}");
        assert!(parse_language("lang/a b.txt", "").is_err());
    }

    #[test]
    fn finds_text_in_stories_and_scripts() {
        let story = "define kitty = Character(\"Kitty\", { color = \"#fff\" })\nlabel start:\n    kitty \"Hi [name].\"\n    menu:\n        \"Well?\"\n        \"Yes\":\n            \"Good.\"\n        \"No\" if a:\n            if b:\n                kitty \"Hm.\"\n";
        let file = crate::parse("s.kulisa", story).unwrap();
        let strings = story_strings(&file);
        let found: Vec<(Option<&str>, &str)> =
            strings.iter().map(|s| (s.who.as_deref(), s.text.as_str())).collect();
        assert_eq!(
            found,
            [
                (None, "Kitty"),
                (Some("kitty"), "Hi [name]."),
                (None, "Well?"),
                (None, "Yes"),
                (None, "No"),
                (None, "Good."),
                (Some("kitty"), "Hm."),
            ]
        );
        let src = "local t = kulisa.t\nlabel(t(\"Start\"), kulisa.t('It\\'s'))\nget(\"x\") print(t(name)) text(\"y\") t(\"\")\nname = mark(\"Lantern\"), remark(\"no\")\n";
        let found: Vec<String> = script_strings("ui.luau", src).into_iter().map(|s| s.text).collect();
        assert_eq!(found, ["Start", "It's", "Lantern"]);
        assert_eq!(script_strings("ui.luau", src)[0].place, "ui.luau:2");
    }

    #[test]
    fn updating_keeps_translations_and_comments_out_unused_ones() {
        let source = |who: Option<&str>, text: &str| LangSource {
            who: who.map(str::to_string),
            text: text.to_string(),
            place: "s.kulisa:1".into(),
        };
        let (first, report) = update_language(
            "sr",
            None,
            &[source(Some("kitty"), "Hello."), source(None, "#hash"), source(Some("kitty"), "Hello.")],
        );
        assert_eq!(report, LangReport { total: 2, missing: 2, unused: 0 });
        assert!(first.contains("language = sr\n"), "{first}");
        assert!(first.contains("@kitty\nHello.\n= \n"), "{first}");
        assert!(first.contains("\\#hash\n= \n"), "{first}");

        let filled =
            first.replace("Hello.\n= ", "Hello.\n= Zdravo.").replace("language = sr", "language = Srpski");
        let old = parse_language("lang/sr.txt", &filled).unwrap_or_else(|e| panic!("{e}"));
        let (next, report) =
            update_language("sr", Some(&old), &[source(Some("kitty"), "Hello."), source(None, "New")]);
        assert_eq!(report, LangReport { total: 2, missing: 1, unused: 0 });
        assert!(next.contains("language = Srpski\n") && next.contains("= Zdravo.\n"), "{next}");

        let (gone, report) = update_language("sr", Some(&old), &[source(None, "New")]);
        assert_eq!(report, LangReport { total: 1, missing: 1, unused: 1 });
        assert!(gone.contains("#   Hello.\n#   = Zdravo.\n"), "{gone}");
        let again = parse_language("lang/sr.txt", &gone).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(update_language("sr", Some(&again), &[source(None, "New")]).1.unused, 0);

        // An entry for any speaker also serves a character's line.
        let any = parse_language("lang/sr.txt", "Hello.\n= Zdravo.\n").unwrap();
        let (next, report) = update_language("sr", Some(&any), &[source(Some("kitty"), "Hello.")]);
        assert_eq!(report, LangReport { total: 1, missing: 0, unused: 0 });
        assert!(next.contains("@kitty\nHello.\n= Zdravo.\n"), "{next}");
    }
}
