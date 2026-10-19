//! The story VM: runs bytecode until it needs the player, records checkpoints for
//! rollback, and saves positions that survive script edits.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;

use kulisa_core::Value;

use crate::compile::{Chunk, ChunkId, Op, Program, SayOp, TextPart};
use crate::diag::Loc;

/// Evaluates the Luau parts of a story. `kulisa-lua` implements it; tests use a fake.
pub trait Host {
    /// Binds a `define` constant (not saved).
    fn define(&mut self, name: &str, chunk: &Chunk) -> Result<(), String>;
    /// Sets a story variable from `chunk` unless it already has a value.
    fn set_default(&mut self, name: &str, chunk: &Chunk) -> Result<(), String>;
    fn eval(&mut self, chunk: &Chunk) -> Result<Value, String>;
    fn exec(&mut self, chunk: &Chunk) -> Result<(), String>;
    /// Story variables as plain data (for checkpoints and saves).
    fn snapshot(&mut self) -> Result<Value, String>;
    fn restore(&mut self, vars: &Value) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Speaker {
    /// The name used in the script (`mila`).
    pub id: String,
    /// What it was defined as (e.g. a table with `name` and `color`).
    pub data: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub who: Option<Speaker>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    /// Pass this to [`Vm::choose`].
    pub index: usize,
    pub text: String,
}

/// What the runtime should do next.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Show a line and wait for the player.
    Say(Line),
    /// Show choices and wait for [`Vm::choose`]. Choices whose condition is false are left out.
    Menu {
        caption: Option<Line>,
        items: Vec<MenuItem>,
    },
    /// Wait for the player, or `secs` if given.
    Pause(Option<f32>),
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
        looping: bool,
    },
    Stop {
        channel: String,
        fade: Option<f32>,
    },
    /// The story finished.
    End,
}

impl Event {
    /// Whether the runtime must wait before calling [`Vm::next`] again.
    pub fn blocks(&self) -> bool {
        matches!(self, Event::Say(_) | Event::Menu { .. } | Event::Pause(_) | Event::End)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub image: Vec<String>,
    /// Named position (`left`), resolved by the runtime for the current display.
    pub at: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub file: String,
    pub looping: bool,
}

/// What is on stage, as plain data. The runtime rebuilds visuals and music from it after a
/// rollback or load. Positions stay named, so saves are portable across display profiles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stage {
    pub scene: Option<Vec<String>>,
    /// In draw order; at most one entry per tag (the image's first word).
    pub shown: Vec<Shown>,
    /// Looping tracks per channel.
    pub tracks: BTreeMap<String, Track>,
}

fn strings(v: &Value) -> Vec<String> {
    v.as_list().iter().filter_map(|s| s.as_str().map(str::to_string)).collect()
}

fn list(items: &[String]) -> Value {
    Value::List(items.iter().map(|s| s.as_str().into()).collect())
}

impl Stage {
    pub fn to_value(&self) -> Value {
        let shown = self
            .shown
            .iter()
            .map(|s| Value::map().with("image", list(&s.image)).with("at", s.at.clone()))
            .collect::<Vec<_>>();
        let tracks = self
            .tracks
            .iter()
            .map(|(ch, t)| (ch.clone(), Value::map().with("file", t.file.as_str()).with("loop", t.looping)))
            .collect();
        Value::map()
            .with("scene", self.scene.as_deref().map(list))
            .with("shown", Value::List(shown))
            .with("tracks", Value::Map(tracks))
    }

    pub fn from_value(v: &Value) -> Stage {
        let scene = match v.get("scene") {
            Value::Nil => None,
            s => Some(strings(s)),
        };
        let shown = v
            .get("shown")
            .as_list()
            .iter()
            .map(|s| Shown { image: strings(s.get("image")), at: s.get("at").as_str().map(str::to_string) })
            .filter(|s| !s.image.is_empty())
            .collect();
        let tracks = match v.get("tracks") {
            Value::Map(m) => m
                .iter()
                .filter_map(|(ch, t)| {
                    let file = t.get("file").as_str()?.to_string();
                    Some((ch.clone(), Track { file, looping: t.get("loop").as_bool().unwrap_or(true) }))
                })
                .collect(),
            _ => BTreeMap::new(),
        };
        Stage { scene, shown, tracks }
    }
}

