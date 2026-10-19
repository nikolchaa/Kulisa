//! Syntax tree of one `.kulisa` file. Luau parts (conditions, `$` lines, `lua:` blocks,
//! `define`/`default` values) are kept as source text and compiled by the host.

use crate::Loc;

/// Words that start statements; they cannot be character names.
pub const KEYWORDS: &[&str] = &[
    "label", "define", "default", "jump", "call", "return", "menu", "if", "elif", "else", "scene", "show",
    "hide", "with", "play", "stop", "pause", "lua",
];

#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub name: String,
    pub defines: Vec<Binding>,
    pub defaults: Vec<Binding>,
    /// Top-level `lua:` blocks, run once at start.
    pub init: Vec<Code>,
    pub labels: Vec<Label>,
}

/// `define name = expr` (constant) or `default name = expr` (saved story variable).
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub name: String,
    pub expr: String,
    pub loc: Loc,
    pub expr_loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Code {
    pub code: String,
    pub loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub name: String,
    pub loc: Loc,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub loc: Loc,
    /// The statement's first line, trimmed; its hash identifies the statement in saves.
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Say {
    /// Character name and where it was written.
    pub who: Option<(String, Loc)>,
    pub text: String,
    pub text_loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub text: String,
    pub text_loc: Loc,
    pub guard: Option<(String, Loc)>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Branch {
    /// `None` for `else`.
    pub cond: Option<(String, Loc)>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Say(Say),
    Menu {
        caption: Option<Say>,
        choices: Vec<Choice>,
    },
    If {
        branches: Vec<Branch>,
    },
    Jump {
        target: String,
        target_loc: Loc,
    },
    Call {
        target: String,
        target_loc: Loc,
    },
    Return,
    /// `$ code` or a `lua:` block.
    Lua(Code),
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
