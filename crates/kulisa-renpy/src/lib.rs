//! Imports Ren'Py projects: `.rpy` scripts become `.kulisa` stories (labels, dialogue,
//! menus, conditions, jumps and calls, scene/show/hide/with, music and sound, simple `$`
//! lines), images are renamed the way Ren'Py names them, and everything that could not be
//! converted is listed in notes (and left in the story as comments).
//!
//! Screens, transforms, styles and Python blocks are Ren'Py's own; Kulisa games do those in
//! Luau, so they are reported rather than translated.

mod expr;
mod text;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use expr::quote;

/// Something the import changed, approximated or left out.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub file: String,
    pub line: usize,
    pub message: String,
}

/// What `options.rpy` and `gui.rpy` say about the game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameInfo {
    pub title: Option<String>,
    pub version: Option<String>,
    /// `gui.init(width, height)`.
    pub size: Option<(u32, u32)>,
}

/// One logical line: continuation lines joined, the trailing comment removed.
#[derive(Clone, Debug)]
struct Line {
    no: usize,
    indent: usize,
    text: String,
}

impl Line {
    fn is_comment(&self) -> bool {
        self.text.starts_with('#')
    }
}

/// Splits source into logical lines (brackets and triple-quoted strings span lines).
fn logical_lines(src: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let (mut start, mut indent) = (0, 0);
    let mut depth = 0i32;
    let mut in_str: Option<(char, bool)> = None;
    for (n, raw) in src.lines().enumerate() {
        let raw = raw.replace('\t', "    ");
        let continuing = !buf.is_empty();
        // Inside a triple-quoted string the line break is part of the text.
        let joiner = if in_str.is_some_and(|(_, t)| t) { '\n' } else { ' ' };
        if !continuing {
            if raw.trim().is_empty() {
                continue;
            }
            start = n + 1;
            indent = raw.len() - raw.trim_start().len();
            if raw.trim_start().starts_with('#') {
                out.push(Line { no: start, indent, text: raw.trim().to_string() });
                continue;
            }
        }
        let chars: Vec<char> = raw.chars().collect();
        let mut i = 0;
        let mut piece = String::new();
        while i < chars.len() {
            let c = chars[i];
            if let Some((q, triple)) = in_str {
                piece.push(c);
                if c == '\\' {
                    if let Some(&n) = chars.get(i + 1) {
                        piece.push(n);
                    }
                    i += 2;
                    continue;
                }
                if c == q && (!triple || (chars.get(i + 1) == Some(&q) && chars.get(i + 2) == Some(&q))) {
                    if triple {
                        piece.push(q);
                        piece.push(q);
                        i += 2;
                    }
                    in_str = None;
                }
                i += 1;
                continue;
            }
            match c {
                '#' => break,
                '"' | '\'' => {
                    let triple = chars.get(i + 1) == Some(&c) && chars.get(i + 2) == Some(&c);
                    in_str = Some((c, triple));
                    piece.push(c);
                    if triple {
                        piece.push(c);
                        piece.push(c);
                        i += 2;
                    }
                }
                '(' | '[' | '{' => {
                    depth += 1;
                    piece.push(c);
                }
                ')' | ']' | '}' => {
                    depth -= 1;
                    piece.push(c);
                }
                c => piece.push(c),
            }
            i += 1;
        }
        let piece = piece.trim_end();
        let (piece, backslash) = match piece.strip_suffix('\\') {
            Some(p) if in_str.is_none() => (p, true),
            _ => (piece, false),
        };
        if continuing {
            buf.push(joiner);
            buf.push_str(piece.trim_start());
        } else {
            buf.push_str(piece.trim_start());
        }
        // A one-line string left open (an error in Ren'Py too) ends with its line.
        if in_str.is_some_and(|(_, t)| !t) {
            in_str = None;
        }
        if depth <= 0 && in_str.is_none() && !backslash {
            if !buf.trim().is_empty() {
                out.push(Line { no: start, indent, text: std::mem::take(&mut buf).trim().to_string() });
            }
            buf.clear();
            depth = 0;
        }
    }
    if !buf.trim().is_empty() {
        out.push(Line { no: start, indent, text: buf.trim().to_string() });
    }
    out
}

/// The end of the block that line `i` opens: the following lines indented further (comments
/// count as part of it).
fn block_end(lines: &[Line], i: usize, end: usize) -> usize {
    let mut j = i + 1;
    while j < end && (lines[j].indent > lines[i].indent || lines[j].is_comment()) {
        j += 1;
    }
    // Comments right after the block belong to what follows.
    while j > i + 1 && lines[j - 1].is_comment() && lines[j - 1].indent <= lines[i].indent {
        j -= 1;
    }
    j
}

/// The first word and the rest.
fn first_word(s: &str) -> (&str, &str) {
    let end = s.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).unwrap_or(s.len());
    (&s[..end], s[end..].trim_start())
}