/// An error while running the story, located in the script when possible.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeError {
    pub loc: Option<Loc>,
    pub message: String,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.loc {
            Some(l) => write!(f, "{}:{}:{}: error: {}", l.file, l.line, l.col, self.message),
            None => write!(f, "error: {}", self.message),
        }
    }
}

impl std::error::Error for RuntimeError {}

fn err(message: impl Into<String>) -> RuntimeError {
    RuntimeError { loc: None, message: message.into() }
}

#[derive(Clone, Debug, PartialEq)]
struct Frame {
    label: String,
    pc: u32,
}

#[derive(Clone, Debug, PartialEq)]
struct Checkpoint {
    frames: Vec<Frame>,
    stage: Stage,
    vars: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    NotStarted,
    Running,
    /// At a blocking op that has been shown; `next` moves past it.
    Waiting,
    /// At a menu; only `choose` moves on.
    Choosing,
    /// At a blocking op restored by rollback or load; `next` shows it again.
    Restored,
    Ended,
}

/// Runs one story. Cheap to create; the program is shared.
pub struct Vm {
    program: Arc<Program>,
    frames: Vec<Frame>,
    stage: Stage,
    state: State,
    history: VecDeque<Checkpoint>,
    redo: Vec<Checkpoint>,
    /// Checkpoints kept for rollback.
    pub max_history: usize,
}

/// Lua truthiness: only nil and false are false.
fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Bool(false))
}

/// How interpolated values read in dialogue.
fn display(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        Value::Float(f) => f.to_string(),
        Value::Str(s) => s.clone(),
        Value::Bytes(_) => "[bytes]".into(),
        Value::List(_) | Value::Map(_) => "[table]".into(),
    }
}

impl Vm {
    pub fn new(program: Arc<Program>) -> Self {
        Self {
            program,
            frames: Vec::new(),
            stage: Stage::default(),
            state: State::NotStarted,
            history: VecDeque::new(),
            redo: Vec::new(),
            max_history: 100,
        }
    }

    pub fn program(&self) -> &Arc<Program> {
        &self.program
    }

    /// What is on stage now.
    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    pub fn is_ended(&self) -> bool {
        self.state == State::Ended
    }

    fn chunk(&self, id: ChunkId) -> &Chunk {
        self.program.chunk(id)
    }

    fn host_err(&self, id: ChunkId, message: String) -> RuntimeError {
        RuntimeError { loc: Some(self.chunk(id).loc.clone()), message }
    }

    fn eval(&self, host: &mut dyn Host, id: ChunkId) -> Result<Value, RuntimeError> {
        host.eval(self.chunk(id)).map_err(|e| self.host_err(id, e))
    }

    /// Binds defines, applies defaults, runs init blocks and enters `label`.
    pub fn start(&mut self, host: &mut dyn Host, label: &str) -> Result<(), RuntimeError> {
        let program = self.program.clone();
        for (name, id) in &program.defines {
            host.define(name, program.chunk(*id)).map_err(|e| self.host_err(*id, e))?;
        }
        self.apply_defaults(host)?;
        for id in &program.init {
            host.exec(program.chunk(*id)).map_err(|e| self.host_err(*id, e))?;
        }
        if !program.labels.contains_key(label) {
            return Err(err(format!("there is no label `{label}` to start at")));
        }
        self.frames = vec![Frame { label: label.to_string(), pc: 0 }];
        self.stage = Stage::default();
        self.history.clear();
        self.redo.clear();
        self.state = State::Running;
        Ok(())
    }

    fn apply_defaults(&self, host: &mut dyn Host) -> Result<(), RuntimeError> {
        for (name, id) in &self.program.defaults {
            host.set_default(name, self.chunk(*id)).map_err(|e| self.host_err(*id, e))?;
        }
        Ok(())
    }

    fn op(&self) -> Result<&Op, RuntimeError> {
        let f = self.frames.last().ok_or_else(|| err("the story is not running"))?;
        let code = self
            .program
            .labels
            .get(&f.label)
            .ok_or_else(|| err(format!("label `{}` is missing", f.label)))?;
        code.ops
            .get(f.pc as usize)
            .ok_or_else(|| err(format!("position {} is outside label `{}`", f.pc, f.label)))
    }

    fn text(&self, host: &mut dyn Host, parts: &[TextPart]) -> Result<String, RuntimeError> {
        let mut out = String::new();
        for p in parts {
            match p {
                TextPart::Lit(s) => out.push_str(s),
                TextPart::Expr(id) => out.push_str(&display(&self.eval(host, *id)?)),
            }
        }
        Ok(out)
    }

