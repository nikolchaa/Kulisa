//! Compiler: syntax trees of all story files into one [`Program`] of per-label bytecode.
//!
//! Luau parts become [`Chunk`]s that the host compiles; the VM refers to them by id.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diag::{Diagnostic, Diagnostics, Loc, suggest};

pub type ChunkId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkKind {
    /// Evaluates to a value: conditions, `[interpolation]`, `define`/`default` values,
    /// character names.
    Expr,
    /// Runs for its effects: `$` lines and `lua:` blocks.
    Stmt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub id: ChunkId,
    pub kind: ChunkKind,
    pub source: String,
    pub loc: Loc,
}

/// Dialogue text: literal parts and `[expression]` parts.
#[derive(Clone, Debug, PartialEq)]
pub enum TextPart {
    Lit(String),
    Expr(ChunkId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SayOp {
    /// Chunk evaluating the character (`define`d value), and its name in the script.
    pub who: Option<(ChunkId, String)>,
    pub text: Vec<TextPart>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceOp {
    pub text: Vec<TextPart>,
    pub guard: Option<ChunkId>,
    /// Op index the choice continues at.
    pub target: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Say(SayOp),
    Menu {
        caption: Option<SayOp>,
        choices: Vec<ChoiceOp>,
    },
    Exec(ChunkId),
    /// Continues at `target` when `cond` is falsy (nil or false).
    JumpIfNot {
        cond: ChunkId,
        target: u32,
    },
    Goto(u32),
    Jump(String),
    Call(String),
    Return,
    Scene {
        image: Vec<String>,
        with: Option<String>,
    },
    Show {
        image: Vec<String>,
        at: Option<String>,
        with: Option<String>,
    },
    Hide {
        tag: String,
        with: Option<String>,
    },
    With(String),
    Play {
        channel: String,
        file: String,
        fade: Option<f32>,
        looping: Option<bool>,
    },
    Stop {
        channel: String,
        fade: Option<f32>,
    },
    Pause(Option<f32>),
}

impl Op {
    /// Ops that wait for the player (and where checkpoints and saves happen).
    pub fn blocks(&self) -> bool {
        matches!(self, Op::Say(_) | Op::Menu { .. } | Op::Pause(_))
    }
}

/// One source statement inside a label.
#[derive(Clone, Debug, PartialEq)]
pub struct StmtInfo {
    pub first_op: u32,
    /// FNV-1a of the statement's first line; identifies it across script edits.
    pub hash: u64,
    pub loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LabelCode {
    pub name: String,
    pub loc: Loc,
    pub ops: Vec<Op>,
    pub stmts: Vec<StmtInfo>,
    /// Statement index of each op.
    pub op_stmt: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Program {
    pub labels: HashMap<String, LabelCode>,
    pub chunks: Vec<Chunk>,
    /// `define` constants, in source order.
    pub defines: Vec<(String, ChunkId)>,
    /// `default` story variables, in source order.
    pub defaults: Vec<(String, ChunkId)>,
    /// Top-level `lua:` blocks, in source order.
    pub init: Vec<ChunkId>,
}

impl Program {
    pub fn chunk(&self, id: ChunkId) -> &Chunk {
        &self.chunks[id as usize]
    }
}

pub(crate) fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

struct Compiler<'a> {
    program: Program,
    errors: Vec<Diagnostic>,
    labels: HashSet<String>,
    characters: HashSet<String>,
    lines: &'a HashMap<String, Vec<String>>,
}

/// Compiles story files (`(name, source)` pairs) into one program.
pub fn compile(files: &[(&str, &str)]) -> Result<Program, Diagnostics> {
    let mut errors = Vec::new();
    let mut parsed = Vec::new();
    for (name, src) in files {
        match crate::parse(name, src) {
            Ok(f) => parsed.push(f),
            Err(e) => errors.extend(e),
        }
    }
    let lines: HashMap<String, Vec<String>> =
        files.iter().map(|(n, s)| (n.to_string(), s.lines().map(str::to_string).collect())).collect();
    let mut c = Compiler {
        program: Program::default(),
        errors,
        labels: HashSet::new(),
        characters: HashSet::new(),
        lines: &lines,
    };
    c.declare(&parsed);
    for f in &parsed {
        for l in &f.labels {
            if !c.program.labels.contains_key(&l.name) {
                let code = c.label(l);
                c.program.labels.insert(l.name.clone(), code);
            }
        }
    }
    if c.errors.is_empty() { Ok(c.program) } else { Err(Diagnostics(c.errors)) }
}

impl Compiler<'_> {
    fn line(&self, loc: &Loc) -> &str {
        self.lines.get(&loc.file).and_then(|l| l.get(loc.line - 1)).map_or("", String::as_str)
    }

    fn error(&mut self, loc: &Loc, msg: impl Into<String>) {
        let d = Diagnostic::new(loc.clone(), msg, self.line(loc));
        self.errors.push(d);
    }

    fn chunk(&mut self, kind: ChunkKind, source: &str, loc: &Loc) -> ChunkId {
        let id = self.program.chunks.len() as ChunkId;
        self.program.chunks.push(Chunk { id, kind, source: source.to_string(), loc: loc.clone() });
        id
    }

    /// Collects labels, defines and defaults across files and reports duplicates.
    fn declare(&mut self, files: &[File]) {
        let mut label_locs: HashMap<String, Loc> = HashMap::new();
        let mut names: HashMap<String, Loc> = HashMap::new();
        for f in files {
            for l in &f.labels {
                if let Some(first) = label_locs.get(&l.name) {
                    let msg =
                        format!("label `{}` is already defined at {}:{}", l.name, first.file, first.line);
                    self.error(&l.loc, msg);
                } else {
                    label_locs.insert(l.name.clone(), l.loc.clone());
                }
            }
            for (bindings, is_define) in [(&f.defines, true), (&f.defaults, false)] {
                for b in bindings {
                    if let Some(first) = names.get(&b.name) {
                        let msg = format!("`{}` is already defined at {}:{}", b.name, first.file, first.line);
                        self.error(&b.loc, msg);
                        continue;
                    }
                    names.insert(b.name.clone(), b.loc.clone());
                    let id = self.chunk(ChunkKind::Expr, &b.expr, &b.expr_loc);
                    if is_define {
                        self.characters.insert(b.name.clone());
                        self.program.defines.push((b.name.clone(), id));
                    } else {
                        self.program.defaults.push((b.name.clone(), id));
                    }
                }
            }
            for code in &f.init {
                let id = self.chunk(ChunkKind::Stmt, &code.code, &code.loc);
                self.program.init.push(id);
            }
        }
        self.labels = label_locs.into_keys().collect();
    }

    fn label(&mut self, l: &Label) -> LabelCode {
        let mut code = LabelCode {
            name: l.name.clone(),
            loc: l.loc.clone(),
            ops: vec![],
            stmts: vec![],
            op_stmt: vec![],
        };
        self.block(&l.body, &mut code);
        // Falling off the end of a label returns (or ends the story when nothing called it).
        let end_stmt = code.stmts.len().saturating_sub(1) as u32;
        code.ops.push(Op::Return);
        code.op_stmt.push(end_stmt);
        code
    }

    fn emit(code: &mut LabelCode, op: Op) -> u32 {
        code.ops.push(op);
        code.op_stmt.push(code.stmts.len().saturating_sub(1) as u32);
        code.ops.len() as u32 - 1
    }

    fn patch(code: &mut LabelCode, at: u32, to: u32) {
        match &mut code.ops[at as usize] {
            Op::Goto(t) | Op::JumpIfNot { target: t, .. } => *t = to,
            _ => unreachable!("only jumps are patched"),
        }
    }

    fn block(&mut self, body: &[Stmt], code: &mut LabelCode) {
        for s in body {
            code.stmts.push(StmtInfo {
                first_op: code.ops.len() as u32,
                hash: fnv1a(&s.source),
                loc: s.loc.clone(),
            });
            self.stmt(s, code);
        }
    }

    fn target(&mut self, name: &str, loc: &Loc) {
        if !self.labels.contains(name) {
            let hint = suggest(name, self.labels.iter().map(String::as_str))
                .map(|s| format!("; did you mean `{s}`?"))
                .unwrap_or_default();
            self.error(loc, format!("unknown label `{name}`{hint}"));
        }
    }

    fn text(&mut self, text: &str, loc: &Loc) -> Vec<TextPart> {
        let mut parts = Vec::new();
        let mut lit = String::new();
        let mut rest = text;
        while let Some(i) = rest.find('[') {
            lit.push_str(&rest[..i]);
            rest = &rest[i + 1..];
            if let Some(r) = rest.strip_prefix('[') {
                lit.push('[');
                rest = r;
                continue;
            }
            // The matching `]`, so indexing like `[bag[2]]` works.
            let mut depth = 0usize;
            let end = rest.char_indices().find_map(|(i, c)| match c {
                '[' => {
                    depth += 1;
                    None
                }
                ']' if depth == 0 => Some(i),
                ']' => {
                    depth -= 1;
                    None
                }
                _ => None,
            });
            let Some(end) = end else {
                self.error(loc, "`[` starts an interpolation that is never closed with `]` (write `[[` for a literal bracket)");
                return parts;
            };
            let expr = rest[..end].trim();
            if expr.is_empty() {
                self.error(loc, "empty `[]` in text; put an expression inside, like `[trust]`");
            }
            if !lit.is_empty() {
                parts.push(TextPart::Lit(std::mem::take(&mut lit)));
            }
            parts.push(TextPart::Expr(self.chunk(ChunkKind::Expr, expr, loc)));
            rest = &rest[end + 1..];
        }
        lit.push_str(rest);
        if !lit.is_empty() || parts.is_empty() {
            parts.push(TextPart::Lit(lit));
        }
        parts
    }

    fn say(&mut self, s: &Say) -> SayOp {
        let who = s.who.as_ref().map(|(name, loc)| {
            if !self.characters.contains(name) {
                let hint = suggest(name, self.characters.iter().map(String::as_str))
                    .map(|c| format!("; did you mean `{c}`?"))
                    .unwrap_or_else(|| {
                        format!("; define it first, like `define {name} = Character(\"Name\")`")
                    });
                self.error(loc, format!("unknown character `{name}`{hint}"));
            }
            (self.chunk(ChunkKind::Expr, name, loc), name.clone())
        });
        SayOp { who, text: self.text(&s.text, &s.text_loc) }
    }

    fn stmt(&mut self, s: &Stmt, code: &mut LabelCode) {
        match &s.kind {
            StmtKind::Say(say) => {
                let op = self.say(say);
                Self::emit(code, Op::Say(op));
            }
            StmtKind::Menu { caption, choices } => {
                let caption = caption.as_ref().map(|c| self.say(c));
                let ops: Vec<ChoiceOp> = choices
                    .iter()
                    .map(|c| ChoiceOp {
                        text: self.text(&c.text, &c.text_loc),
                        guard: c.guard.as_ref().map(|(g, loc)| self.chunk(ChunkKind::Expr, g, loc)),
                        target: 0,
                    })
                    .collect();
                let menu = Self::emit(code, Op::Menu { caption, choices: ops });
                let mut ends = Vec::new();
                for (i, choice) in choices.iter().enumerate() {
                    let start = code.ops.len() as u32;
                    if let Op::Menu { choices, .. } = &mut code.ops[menu as usize] {
                        choices[i].target = start;
                    }
                    self.block(&choice.body, code);
                    ends.push(Self::emit(code, Op::Goto(0)));
                }
                let end = code.ops.len() as u32;
                ends.into_iter().for_each(|g| Self::patch(code, g, end));
            }
            StmtKind::If { branches } => {
                let mut ends = Vec::new();
                for b in branches {
                    let skip = b.cond.as_ref().map(|(cond, loc)| {
                        let id = self.chunk(ChunkKind::Expr, cond, loc);
                        Self::emit(code, Op::JumpIfNot { cond: id, target: 0 })
                    });
                    self.block(&b.body, code);
                    if let Some(skip) = skip {
                        ends.push(Self::emit(code, Op::Goto(0)));
                        let next = code.ops.len() as u32;
                        Self::patch(code, skip, next);
                    }
                }
                let end = code.ops.len() as u32;
                ends.into_iter().for_each(|g| Self::patch(code, g, end));
            }
            StmtKind::Jump { target, target_loc } => {
                self.target(target, target_loc);
                Self::emit(code, Op::Jump(target.clone()));
            }
            StmtKind::Call { target, target_loc } => {
                self.target(target, target_loc);
                Self::emit(code, Op::Call(target.clone()));
            }
            StmtKind::Return => {
                Self::emit(code, Op::Return);
            }
            StmtKind::Lua(c) => {
                let id = self.chunk(ChunkKind::Stmt, &c.code, &c.loc);
                Self::emit(code, Op::Exec(id));
            }
            StmtKind::Scene { image, with } => {
                Self::emit(code, Op::Scene { image: image.clone(), with: with.clone() });
            }
            StmtKind::Show { image, at, with } => {
                Self::emit(code, Op::Show { image: image.clone(), at: at.clone(), with: with.clone() });
            }
            StmtKind::Hide { tag, with } => {
                Self::emit(code, Op::Hide { tag: tag.clone(), with: with.clone() });
            }
            StmtKind::With(t) => {
                Self::emit(code, Op::With(t.clone()));
            }
            StmtKind::Play { channel, file, fade, looping } => {
                let op =
                    Op::Play { channel: channel.clone(), file: file.clone(), fade: *fade, looping: *looping };
                Self::emit(code, op);
            }
            StmtKind::Stop { channel, fade } => {
                Self::emit(code, Op::Stop { channel: channel.clone(), fade: *fade });
            }
            StmtKind::Pause(secs) => {
                Self::emit(code, Op::Pause(*secs));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_one(src: &str) -> Program {
        compile(&[("t.kulisa", src)]).unwrap_or_else(|e| panic!("{e}"))
    }

    fn errors(src: &str) -> String {
        compile(&[("t.kulisa", src)]).unwrap_err().to_string()
    }

    #[test]
    fn lays_out_menus_and_ifs() {
        let p = compile_one(
            "define m = Character(\"M\")\nlabel start:\n    menu:\n        \"A\":\n            m \"a\"\n        \"B\" if ok:\n            \"b\"\n    if x:\n        \"x\"\n    else:\n        \"y\"\n    \"end\"\n",
        );
        let ops = &p.labels["start"].ops;
        let Op::Menu { choices, .. } = &ops[0] else { panic!() };
        assert_eq!(choices[0].target, 1);
        assert!(matches!(ops[1], Op::Say(_)));
        assert_eq!(ops[2], Op::Goto(5));
        assert_eq!(choices[1].target, 3);
        assert!(choices[1].guard.is_some());
        assert_eq!(ops[4], Op::Goto(5));
        assert!(matches!(ops[5], Op::JumpIfNot { target: 8, .. }));
        assert_eq!(ops[7], Op::Goto(9));
        assert!(matches!(&ops[9], Op::Say(s) if s.text == vec![TextPart::Lit("end".into())]));
        assert_eq!(ops[10], Op::Return, "implicit return at the end of a label");
        // Statements: menu, m "a", "b", if, "x", "y", "end".
        let firsts: Vec<u32> = p.labels["start"].stmts.iter().map(|s| s.first_op).collect();
        assert_eq!(firsts, vec![0, 1, 3, 5, 6, 8, 9]);
    }

    #[test]
    fn interpolates_text() {
        let p = compile_one("label s:\n    \"Trust: [trust]. [[not this]] [ a.b ]![bag[2]]\"\n");
        let Op::Say(s) = &p.labels["s"].ops[0] else { panic!() };
        assert_eq!(s.text.len(), 6);
        assert_eq!(s.text[0], TextPart::Lit("Trust: ".into()));
        let TextPart::Expr(id) = s.text[1] else { panic!() };
        assert_eq!(p.chunk(id).source, "trust");
        assert_eq!(s.text[2], TextPart::Lit(". [not this]] ".into()));
        let TextPart::Expr(id) = s.text[3] else { panic!() };
        assert_eq!(p.chunk(id).source, "a.b");
        let TextPart::Expr(id) = s.text[5] else { panic!() };
        assert_eq!(p.chunk(id).source, "bag[2]", "brackets nest");
    }

    #[test]
    fn hashes_identify_statements() {
        let a = compile_one("label s:\n    \"one\"\n    \"two\"\n");
        let b = compile_one("label s:\n    \"new line\"\n    \"one\"\n    \"two\"\n");
        let (ha, hb) = (&a.labels["s"].stmts, &b.labels["s"].stmts);
        assert_eq!(ha[1].hash, hb[2].hash, "same text, same hash, wherever it moved");
        assert_ne!(ha[0].hash, hb[0].hash);
    }

    #[test]
    fn resolves_names_with_suggestions() {
        let e = errors(
            "define mila = 1\nlabel start:\n    jump cafe_tak\n    mlia \"hi\"\n    stranger \"hi\"\nlabel cafe_talk:\n    \"x\"\n",
        );
        assert!(
            e.contains("t.kulisa:3:10: error: unknown label `cafe_tak`; did you mean `cafe_talk`?"),
            "{e}"
        );
        assert!(e.contains("unknown character `mlia`; did you mean `mila`?"), "{e}");
        assert!(e.contains("unknown character `stranger`; define it first"), "{e}");
    }

    #[test]
    fn reports_duplicates_across_files() {
        let e = compile(&[
            ("a.kulisa", "label start:\n    \"a\"\n"),
            ("b.kulisa", "label start:\n    \"b\"\ndefault x = 1\ndefault x = 2\n"),
        ])
        .unwrap_err()
        .to_string();
        assert!(e.contains("b.kulisa:1:1: error: label `start` is already defined at a.kulisa:1"), "{e}");
        assert!(e.contains("`x` is already defined at b.kulisa:3"), "{e}");
    }

    #[test]
    fn collects_defines_defaults_and_init() {
        let p = compile_one("define a = 1\ndefault b = a + 1\nlua:\n    print(1)\nlabel s:\n    $ b += 1\n");
        assert_eq!(p.defines[0].0, "a");
        assert_eq!(p.chunk(p.defaults[0].1).source, "a + 1");
        assert_eq!(p.chunk(p.init[0]).kind, ChunkKind::Stmt);
        assert!(matches!(p.labels["s"].ops[0], Op::Exec(_)));
    }
}