fn is_ident(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Words up to the first keyword in `stops`.
fn words_until<'a>(s: &'a str, stops: &[&str]) -> (Vec<&'a str>, Vec<&'a str>) {
    let all: Vec<&str> = s.split_whitespace().collect();
    let at = all.iter().position(|w| stops.contains(w)).unwrap_or(all.len());
    (all[..at].to_vec(), all[at..].to_vec())
}

/// Ren'Py's transitions as the ones Kulisa templates ship (fade, dissolve, wipes): the
/// transition to use (none for `None` and screen shakes) and a note when it changed.
fn transition(src: &str) -> (Option<String>, Option<String>) {
    let src = src.trim();
    let name = src.split('(').next().unwrap_or("").trim();
    match name {
        "None" => (None, None),
        "fade" | "dissolve" | "wipeleft" | "wiperight" | "wipeup" | "wipedown" => (Some(name.into()), None),
        "Dissolve" => (Some("dissolve".into()), None),
        "Fade" => (Some("fade".into()), None),
        "vpunch" | "hpunch" => (None, Some(format!("`with {name}` (a screen shake) was dropped"))),
        n if n.starts_with("move")
            || n.starts_with("ease")
            || n.starts_with("slide")
            || n.starts_with("push")
            || n.starts_with("zoom")
            || [
                "pixellate",
                "irisin",
                "irisout",
                "blinds",
                "squares",
                "ImageDissolve",
                "CropMove",
                "PushMove",
            ]
            .contains(&n) =>
        {
            (Some("dissolve".into()), Some(format!("transition `{name}` became `dissolve`")))
        }
        n if is_ident(n) => (Some(n.into()), None),
        _ => (None, Some(format!("transition `{src}` was dropped"))),
    }
}

/// Ren'Py statement keywords: a line starting with one is never a say line.
const STATEMENTS: &[&str] = &[
    "label", "jump", "call", "return", "pass", "menu", "if", "elif", "else", "while", "scene", "show",
    "hide", "with", "play", "queue", "stop", "voice", "pause", "window", "nvl", "define", "default", "image",
    "python", "init",
];

/// Ren'Py's positions as classic-vn's (left, center, right).
fn position(at: &str) -> Option<&'static str> {
    Some(match at {
        "left" | "topleft" | "offscreenleft" => "left",
        "right" | "topright" | "offscreenright" => "right",
        "center" | "truecenter" | "top" | "default" => "center",
        _ => return None,
    })
}

/// Converts Ren'Py scripts one file at a time, after [`Converter::prescan`] has seen all of
/// them (audio names, character images and game info can be defined in any file).
#[derive(Default)]
pub struct Converter {
    pub notes: Vec<Note>,
    pub info: GameInfo,
    /// Image statements with a file: (image name, path in the game, file, line).
    pub images: Vec<(String, String, String, usize)>,
    /// Audio files the stories play, as written there.
    pub audio: BTreeSet<String>,
    /// Every file of the game folder (game-relative), to find audio by Ren'Py's rules.
    pub files: BTreeSet<String>,
    audio_names: BTreeMap<String, String>,
    char_images: BTreeMap<String, String>,
    /// Characters without a name (`Character(None, ...)`): their lines become narration.
    nameless: BTreeSet<String>,
    /// Speakers given as strings (`"Sylvie" "Hi"`): display name -> generated character.
    speakers: BTreeMap<String, String>,
    file: String,
    out: Vec<String>,
    hoisted: Vec<String>,
    last_speaker: Option<String>,
}

impl Converter {
    pub fn new(files: BTreeSet<String>) -> Self {
        Self { files, ..Default::default() }
    }

    fn note(&mut self, line: usize, message: impl Into<String>) {
        self.notes.push(Note { file: self.file.clone(), line, message: message.into() });
    }

    fn push(&mut self, indent: usize, text: impl AsRef<str>) {
        self.out.push(format!("{}{}", " ".repeat(indent), text.as_ref()));
    }

    /// Leaves an unconverted line in the story as a comment.
    fn comment_out(&mut self, indent: usize, line: &Line, why: &str) {
        self.note(line.no, format!("{why}: `{}`", line.text.lines().next().unwrap_or("")));
        self.push(indent, format!("# Ren'Py: {}", line.text.replace('\n', " ")));
    }