    fn line(&self, host: &mut dyn Host, say: &SayOp) -> Result<Line, RuntimeError> {
        let who = match &say.who {
            Some((id, name)) => Some(Speaker { id: name.clone(), data: self.eval(host, *id)? }),
            None => None,
        };
        Ok(Line { who, text: self.text(host, &say.text)? })
    }

    /// Builds the event for the blocking op at the current position.
    fn blocking_event(&self, host: &mut dyn Host) -> Result<Event, RuntimeError> {
        Ok(match self.op()?.clone() {
            Op::Say(s) => Event::Say(self.line(host, &s)?),
            Op::Pause(secs) => Event::Pause(secs),
            Op::Menu { caption, choices } => {
                let caption = caption.as_ref().map(|c| self.line(host, c)).transpose()?;
                let mut items = Vec::new();
                for (index, c) in choices.iter().enumerate() {
                    let visible = match c.guard {
                        Some(g) => truthy(&self.eval(host, g)?),
                        None => true,
                    };
                    if visible {
                        items.push(MenuItem { index, text: self.text(host, &c.text)? });
                    }
                }
                if items.is_empty() {
                    let loc = self.position_loc();
                    return Err(RuntimeError {
                        loc,
                        message: "every choice in this menu is hidden by its condition".into(),
                    });
                }
                Event::Menu { caption, items }
            }
            _ => unreachable!("not a blocking op"),
        })
    }

    fn position_loc(&self) -> Option<Loc> {
        let f = self.frames.last()?;
        let code = self.program.labels.get(&f.label)?;
        let stmt = *code.op_stmt.get(f.pc as usize)?;
        code.stmts.get(stmt as usize).map(|s| s.loc.clone())
    }

    fn checkpoint(&self, host: &mut dyn Host) -> Result<Checkpoint, RuntimeError> {
        let vars = host.snapshot().map_err(|e| RuntimeError { loc: self.position_loc(), message: e })?;
        Ok(Checkpoint { frames: self.frames.clone(), stage: self.stage.clone(), vars })
    }

    fn record(&mut self, cp: Checkpoint) {
        // Replaying exactly what was rolled back keeps the redo list; anything else drops it.
        if self.redo.last() == Some(&cp) {
            self.redo.pop();
        } else {
            self.redo.clear();
        }
        self.history.push_back(cp);
        while self.history.len() > self.max_history.max(1) {
            self.history.pop_front();
        }
    }

    /// Runs until the next event. After a blocking event, call again to continue (or
    /// [`Vm::choose`] for menus).
    pub fn next(&mut self, host: &mut dyn Host) -> Result<Event, RuntimeError> {
        match self.state {
            State::NotStarted => return Err(err("call `start` before `next`")),
            State::Ended => return Ok(Event::End),
            State::Choosing => return Err(err("waiting for a menu choice; call `choose`")),
            State::Restored => {
                let ev = self.blocking_event(host)?;
                self.state = if matches!(ev, Event::Menu { .. }) { State::Choosing } else { State::Waiting };
                return Ok(ev);
            }
            State::Waiting => {
                self.frames.last_mut().unwrap().pc += 1;
                self.state = State::Running;
            }
            State::Running => {}
        }

        loop {
            let op = self.op()?.clone();
            if op.blocks() {
                let ev = self.blocking_event(host)?;
                let cp = self.checkpoint(host)?;
                self.record(cp);
                self.state = if matches!(ev, Event::Menu { .. }) { State::Choosing } else { State::Waiting };
                return Ok(ev);
            }
            let frame = self.frames.last_mut().unwrap();
            frame.pc += 1;
            match op {
                Op::Exec(id) => {
                    let chunk = self.program.chunk(id).clone();
                    host.exec(&chunk).map_err(|e| self.host_err(id, e))?;
                }
                Op::JumpIfNot { cond, target } => {
                    if !truthy(&self.eval(host, cond)?) {
                        self.frames.last_mut().unwrap().pc = target;
                    }
                }
                Op::Goto(target) => frame.pc = target,
                Op::Jump(label) => *frame = Frame { label, pc: 0 },
                Op::Call(label) => self.frames.push(Frame { label, pc: 0 }),
                Op::Return => {
                    self.frames.pop();
                    if self.frames.is_empty() {
                        self.state = State::Ended;
                        return Ok(Event::End);
                    }
                }
                Op::Scene { image, with } => {
                    self.stage.scene = Some(image.clone()).filter(|i| !i.is_empty());
                    self.stage.shown.clear();
                    return Ok(Event::Scene { image, with });
                }
                Op::Show { image, at, with } => {
                    let shown = Shown { image: image.clone(), at: at.clone() };
                    match self.stage.shown.iter_mut().find(|s| s.image[0] == image[0]) {
                        Some(existing) => *existing = shown,
                        None => self.stage.shown.push(shown),
                    }
                    return Ok(Event::Show { image, at, with });
                }
                Op::Hide { tag, with } => {
                    self.stage.shown.retain(|s| s.image[0] != tag);
                    return Ok(Event::Hide { tag, with });
                }
                Op::With(t) => return Ok(Event::With(t)),
                Op::Play { channel, file, fade, looping } => {
                    let looping = looping.unwrap_or(channel == "music");
                    if looping {
                        self.stage.tracks.insert(channel.clone(), Track { file: file.clone(), looping });
                    } else {
                        self.stage.tracks.remove(&channel);
                    }
                    return Ok(Event::Play { channel, file, fade, looping });
                }
                Op::Stop { channel, fade } => {
                    self.stage.tracks.remove(&channel);
                    return Ok(Event::Stop { channel, fade });
                }
                Op::Say(_) | Op::Menu { .. } | Op::Pause(_) => unreachable!("handled above"),
            }
        }
    }

