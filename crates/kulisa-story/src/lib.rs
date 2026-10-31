//! The `.kulisa` story language: lexer, parser, compiler and VM.

pub mod ast;
mod compile;
mod diag;
mod lang;
mod lex;
mod parse;
mod vm;

pub use compile::{
    ChoiceOp, Chunk, ChunkId, ChunkKind, LabelCode, Op, Program, SayOp, StmtInfo, TextPart, compile,
    compile_translated,
};
pub use diag::{Diagnostic, Diagnostics, Loc};
pub use lang::{
    LangEntry, LangReport, LangSource, Language, lang_key, parse_language, script_strings, story_strings,
    update_language, valid_lang_code,
};
pub use parse::parse;
pub use vm::{Event, Host, Line, MenuItem, RuntimeError, Shown, Speaker, Stage, Track, Vm};