    /// Reads what any file can define for the others: `define audio.x`, characters' image
    /// tags, `config.name`, `config.version` and `gui.init`.
    pub fn prescan(&mut self, src: &str) {
        for line in logical_lines(src) {
            let t = line.text.as_str();
            if let Some(rest) = t.strip_prefix("define ") {
                let rest = rest.trim_start_matches(|c: char| c == '-' || c.is_ascii_digit()).trim();
                let Some((name, value)) = rest.split_once('=') else { continue };
                let (name, value) = (name.trim(), value.trim());
                let string = || {
                    expr::tokenize(value).ok().and_then(|t| {
                        t.into_iter().find_map(|t| if let expr::Tok::Str(s) = t { Some(s) } else { None })
                    })
                };
                match name {
                    "config.name" => self.info.title = string(),
                    "config.version" => self.info.version = string(),
                    n if n.starts_with("audio.") => {
                        if let Some(file) = string() {
                            self.audio_names.insert(n["audio.".len()..].to_string(), file);
                        }
                    }
                    n if is_ident(n) && value.starts_with("Character") => {
                        let toks = expr::tokenize(value).unwrap_or_default();
                        // `Character(None, ...)` speaks without a name: narration.
                        if toks.get(2) == Some(&expr::Tok::Ident("None".into())) {
                            self.nameless.insert(n.to_string());
                        }
                        for w in toks.windows(3) {
                            if let [expr::Tok::Ident(k), expr::Tok::Op(eq), expr::Tok::Str(img)] = w
                                && k == "image"
                                && eq == "="
                            {
                                self.char_images.insert(n.to_string(), img.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let Some(args) = t.strip_prefix("gui.init(").or_else(|| t.strip_prefix("$ gui.init(")) {
                let nums: Vec<u32> = args.split([',', ')']).filter_map(|n| n.trim().parse().ok()).collect();
                if let [w, h, ..] = nums[..] {
                    self.info.size = Some((w, h));
                }
            }
        }
    }

    /// Converts one `.rpy` file; returns the `.kulisa` source.
    pub fn convert(&mut self, file: &str, src: &str) -> String {
        self.file = file.to_string();
        self.out.clear();
        self.hoisted.clear();
        let lines = logical_lines(src);
        self.root(&lines, 0, lines.len(), 0);
        let mut out = std::mem::take(&mut self.hoisted);
        if !out.is_empty() {
            out.push(String::new());
        }
        out.append(&mut self.out);
        let mut text = out.join("\n");
        text.push('\n');
        text
    }

    /// The characters for speakers named with strings, if any (a story file of their own).
    pub fn speakers_file(&self) -> Option<String> {
        if self.speakers.is_empty() {
            return None;
        }
        let mut out = String::from("# Speakers the Ren'Py script named with strings.\n");
        for (name, id) in &self.speakers {
            out.push_str(&format!("define {id} = Character({})\n", quote(name)));
        }
        Some(out)
    }

    fn root(&mut self, lines: &[Line], start: usize, end: usize, indent: usize) {
        let mut i = start;
        while i < end {
            let next = block_end(lines, i, end);
            self.root_statement(lines, i, next, indent);
            i = next;
        }
    }

    fn root_statement(&mut self, lines: &[Line], i: usize, next: usize, indent: usize) {
        let line = &lines[i];
        let t = line.text.as_str();
        if line.is_comment() {
            self.push(indent, t);
            return;
        }
        let (kw, rest) = first_word(t);
        match kw {
            "label" => {
                let name = rest.split(['(', ':', ' ']).next().unwrap_or("").trim();
                if rest.contains('(') {
                    self.note(line.no, format!("label `{name}`: parameters were dropped"));
                }
                if !is_ident(name) {
                    self.comment_out(indent, line, "unusual label");
                    return;
                }
                if !self.out.is_empty() && self.out.last().is_some_and(|l| !l.is_empty()) {
                    self.out.push(String::new());
                }
                self.push(indent, format!("label {name}:"));
                self.last_speaker = None;
                if !self.body(lines, i + 1, next, indent + 4) {
                    self.push(indent + 4, "pass");
                }
            }
            "define" | "default" => {
                let stmt = self.binding(kw, rest, line);
                if let Some(s) = stmt {
                    self.push(indent, s);
                }
            }
            "image" => self.image(lines, i, next, rest),
            "init" => {
                if t.contains("python") {
                    self.python_comment(lines, i, next, indent, "Python code is not converted");
                } else if t.ends_with(':') {
                    // `init:` only orders definitions; its contents are ordinary statements.
                    self.root(lines, i + 1, next, indent);
                }
            }
            "python" => self.python_comment(lines, i, next, indent, "Python code is not converted"),
            "screen" | "transform" | "style" | "layeredimage" | "translate" | "testcase" => {
                let what = rest.split([':', '(', ' ']).next().unwrap_or("");
                self.note(
                    line.no,
                    format!(
                        "{kw} `{what}` was left out: Kulisa games build screens, styles and animation in Luau"
                    ),
                );
            }
            _ => self.comment_out(indent, line, "not converted at the top level"),
        }
    }

    /// `define`/`default` lines; Ren'Py's own settings (`config.`, `gui.`, ...) are skipped.
    fn binding(&mut self, kw: &str, rest: &str, line: &Line) -> Option<String> {
        let rest = rest.trim_start_matches(|c: char| c == '-' || c.is_ascii_digit()).trim();
        let Some((name, value)) = rest.split_once('=') else {
            self.comment_out(0, line, "not converted");
            return None;
        };
        let (name, value) = (name.trim(), value.trim());
        if name.contains('.') {
            let quiet = ["config.", "gui.", "build.", "style.", "audio.", "_game_menu", "preferences."]
                .iter()
                .any(|p| name.starts_with(p));
            if !quiet {
                self.note(line.no, format!("`{kw} {name}` was left out (only plain names are converted)"));
            }
            return None;
        }
        if !is_ident(name) {
            self.comment_out(0, line, "not converted");
            return None;
        }
        match expr::expr(value) {
            Ok(v) => Some(format!("{kw} {name} = {v}")),
            Err(e) => {
                self.note(line.no, format!("`{kw} {name}`: {e}; it is left as a comment"));
                Some(format!("# Ren'Py: {}", line.text))
            }
        }
    }

    fn image(&mut self, lines: &[Line], i: usize, next: usize, rest: &str) {
        let line = &lines[i];
        let Some((name, value)) = rest.split_once('=') else {
            if next > i + 1 || rest.ends_with(':') {
                self.note(
                    line.no,
                    format!(
                        "image `{}` uses ATL animation and was left out",
                        rest.trim_end_matches(':').trim()
                    ),
                );
            }
            return;
        };
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        match expr::tokenize(value.trim()).as_deref() {
            Ok([expr::Tok::Str(path)]) => {
                self.images.push((name, path.clone(), self.file.clone(), line.no));
            }
            _ => self.note(line.no, format!("image `{name}` is not a plain file and was left out")),
        }
    }

    /// Keeps a Python block as comments.
    fn python_comment(&mut self, lines: &[Line], i: usize, next: usize, indent: usize, why: &str) {
        self.note(lines[i].no, format!("{why}: `{}` ({} lines)", lines[i].text, next - i - 1));
        self.push(indent, format!("# Ren'Py: {}", lines[i].text));
        for l in &lines[i + 1..next] {
            let extra = l.indent.saturating_sub(lines[i].indent);
            self.push(indent, format!("#{}{}", " ".repeat(extra), l.text.replace('\n', " ")));
        }
    }

    /// A block of story statements. Returns whether it produced any.
    fn body(&mut self, lines: &[Line], start: usize, end: usize, indent: usize) -> bool {
        let mut any = false;
        let mut i = start;
        while i < end {
            let next = block_end(lines, i, end);
            any |= self.statement(lines, i, next, indent);
            i = next;
        }
        any
    }

    /// One story statement (and its block). Returns whether it produced a statement.
    fn statement(&mut self, lines: &[Line], i: usize, next: usize, indent: usize) -> bool {
        let line = &lines[i];
        let t = line.text.as_str();
        if line.is_comment() {
            self.push(indent, t);
            return false;
        }
        let (kw, rest) = first_word(t);
        // A word followed by a string is a say line, whatever the word.
        let say_like =
            t.starts_with(['"', '\'']) || (rest.starts_with(['"', '\'']) && !STATEMENTS.contains(&kw));
        if !say_like {
            match kw {
                "label" => {
                    let name = rest.split(['(', ':', ' ']).next().unwrap_or("");
                    self.note(
                        line.no,
                        format!(
                            "label `{name}` inside another label was flattened; jumps to it will not work"
                        ),
                    );
                    return self.body(lines, i + 1, next, indent);
                }
                "jump" | "call" => return self.jump(kw, rest, line, indent),
                "return" => {
                    if !rest.is_empty() {
                        self.note(line.no, "the value of `return` was dropped");
                    }
                    self.push(indent, "return");
                    return true;
                }
                "pass" => {
                    self.push(indent, "pass");
                    return true;
                }
                "menu" => return self.menu(lines, i, next, indent),
                "if" | "elif" => {
                    let cond = rest.trim_end_matches(':').trim();
                    match expr::expr(cond) {
                        Ok(c) => self.push(indent, format!("{kw} {c}:")),
                        Err(e) => {
                            self.note(
                                line.no,
                                format!("condition `{cond}`: {e}; the branch is skipped (`false`)"),
                            );
                            self.push(indent, format!("# Ren'Py: {t}"));
                            self.push(indent, format!("{kw} false:"));
                        }
                    }
                    if !self.body(lines, i + 1, next, indent + 4) {
                        self.push(indent + 4, "pass");
                    }
                    return true;
                }
                "else" => {
                    self.push(indent, "else:");
                    if !self.body(lines, i + 1, next, indent + 4) {
                        self.push(indent + 4, "pass");
                    }
                    return true;
                }
                "while" => {
                    self.python_comment(lines, i, next, indent, "`while` loops are not converted");
                    return false;
                }
                "scene" | "show" | "hide" => return self.stage(kw, rest, line, indent),
                "with" => {
                    let (tr, note) = transition(rest);
                    if let Some(n) = note {
                        self.note(line.no, n);
                    }
                    if let Some(tr) = tr {
                        self.push(indent, format!("with {tr}"));
                        return true;
                    }
                    return false;
                }
                "play" | "queue" | "voice" | "stop" => return self.audio_statement(kw, rest, line, indent),
                "pause" => {
                    let secs = rest.split_whitespace().next();
                    match secs.and_then(|s| s.parse::<f32>().ok()) {
                        Some(s) => self.push(indent, format!("pause {s}")),
                        None => self.push(indent, "pause"),
                    }
                    return true;
                }
                "window" | "nvl" => {
                    self.note(
                        line.no,
                        format!("`{t}` was dropped (the template decides how the text box shows)"),
                    );
                    return false;
                }
                "define" | "default" => {
                    if let Some(s) = self.binding(kw, rest, line) {
                        self.hoisted.push(s);
                    }
                    return false;
                }
                "image" => {
                    self.image(lines, i, next, rest);
                    return false;
                }
                "python" | "init" => {
                    return self.python_block(lines, i, next, indent);
                }
                _ => {}
            }
            if let Some(code) = t.strip_prefix('$') {
                return self.dollar(code.trim(), line, indent);
            }
        }
        self.say(line, indent)
    }

    fn jump(&mut self, kw: &str, rest: &str, line: &Line, indent: usize) -> bool {
        let (words, _) = words_until(rest, &["from"]);
        let target = words.first().copied().unwrap_or("");
        if target == "expression" || target == "screen" {
            self.comment_out(indent, line, &format!("`{kw} {target}` is not converted"));
            return false;
        }
        let name = target.split('(').next().unwrap_or("");
        if target.contains('(') {
            self.note(line.no, format!("the arguments of `{kw} {name}` were dropped"));
        }
        if !is_ident(name) {
            self.comment_out(indent, line, "not converted");
            return false;
        }
        self.push(indent, format!("{kw} {name}"));
        true
    }

    fn menu(&mut self, lines: &[Line], i: usize, next: usize, indent: usize) -> bool {
        let line = &lines[i];
        let header = line.text.trim_end_matches(':').trim();
        if header != "menu" {
            self.note(line.no, format!("`{header}`: the menu's name and arguments were dropped"));
        }
        let mark = self.out.len();
        self.push(indent, "menu:");
        let mut choices = 0;
        let mut j = i + 1;
        while j < next {
            let l = &lines[j];
            let end = block_end(lines, j, next);
            if l.is_comment() {
                self.push(indent + 4, &l.text);
            } else if l.text.starts_with("set ") {
                self.note(l.no, "the menu's `set` was dropped");
            } else if l.text.ends_with(':') {
                choices += self.choice(lines, j, end, indent + 4) as usize;
            } else {
                // The caption: a say line.
                self.say(l, indent + 4);
            }
            j = end;
        }
        if choices == 0 {
            self.out.truncate(mark);
            self.comment_out(indent, line, "a menu without choices");
            return false;
        }
        true
    }

    fn choice(&mut self, lines: &[Line], i: usize, next: usize, indent: usize) -> bool {
        let line = &lines[i];
        let chars: Vec<char> = line.text.chars().collect();
        let Ok(Some((label, after))) = expr::string_at(&chars, 0) else {
            self.comment_out(indent, line, "not a menu choice");
            return false;
        };
        let rest: String = chars[after..].iter().collect();
        let rest = rest.trim().trim_end_matches(':').trim();
        // `"Choice" (args) if cond:`: arguments are dropped.
        let rest = if rest.starts_with('(') {
            self.note(line.no, "the arguments of a menu choice were dropped");
            rest.find(')').map_or("", |p| rest[p + 1..].trim())
        } else {
            rest
        };
        let (label, tag_notes) = text::say_text(&label);
        for n in tag_notes {
            self.note(line.no, n);
        }
        let guard = match rest.strip_prefix("if ") {
            Some(cond) => match expr::expr(cond) {
                Ok(c) => format!(" if {c}"),
                Err(e) => {
                    self.note(line.no, format!("choice condition `{cond}`: {e}; the choice always shows"));
                    String::new()
                }
            },
            None => String::new(),
        };
        self.push(indent, format!("{}{guard}:", quote(&label)));
        if !self.body(lines, i + 1, next, indent + 4) {
            self.push(indent + 4, "pass");
        }
        true
    }

    fn stage(&mut self, kw: &str, rest: &str, line: &Line, indent: usize) -> bool {
        let first = rest.split_whitespace().next().unwrap_or("");
        if ["expression", "text", "screen", "layer"].contains(&first) {
            self.comment_out(indent, line, &format!("`{kw} {first}` is not converted"));
            return false;
        }
        let stops = ["at", "with", "behind", "onlayer", "zorder", "as"];
        let (image, mut tail) = words_until(rest, &stops);
        let image: Vec<&str> = image.into_iter().filter(|w| !w.starts_with('-')).collect();
        if let Some(bad) = image.iter().find(|w| !is_ident(w)) {
            self.comment_out(indent, line, &format!("image word `{bad}` is not a Kulisa name"));
            return false;
        }
        let (mut at, mut with) = (None, None);
        while let Some((&clause, rest_words)) = tail.split_first() {
            let stop = rest_words.iter().position(|w| stops.contains(w)).unwrap_or(rest_words.len());
            let value = rest_words[..stop].join(" ");
            match clause {
                "at" => {
                    let first = value.split(',').next().unwrap_or("").trim();
                    match position(first) {
                        Some(p) => at = Some(p),
                        None => self.note(line.no, format!("position `{value}` was dropped")),
                    }
                }
                "with" => {
                    let (t, note) = transition(&value);
                    if let Some(n) = note {
                        self.note(line.no, n);
                    }
                    with = t;
                }
                other => self.note(line.no, format!("`{other} {value}` was dropped")),
            }
            tail = rest_words[stop..].to_vec();
        }
        let mut s = match kw {
            "hide" => match image.first() {
                Some(tag) => format!("hide {tag}"),
                None => {
                    self.comment_out(indent, line, "not converted");
                    return false;
                }
            },
            "show" if image.is_empty() => {
                self.comment_out(indent, line, "not converted");
                return false;
            }
            _ => [kw].into_iter().chain(image.iter().copied()).collect::<Vec<_>>().join(" "),
        };
        if kw == "show"
            && let Some(at) = at
        {
            s = format!("{s} at {at}");
        }
        if let Some(t) = with {
            s = format!("{s} with {t}");
        }
        self.push(indent, s);
        true
    }

    /// The game path of an audio file named in a script: as written, under `audio/`, or a
    /// Ren'Py audio name (`define audio.x = "..."` or a file in `audio/`).
    fn audio_path(&mut self, name: &str, line: usize) -> Option<String> {
        if let Some(file) = self.audio_names.get(name).cloned() {
            return self.audio_path(&file, line);
        }
        for candidate in [name.to_string(), format!("audio/{name}")] {
            if self.files.contains(&candidate) {
                self.audio.insert(candidate.clone());
                return Some(candidate);
            }
        }
        // Ren'Py names files in audio/ after their stem (`audio/sunflower.ogg` -> sunflower).
        let by_stem = self.files.iter().find(|f| {
            f.strip_prefix("audio/")
                .and_then(|p| p.rsplit_once('.'))
                .is_some_and(|(stem, _)| stem.to_lowercase().replace([' ', '-'], "_") == name)
        });
        if let Some(f) = by_stem.cloned() {
            self.audio.insert(f.clone());
            return Some(f);
        }
        self.note(line, format!("audio file `{name}` was not found in the game"));
        None
    }

    fn audio_statement(&mut self, kw: &str, rest: &str, line: &Line, indent: usize) -> bool {
        let channel_of = |c: &str| if c == "audio" { "sound".to_string() } else { c.to_string() };
        if kw == "stop" {
            let mut words = rest.split_whitespace();
            let channel = channel_of(words.next().unwrap_or("music"));
            let fade = match (words.next(), words.next()) {
                (Some("fadeout"), Some(n)) => n.parse::<f32>().ok().map(|n| format!(" fadeout {n}")),
                _ => None,
            };
            self.push(indent, format!("stop {channel}{}", fade.unwrap_or_default()));
            return true;
        }
        let (channel, rest) = if kw == "voice" {
            if rest.trim() == "sustain" {
                return false;
            }
            ("voice".to_string(), rest)
        } else {
            let (c, r) = first_word(rest);
            (channel_of(c), r)
        };
        if kw == "queue" {
            self.note(line.no, "`queue` became `play`");
        }
        let chars: Vec<char> = rest.chars().collect();
        let (file, after) = match expr::string_at(&chars, 0) {
            Ok(Some((f, after))) => (f, chars[after..].iter().collect::<String>()),
            _ if rest.starts_with('[') => {
                let inner: Vec<char> = rest[1..].chars().collect();
                match expr::string_at(&inner, 0) {
                    Ok(Some((f, _))) => {
                        self.note(line.no, "a playlist became its first file");
                        (f, rest.split_once(']').map_or("", |(_, a)| a).to_string())
                    }
                    _ => {
                        self.comment_out(indent, line, "not converted");
                        return false;
                    }
                }
            }
            _ => {
                let (name, after) = first_word(rest);
                (name.to_string(), after.to_string())
            }
        };
        let Some(path) = self.audio_path(&file, line.no) else {
            self.push(indent, format!("# Ren'Py: {}", line.text));
            return false;
        };
        let mut s = format!("play {channel} {}", quote(&path));
        let mut words = after.split_whitespace();
        while let Some(w) = words.next() {
            match w {
                "fadein" => {
                    if let Some(n) = words.next().and_then(|n| n.parse::<f32>().ok()) {
                        s.push_str(&format!(" fadein {n}"));
                    }
                }
                "fadeout" | "volume" => {
                    words.next();
                }
                "loop" | "noloop" => s.push_str(&format!(" {w}")),
                _ => {}
            }
        }
        self.push(indent, s);
        true
    }

    fn dollar(&mut self, code: &str, line: &Line, indent: usize) -> bool {
        if let Some(args) = code.strip_prefix("renpy.pause(") {
            let secs = args.trim_end_matches(')').split(',').next().unwrap_or("").trim();
            match secs.parse::<f32>() {
                Ok(s) => self.push(indent, format!("pause {s}")),
                Err(_) => self.push(indent, "pause"),
            }
            return true;
        }
        match expr::statement(code) {
            Ok(s) if s.is_empty() => false,
            Ok(s) => {
                self.push(indent, format!("$ {s}"));
                true
            }
            Err(e) => {
                self.comment_out(indent, line, &format!("`$` line not converted ({e})"));
                false
            }
        }
    }

    /// A `python:` block inside a label: a `lua:` block when every line converts.
    fn python_block(&mut self, lines: &[Line], i: usize, next: usize, indent: usize) -> bool {
        let base = lines.get(i + 1).map_or(0, |l| l.indent);
        let mut converted = Vec::new();
        for l in &lines[i + 1..next] {
            if l.is_comment() {
                continue;
            }
            if l.indent != base {
                converted.clear();
                break;
            }
            match expr::statement(&l.text) {
                Ok(s) if s.is_empty() => {}
                Ok(s) => converted.push(s),
                Err(_) => {
                    converted.clear();
                    break;
                }
            }
        }
        if converted.is_empty() {
            self.python_comment(lines, i, next, indent, "Python code is not converted");
            return false;
        }
        self.push(indent, "lua:");
        for s in converted {
            self.push(indent + 4, s);
        }
        true
    }

    /// A say line: `who [attributes] "text" [with t]`, `"Name" "text"` or `"text"`.
    fn say(&mut self, line: &Line, indent: usize) -> bool {
        let chars: Vec<char> = line.text.chars().collect();
        // The words before the text.
        let mut i = 0;
        let mut words: Vec<String> = Vec::new();
        let mut speaker_name: Option<String> = None;
        let text = loop {
            while chars.get(i).is_some_and(|c| c.is_whitespace()) {
                i += 1;
            }
            match expr::string_at(&chars, i) {
                Ok(Some((s, after))) => {
                    i = after;
                    // `"Name" "text"`: a string speaker.
                    let mut j = after;
                    while chars.get(j).is_some_and(|c| c.is_whitespace()) {
                        j += 1;
                    }
                    if words.is_empty()
                        && speaker_name.is_none()
                        && let Ok(Some((t, after2))) = expr::string_at(&chars, j)
                    {
                        speaker_name = Some(s);
                        i = after2;
                        break t;
                    }
                    break s;
                }
                Ok(None) => {}
                Err(e) => {
                    self.comment_out(indent, line, &format!("not converted ({e})"));
                    return false;
                }
            }
            let start = i;
            while chars.get(i).is_some_and(|c| !c.is_whitespace() && *c != '"' && *c != '\'') {
                i += 1;
            }
            if i == start {
                self.comment_out(indent, line, "not converted");
                return false;
            }
            words.push(chars[start..i].iter().collect());
        };
        let after: String = chars[i..].iter().collect();
        let after = after.trim();
        if let Some(t) = after.strip_prefix("with ") {
            self.note(line.no, format!("the say line's `with {}` was dropped", t.trim()));
        } else if !after.is_empty()
            && !after.starts_with('(')
            && !after.starts_with("id ")
            && after != "nointeract"
        {
            self.note(line.no, format!("`{after}` after the text was dropped"));
        }

        let (body, tag_notes) = text::say_text(&text);
        for n in tag_notes {
            self.note(line.no, n);
        }
        let who: Option<String> = match (speaker_name, words.first().map(String::as_str)) {
            (Some(name), _) => {
                let mut id: String = name
                    .to_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                id = format!("speaker_{}", id.trim_matches('_'));
                self.speakers.entry(name).or_insert(id).clone().into()
            }
            (None, None) => None,
            (None, Some("narrator" | "centered")) => None,
            (None, Some(w)) if self.nameless.contains(w) => None,
            (None, Some("extend")) => {
                self.note(line.no, "`extend` became a new line by the same speaker");
                self.last_speaker.clone()
            }
            (None, Some(w)) if is_ident(w) => {
                let attributes: Vec<&str> =
                    words[1..].iter().map(String::as_str).filter(|a| is_ident(a)).collect();
                if !attributes.is_empty() {
                    let tag = self.char_images.get(w).cloned().unwrap_or_else(|| w.to_string());
                    self.push(indent, format!("show {tag} {}", attributes.join(" ")));
                }
                Some(w.to_string())
            }
            (None, Some(w)) => {
                self.comment_out(indent, line, &format!("speaker `{w}` is not converted"));
                return false;
            }
        };
        if words.first().is_none_or(|w| w != "extend") {
            self.last_speaker = who.clone();
        }
        match who {
            Some(w) => self.push(indent, format!("{w} {}", quote(&body))),
            None => self.push(indent, quote(&body)),
        }
        true
    }
}

/// A converted project.
#[derive(Debug, Default)]
pub struct Import {
    /// Story files: path in the new game (`story/script.kulisa`) and source.
    pub stories: Vec<(String, String)>,
    /// Files to copy: from the Ren'Py game, to a path in the new game.
    pub copies: Vec<(PathBuf, String)>,
    pub info: GameInfo,
    pub notes: Vec<Note>,
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            // Translations, caches and saves are not the game's script.
            if dir == base && ["tl", "cache", "saves"].contains(&name.as_str()) {
                continue;
            }
            walk(&path, base, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// Reads a Ren'Py project (the folder with `game/`, or `game/` itself).
pub fn import(project: &Path) -> Result<Import, String> {
    let game = if project.join("game").is_dir() { project.join("game") } else { project.to_path_buf() };
    let mut files = Vec::new();
    walk(&game, &game, &mut files);
    files.sort();
    let scripts: Vec<&String> = files.iter().filter(|f| f.ends_with(".rpy")).collect();
    if scripts.is_empty() {
        return Err(format!("{} has no .rpy scripts", game.display()));
    }
    let mut conv = Converter::new(files.iter().cloned().collect());
    let mut sources = Vec::new();
    for f in &scripts {
        let src = std::fs::read_to_string(game.join(f)).map_err(|e| format!("{f}: {e}"))?;
        conv.prescan(&src);
        sources.push((f.to_string(), src));
    }
    let mut out = Import::default();
    for (f, src) in &sources {
        let kulisa = conv.convert(f, src);
        // Ren'Py's generated gui, screens and options files hold nothing for the story.
        let has_story = kulisa.lines().any(|l| {
            let t = l.trim_start();
            !t.is_empty() && !t.starts_with('#')
        });
        if has_story {
            let name = f.trim_end_matches(".rpy");
            out.stories.push((format!("story/{name}.kulisa"), kulisa));
        }
    }
    if let Some(s) = conv.speakers_file() {
        out.stories.push(("story/speakers.kulisa".into(), s));
    }

    // Images: every file under images/ is named after its file name (folders ignored,
    // lowercase), as Ren'Py does; `image` statements name the others.
    let mut taken = BTreeMap::new();
    for f in files.iter().filter(|f| f.starts_with("images/")) {
        let file_name = f.rsplit('/').next().unwrap_or(f);
        let Some((stem, ext)) = file_name.rsplit_once('.') else { continue };
        let ext = ext.to_lowercase();
        if !IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let name = stem.to_lowercase();
        if let Some(first) = taken.insert(name.clone(), f.clone()) {
            conv.file = f.clone();
            conv.note(
                0,
                format!("image `{name}` is defined by both {first} and {f}; the second was skipped"),
            );
            taken.insert(name, first);
            continue;
        }
        out.copies.push((game.join(f), format!("images/{name}.{ext}")));
    }
    for (name, path, file, line) in conv.images.clone() {
        let src = [path.clone(), format!("images/{path}")].into_iter().find(|p| files.contains(p));
        conv.file = file;
        match src {
            Some(src) => {
                let ext = src.rsplit_once('.').map_or("png", |(_, e)| e).to_lowercase();
                if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                    out.copies.push((game.join(&src), format!("images/{name}.{ext}")));
                } else {
                    conv.note(line, format!("image `{name}`: `.{ext}` images are not supported"));
                }
            }
            None => conv.note(line, format!("image `{name}`: `{path}` was not found")),
        }
    }
    for a in &conv.audio {
        out.copies.push((game.join(a), a.clone()));
    }
    out.info = conv.info.clone();
    out.notes = std::mem::take(&mut conv.notes);
    out.notes.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    Ok(out)
}

#[cfg(test)]
mod tests;