    /// Picks menu item `index` (a [`MenuItem::index`]).
    pub fn choose(&mut self, host: &mut dyn Host, index: usize) -> Result<(), RuntimeError> {
        if self.state != State::Choosing {
            return Err(err("there is no menu to choose from"));
        }
        let Op::Menu { choices, .. } = self.op()?.clone() else { unreachable!("choosing at a menu") };
        let choice = choices.get(index).ok_or_else(|| err(format!("menu has no choice {index}")))?;
        if let Some(g) = choice.guard
            && !truthy(&self.eval(host, g)?)
        {
            return Err(err(format!("choice {index} is hidden by its condition")));
        }
        self.frames.last_mut().unwrap().pc = choice.target;
        self.state = State::Running;
        Ok(())
    }

    fn restore(&mut self, host: &mut dyn Host, cp: &Checkpoint) -> Result<(), RuntimeError> {
        host.restore(&cp.vars).map_err(|e| err(format!("restoring story variables: {e}")))?;
        self.frames = cp.frames.clone();
        self.stage = cp.stage.clone();
        self.state = State::Restored;
        Ok(())
    }

    /// Steps back to the previous line or menu. Returns false at the start of history.
    /// The runtime should rebuild visuals from [`Vm::stage`], then call [`Vm::next`].
    pub fn rollback(&mut self, host: &mut dyn Host) -> Result<bool, RuntimeError> {
        if self.history.len() < 2 || !matches!(self.state, State::Waiting | State::Choosing | State::Restored)
        {
            return Ok(false);
        }
        let current = self.history.pop_back().unwrap();
        self.redo.push(current);
        let previous = self.history.back().unwrap().clone();
        self.restore(host, &previous)?;
        Ok(true)
    }

    /// Undoes a rollback. Returns false when there is nothing to redo.
    pub fn roll_forward(&mut self, host: &mut dyn Host) -> Result<bool, RuntimeError> {
        let Some(cp) = self.redo.pop() else { return Ok(false) };
        self.restore(host, &cp)?;
        self.history.push_back(cp);
        Ok(true)
    }

    pub fn can_roll_forward(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Whether [`Vm::rollback`] would move.
    pub fn can_rollback(&self) -> bool {
        self.history.len() >= 2 && matches!(self.state, State::Waiting | State::Choosing | State::Restored)
    }

    /// Forgets the history, so the player cannot roll back past this point (e.g. after a
    /// minigame result). The current line stays.
    pub fn block_rollback(&mut self) {
        let keep = matches!(self.state, State::Waiting | State::Choosing | State::Restored);
        let last = if keep { self.history.pop_back() } else { None };
        self.history.clear();
        self.history.extend(last);
        self.redo.clear();
    }

    /// Identifies the line or menu the story is at, the same way across sessions and script
    /// edits that keep its text (label plus the statement hash). For "seen" tracking.
    pub fn line_key(&self) -> Option<u64> {
        let f = self.frames.last()?;
        let code = self.program.labels.get(&f.label)?;
        let stmt = *code.op_stmt.get(f.pc as usize)?;
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in f.label.bytes().chain(code.stmts.get(stmt as usize)?.hash.to_le_bytes()) {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        Some(h)
    }

    /// The current position, stage and variables as plain data. Only valid while waiting
    /// for the player (where saves happen).
    pub fn save(&self) -> Result<Value, RuntimeError> {
        let cp = self
            .history
            .back()
            .filter(|_| matches!(self.state, State::Waiting | State::Choosing | State::Restored))
            .ok_or_else(|| err("saving is only possible while the story waits for the player"))?;
        let top = cp.frames.len() - 1;
        let frames = cp
            .frames
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let code = &self.program.labels[&f.label];
                // Lower frames point just past their `call`.
                let after = i != top;
                let op = if after { f.pc - 1 } else { f.pc };
                let stmt = code.op_stmt[op as usize];
                Value::map()
                    .with("label", f.label.as_str())
                    .with("stmt", stmt)
                    .with("hash", format!("{:016x}", code.stmts[stmt as usize].hash))
                    .with("after", after)
            })
            .collect::<Vec<_>>();
        Ok(Value::map()
            .with("frames", Value::List(frames))
            .with("stage", cp.stage.to_value())
            .with("vars", cp.vars.clone()))
    }

    /// Restores a [`Vm::save`] made with this or an earlier version of the script. Returns
    /// warnings when positions had to move because the script changed.
    pub fn load(&mut self, host: &mut dyn Host, save: &Value) -> Result<Vec<String>, RuntimeError> {
        let mut warnings = Vec::new();
        let mut frames = Vec::new();
        let saved = save.get("frames").as_list();
        if saved.is_empty() {
            return Err(err("the save has no story position"));
        }
        for (i, f) in saved.iter().enumerate() {
            let label = f.get("label").as_str().unwrap_or_default().to_string();
            let Some(code) = self.program.labels.get(&label) else {
                return Err(err(format!("the save is in label `{label}`, which no longer exists")));
            };
            let stmt = f.get("stmt").as_int().unwrap_or(0).max(0) as usize;
            let hash = f.get("hash").as_str().and_then(|h| u64::from_str_radix(h, 16).ok());
            let after = f.get("after").as_bool().unwrap_or(i + 1 < saved.len());
            let found = match hash {
                Some(h) if code.stmts.get(stmt).is_some_and(|s| s.hash == h) => Some(stmt),
                Some(h) => code
                    .stmts
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.hash == h)
                    .map(|(j, _)| j)
                    .min_by_key(|j| j.abs_diff(stmt)),
                None => None,
            };
            let pc = match found {
                Some(s) => {
                    if s != stmt {
                        warnings
                            .push(format!("`{label}`: the saved line moved (statement {stmt} is now {s})"));
                    }
                    code.stmts[s].first_op + after as u32
                }
                None => {
                    warnings.push(format!(
                        "`{label}`: the saved line was changed or removed; resuming at the start of the label"
                    ));
                    0
                }
            };
            frames.push(Frame { label, pc });
        }

        host.restore(save.get("vars")).map_err(|e| err(format!("restoring story variables: {e}")))?;
        // Defaults added in a game update fill in for old saves.
        self.apply_defaults(host)?;
        self.frames = frames;
        self.stage = Stage::from_value(save.get("stage"));
        self.history.clear();
        self.redo.clear();
        // Positions that no longer sit on a blocking op run forward to the next one.
        self.state = if self.op()?.blocks() { State::Restored } else { State::Running };
        if self.state == State::Restored {
            let cp = self.checkpoint(host)?;
            self.history.push_back(cp);
        }
        Ok(warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile;

    /// A tiny stand-in for Luau: integers, strings, booleans, `x = n`, `x += n`,
    /// `x > n`/`x < n`/`x == n`, and `Character("Name")`.
    #[derive(Default)]
    struct Fake {
        consts: BTreeMap<String, Value>,
        vars: BTreeMap<String, Value>,
    }

    impl Fake {
        fn value(&self, src: &str) -> Value {
            let src = src.trim();
            if let Some(name) = src.strip_prefix("Character(\"").and_then(|s| s.strip_suffix("\")")) {
                return Value::map().with("name", name);
            }
            if let Ok(i) = src.parse::<i64>() {
                return Value::Int(i);
            }
            if let Some(s) = src.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                return s.into();
            }
            match src {
                "true" => true.into(),
                "false" => false.into(),
                _ => self.vars.get(src).or(self.consts.get(src)).cloned().unwrap_or_default(),
            }
        }
    }

    impl Host for Fake {
        fn define(&mut self, name: &str, chunk: &Chunk) -> Result<(), String> {
            let v = self.value(&chunk.source);
            self.consts.insert(name.into(), v);
            Ok(())
        }
        fn set_default(&mut self, name: &str, chunk: &Chunk) -> Result<(), String> {
            if !self.vars.contains_key(name) {
                let v = self.value(&chunk.source);
                self.vars.insert(name.into(), v);
            }
            Ok(())
        }
        fn eval(&mut self, chunk: &Chunk) -> Result<Value, String> {
            let s = &chunk.source;
            for (op, f) in [
                (" > ", (|a, b| a > b) as fn(i64, i64) -> bool),
                (" < ", |a, b| a < b),
                (" == ", |a, b| a == b),
            ] {
                if let Some((a, b)) = s.split_once(op) {
                    let (a, b) = (self.value(a).as_int().unwrap_or(0), self.value(b).as_int().unwrap_or(0));
                    return Ok(f(a, b).into());
                }
            }
            if s == "boom" {
                return Err("attempt to call a nil value".into());
            }
            Ok(self.value(s))
        }
        fn exec(&mut self, chunk: &Chunk) -> Result<(), String> {
            let s = chunk.source.as_str();
            if let Some((name, n)) = s.split_once("+=") {
                let cur = self.value(name).as_int().unwrap_or(0);
                self.vars.insert(name.trim().into(), Value::Int(cur + self.value(n).as_int().unwrap_or(0)));
            } else if let Some((name, v)) = s.split_once('=') {
                let v = self.value(v);
                self.vars.insert(name.trim().into(), v);
            }
            Ok(())
        }
        fn snapshot(&mut self) -> Result<Value, String> {
            Ok(Value::Map(self.vars.clone()))
        }
        fn restore(&mut self, vars: &Value) -> Result<(), String> {
            self.vars = match vars {
                Value::Map(m) => m.clone(),
                _ => BTreeMap::new(),
            };
            Ok(())
        }
    }

    fn vm(src: &str) -> (Vm, Fake) {
        let program = compile(&[("t.kulisa", src)]).unwrap_or_else(|e| panic!("{e}"));
        let (mut vm, mut host) = (Vm::new(Arc::new(program)), Fake::default());
        vm.start(&mut host, "start").unwrap();
        (vm, host)
    }

    /// Text of the next blocking event, skipping stage events.
    fn next_line(vm: &mut Vm, host: &mut Fake) -> String {
        loop {
            match vm.next(host).unwrap() {
                Event::Say(l) => return l.text,
                Event::Menu { items, .. } => {
                    return format!(
                        "menu:{}",
                        items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>().join("|")
                    );
                }
                Event::Pause(_) => return "pause".into(),
                Event::End => return "end".into(),
                _ => {}
            }
        }
    }

    const STORY: &str = r#"define m = Character("Mila")
default trust = 0

label start:
    scene cafe
    show mila smile at left
    m "One."
    $ trust += 1
    m "Two, trust [trust]."
    menu:
        "Choose:"
        "Kind":
            $ trust += 1
        "Secret" if trust > 5:
            "never"
        "Leave":
            jump leave
    if trust > 1:
        "Warm."
    else:
        "Cold."
    call aside
    "Back."
    return

label aside:
    "Aside."

label leave:
    play music "rain.ogg"
    "Gone."
"#;

    #[test]
    fn runs_lines_menus_ifs_and_calls() {
        let (mut vm, mut h) = vm(STORY);
        assert_eq!(vm.next(&mut h).unwrap(), Event::Scene { image: vec!["cafe".into()], with: None });
        assert!(matches!(vm.next(&mut h).unwrap(), Event::Show { .. }));
        let Event::Say(l) = vm.next(&mut h).unwrap() else { panic!() };
        assert_eq!(l.text, "One.");
        assert_eq!(l.who.unwrap().data.get("name").as_str(), Some("Mila"));
        assert_eq!(next_line(&mut vm, &mut h), "Two, trust 1.");
        assert_eq!(next_line(&mut vm, &mut h), "menu:Kind|Leave", "guarded choice is hidden");
        assert!(vm.next(&mut h).is_err(), "must choose first");
        vm.choose(&mut h, 0).unwrap();
        assert_eq!(next_line(&mut vm, &mut h), "Warm.");
        assert_eq!(next_line(&mut vm, &mut h), "Aside.");
        assert_eq!(next_line(&mut vm, &mut h), "Back.");
        assert_eq!(next_line(&mut vm, &mut h), "end");
        assert!(vm.is_ended());
        assert_eq!(
            vm.stage().shown,
            vec![Shown { image: vec!["mila".into(), "smile".into()], at: Some("left".into()) }]
        );
    }

    #[test]
    fn rolls_back_and_forward() {
        let (mut vm, mut h) = vm(STORY);
        assert_eq!(next_line(&mut vm, &mut h), "One.");
        assert_eq!(next_line(&mut vm, &mut h), "Two, trust 1.");
        assert!(vm.rollback(&mut h).unwrap());
        assert_eq!(h.vars["trust"], Value::Int(0), "variables roll back");
        assert_eq!(next_line(&mut vm, &mut h), "One.");
        assert!(!vm.rollback(&mut h).unwrap(), "nothing before the first line");
        assert!(vm.roll_forward(&mut h).unwrap());
        assert_eq!(next_line(&mut vm, &mut h), "Two, trust 1.");
        assert_eq!(h.vars["trust"], Value::Int(1));

        // Rolling back over a menu and choosing differently takes the other path.
        assert_eq!(next_line(&mut vm, &mut h), "menu:Kind|Leave");
        vm.choose(&mut h, 0).unwrap();
        assert_eq!(next_line(&mut vm, &mut h), "Warm.");
        assert!(vm.rollback(&mut h).unwrap());
        assert_eq!(next_line(&mut vm, &mut h), "menu:Kind|Leave");
        vm.choose(&mut h, 2).unwrap();
        assert_eq!(next_line(&mut vm, &mut h), "Gone.");
        assert!(!vm.can_roll_forward(), "a different path drops the redo list");
        assert_eq!(vm.stage().tracks["music"].file, "rain.ogg");
    }

    #[test]
    fn replaying_the_same_path_keeps_redo() {
        let (mut vm, mut h) = vm(STORY);
        next_line(&mut vm, &mut h);
        next_line(&mut vm, &mut h);
        next_line(&mut vm, &mut h); // menu
        vm.rollback(&mut h).unwrap();
        vm.rollback(&mut h).unwrap();
        assert_eq!(next_line(&mut vm, &mut h), "One.");
        assert_eq!(next_line(&mut vm, &mut h), "Two, trust 1.");
        assert!(vm.can_roll_forward(), "still on the recorded path");
    }

    #[test]
    fn line_keys_and_blocked_rollback() {
        let (mut vm, mut h) = vm(STORY);
        assert_eq!(next_line(&mut vm, &mut h), "One.");
        let one = vm.line_key().unwrap();
        assert!(!vm.can_rollback());
        assert_eq!(next_line(&mut vm, &mut h), "Two, trust 1.");
        assert_ne!(vm.line_key(), Some(one));
        assert!(vm.can_rollback());
        vm.rollback(&mut h).unwrap();
        assert_eq!(vm.line_key(), Some(one), "the same line has the same key");
        assert_eq!(next_line(&mut vm, &mut h), "One.");
        assert_eq!(next_line(&mut vm, &mut h), "Two, trust 1.");
        assert_eq!(next_line(&mut vm, &mut h), "menu:Kind|Leave");
        vm.block_rollback();
        assert!(!vm.can_rollback() && !vm.rollback(&mut h).unwrap());
        vm.choose(&mut h, 0).unwrap();
        assert_eq!(next_line(&mut vm, &mut h), "Warm.");
        assert!(vm.rollback(&mut h).unwrap(), "back to the menu, but no further");
        assert!(!vm.can_rollback());
        // Keys survive recompiling an edited script.
        let edited = STORY.replace(
            "    scene cafe
",
            "    scene cafe
    \"New line.\"
",
        );
        let (mut vm2, mut h2) = super::tests::vm(&edited);
        assert_eq!(next_line(&mut vm2, &mut h2), "New line.");
        assert_eq!(next_line(&mut vm2, &mut h2), "One.");
        assert_eq!(vm2.line_key(), Some(one));
    }

    #[test]
    fn history_is_bounded() {
        let src = "label start:\n    \"a\"\n    \"b\"\n    \"c\"\n    \"d\"\n";
        let (mut vm, mut h) = vm(src);
        vm.max_history = 2;
        for _ in 0..4 {
            next_line(&mut vm, &mut h);
        }
        assert!(vm.rollback(&mut h).unwrap());
        assert!(!vm.rollback(&mut h).unwrap());
    }

    #[test]
    fn saves_and_loads_mid_call() {
        let (mut vm1, mut h1) = vm(STORY);
        for _ in 0..3 {
            next_line(&mut vm1, &mut h1);
        }
        vm1.choose(&mut h1, 0).unwrap();
        next_line(&mut vm1, &mut h1); // Warm.
        assert_eq!(next_line(&mut vm1, &mut h1), "Aside.");
        let save = vm1.save().unwrap();
        let bytes = save.to_bytes();

        let (mut vm2, mut h2) = vm(STORY);
        let save = Value::decode(&mut bytes.as_slice()).unwrap();
        assert!(vm2.load(&mut h2, &save).unwrap().is_empty());
        assert_eq!(h2.vars["trust"], Value::Int(2));
        assert_eq!(vm2.stage().scene, Some(vec!["cafe".into()]));
        assert_eq!(next_line(&mut vm2, &mut h2), "Aside.", "shows the saved line again");
        assert_eq!(next_line(&mut vm2, &mut h2), "Back.", "returns into the caller");
    }

    #[test]
    fn saves_survive_script_edits() {
        let v1 = "default a = 1\nlabel start:\n    \"One.\"\n    \"Two.\"\n    \"Three.\"\n";
        let (mut vm1, mut h1) = vm(v1);
        next_line(&mut vm1, &mut h1);
        assert_eq!(next_line(&mut vm1, &mut h1), "Two.");
        let save = vm1.save().unwrap();

        // Lines inserted above: the save follows its line. A new default fills in.
        let v2 = "default a = 1\ndefault b = 7\nlabel start:\n    \"Intro.\"\n    \"One.\"\n    \"Two.\"\n    \"Three.\"\n";
        let (mut vm2, mut h2) = vm(v2);
        let warnings = vm2.load(&mut h2, &save).unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(next_line(&mut vm2, &mut h2), "Two.");
        assert_eq!(h2.vars["b"], Value::Int(7));

        // The saved line itself was rewritten: resume at the label start.
        let v3 = "label start:\n    \"One.\"\n    \"Two, edited.\"\n";
        let (mut vm3, mut h3) = vm(v3);
        let warnings = vm3.load(&mut h3, &save).unwrap();
        assert!(warnings[0].contains("start of the label"), "{warnings:?}");
        assert_eq!(next_line(&mut vm3, &mut h3), "One.");

        let v4 = "label other:\n    \"x\"\nlabel start:\n    \"y\"\n";
        let mut bad = save.clone();
        bad = bad.with("frames", Value::List(vec![Value::map().with("label", "gone")]));
        let (mut vm4, mut h4) = vm(v4);
        assert!(vm4.load(&mut h4, &bad).unwrap_err().message.contains("no longer exists"));
    }

    #[test]
    fn host_errors_point_into_the_script() {
        let (mut vm, mut h) = vm("label start:\n    \"Hi [boom]\"\n");
        let e = vm.next(&mut h).unwrap_err();
        assert_eq!(e.to_string(), "t.kulisa:2:5: error: attempt to call a nil value");
    }

    #[test]
    fn stage_tracks_scene_shows_and_music() {
        let src = "label start:\n    show a x\n    show b at right\n    show a y at left\n    hide b\n    play music \"m.ogg\"\n    play sound \"click.wav\"\n    \"!\"\n    scene bg\n    stop music\n    \"done\"\n";
        let (mut vm, mut h) = vm(src);
        assert_eq!(next_line(&mut vm, &mut h), "!");
        let st = vm.stage().clone();
        assert_eq!(st.shown, vec![Shown { image: vec!["a".into(), "y".into()], at: Some("left".into()) }]);
        assert_eq!(st.tracks.len(), 1, "one-shot sounds are not stage state");
        assert_eq!(Stage::from_value(&st.to_value()), st);
        assert_eq!(next_line(&mut vm, &mut h), "done");
        assert!(vm.stage().shown.is_empty() && vm.stage().tracks.is_empty());
    }
}
