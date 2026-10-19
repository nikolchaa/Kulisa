//! The story host: runs the Luau parts of `.kulisa` scripts.
//!
//! Two layers: story variables (saved) and constants (not saved). `$` lines and lua:
//! blocks inside labels run with the variables table as their environment, so their
//! assignments are story state. `define`s and top-level `lua:` blocks (helpers, setup)
//! write constants. Lookups chain variables -> constants -> globals.

use kulisa_core::Value;
use kulisa_story::{Chunk, ChunkKind, Diagnostic, Diagnostics, Host, Loc, Program};
use mlua::{Function, Lua, Table, Value as LuaValue};

pub struct StoryHost {
    lua: Lua,
    vars: Table,
    consts: Table,
    compiled: Vec<Option<Function>>,
    /// Registered script state: name -> { save = fn, load = fn }.
    registry: Table,
}

const MAX_DEPTH: usize = 64;

/// Converts a Luau value to plain data. `strict` refuses functions and userdata (saves);
/// otherwise they become a placeholder string (expression results).
pub(crate) fn to_value(v: &LuaValue, path: &str, strict: bool, depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return Err(format!("`{path}` is nested too deeply (or refers to itself)"));
    }
    Ok(match v {
        LuaValue::Nil => Value::Nil,
        LuaValue::Boolean(b) => Value::Bool(*b),
        // mlua's Luau integer is i32 or i64 depending on configuration.
        #[allow(clippy::unnecessary_cast)]
        LuaValue::Integer(i) => Value::Int(*i as i64),
        LuaValue::Number(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => Value::Int(*n as i64),
        LuaValue::Number(n) => Value::Float(*n),
        LuaValue::String(s) => Value::Str(s.to_string_lossy()),
        LuaValue::Table(t) => {
            let len = t.raw_len();
            let pairs: Vec<(LuaValue, LuaValue)> =
                t.pairs().collect::<mlua::Result<_>>().map_err(|e| e.to_string())?;
            if len > 0 && pairs.len() == len {
                let mut list = Vec::with_capacity(len);
                for i in 1..=len {
                    let item: LuaValue = t.raw_get(i).map_err(|e| e.to_string())?;
                    list.push(to_value(&item, &format!("{path}[{i}]"), strict, depth + 1)?);
                }
                Value::List(list)
            } else {
                let mut map = std::collections::BTreeMap::new();
                for (k, item) in pairs {
                    let key = match &k {
                        LuaValue::String(s) => s.to_string_lossy(),
                        LuaValue::Integer(i) => i.to_string(),
                        LuaValue::Number(n) => n.to_string(),
                        _ if strict => {
                            return Err(format!(
                                "`{path}` has a {} key; only string and number keys can be saved",
                                k.type_name()
                            ));
                        }
                        _ => continue,
                    };
                    let item = to_value(&item, &format!("{path}.{key}"), strict, depth + 1)?;
                    map.insert(key, item);
                }
                Value::Map(map)
            }
        }
        other if strict => {
            return Err(format!(
                "story variable `{path}` holds a {}; only plain data (nil, booleans, numbers, strings, tables) can be saved",
                other.type_name()
            ));
        }
        other => Value::Str(format!("[{}]", other.type_name())),
    })
}

pub(crate) fn from_value(lua: &Lua, v: &Value) -> mlua::Result<LuaValue> {
    Ok(match v {
        Value::Nil => LuaValue::Nil,
        Value::Bool(b) => LuaValue::Boolean(*b),
        Value::Int(i) => LuaValue::Number(*i as f64),
        Value::Float(f) => LuaValue::Number(*f),
        Value::Str(s) => LuaValue::String(lua.create_string(s)?),
        Value::Bytes(b) => LuaValue::String(lua.create_string(b)?),
        Value::List(l) => {
            let t = lua.create_table_with_capacity(l.len(), 0)?;
            for (i, item) in l.iter().enumerate() {
                t.raw_set(i + 1, from_value(lua, item)?)?;
            }
            LuaValue::Table(t)
        }
        Value::Map(m) => {
            let t = lua.create_table_with_capacity(0, m.len())?;
            for (k, item) in m {
                t.raw_set(k.as_str(), from_value(lua, item)?)?;
            }
            LuaValue::Table(t)
        }
    })
}

/// Rewrites Luau's `file:N:` prefix (N counted within the chunk) into a story location.
fn locate(message: &str, chunk: &Chunk) -> (Loc, String) {
    let first = message.lines().next().unwrap_or(message);
    let prefix = format!("{}:", chunk.loc.file);
    if let Some(rest) = first.strip_prefix(&prefix)
        && let Some((n, msg)) = rest.split_once(':')
        && let Ok(n) = n.parse::<usize>()
    {
        let line = chunk.loc.line + n.saturating_sub(1);
        let col = if n <= 1 { chunk.loc.col } else { 1 };
        return (Loc { line, col, ..chunk.loc.clone() }, msg.trim().to_string());
    }
    (chunk.loc.clone(), first.trim().to_string())
}

fn message(e: &mlua::Error, chunk: &Chunk) -> String {
    let text = match e {
        mlua::Error::CallbackError { cause, .. } => cause.to_string(),
        mlua::Error::RuntimeError(m) => m.clone(),
        e => e.to_string(),
    };
    locate(&text, chunk).1
}

impl StoryHost {
    /// A host on `lua`. Adds `Character(name, options)` for `define` lines.
    pub fn new(lua: &Lua) -> mlua::Result<Self> {
        let vars = lua.create_table()?;
        let consts = lua.create_table()?;
        let inherit = |parent: Table| -> mlua::Result<Table> {
            let meta = lua.create_table()?;
            meta.raw_set("__index", parent)?;
            Ok(meta)
        };
        consts.set_metatable(Some(inherit(lua.globals())?))?;
        vars.set_metatable(Some(inherit(consts.clone())?))?;
        consts.raw_set(
            "Character",
            lua.create_function(|lua, (name, opts): (String, Option<Table>)| {
                let t = lua.create_table()?;
                if let Some(opts) = opts {
                    for pair in opts.pairs::<LuaValue, LuaValue>() {
                        let (k, v) = pair?;
                        t.raw_set(k, v)?;
                    }
                }
                t.raw_set("name", name)?;
                Ok(t)
            })?,
        )?;
        Ok(Self { lua: lua.clone(), vars, consts, compiled: Vec::new(), registry: lua.create_table()? })
    }

    /// Compiles every chunk of `program`; Luau syntax errors come back as story diagnostics.
    pub fn prepare(&mut self, program: &Program) -> Result<(), Diagnostics> {
        let mut errors = Vec::new();
        let init: std::collections::HashSet<_> = program.init.iter().copied().collect();
        self.compiled = program
            .chunks
            .iter()
            .map(|chunk| {
                let src = match chunk.kind {
                    ChunkKind::Expr => format!("return ({})", chunk.source),
                    ChunkKind::Stmt => chunk.source.clone(),
                };
                let compiled =
                    self.lua.load(&src).set_name(format!("@{}", chunk.loc.file)).into_function().and_then(
                        |f| {
                            let env = if init.contains(&chunk.id) { &self.consts } else { &self.vars };
                            f.set_environment(env.clone()).map(|_| f)
                        },
                    );
                match compiled {
                    Ok(f) => Some(f),
                    Err(e) => {
                        let text = match &e {
                            mlua::Error::SyntaxError { message, .. } => message.clone(),
                            e => e.to_string(),
                        };
                        let (loc, msg) = locate(&text, chunk);
                        let line_no = loc.line - chunk.loc.line;
                        let line_text =
                            chunk.source.lines().nth(line_no).unwrap_or(&chunk.source).to_string();
                        errors.push(Diagnostic::new(loc, format!("Luau: {msg}"), &line_text));
                        None
                    }
                }
            })
            .collect();
        if errors.is_empty() { Ok(()) } else { Err(Diagnostics(errors)) }
    }

    fn function(&self, chunk: &Chunk) -> Result<&Function, String> {
        self.compiled
            .get(chunk.id as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| "chunk was not compiled; call `prepare` with this program first".to_string())
    }

    fn call(&self, chunk: &Chunk) -> Result<LuaValue, String> {
        self.function(chunk)?.call::<LuaValue>(()).map_err(|e| message(&e, chunk))
    }

    /// A story variable, for tests and debugging.
    pub fn var(&self, name: &str) -> Value {
        let v: LuaValue = self.vars.raw_get(name).unwrap_or(LuaValue::Nil);
        to_value(&v, name, false, 0).unwrap_or_default()
    }

    /// The story variables table (what `$` lines read and write).
    pub fn vars(&self) -> &Table {
        &self.vars
    }

    /// Makes `value` visible to story chunks as a constant (helpers, data).
    pub fn define_value(&self, name: &str, value: LuaValue) -> mlua::Result<()> {
        self.consts.raw_set(name, value)
    }

    /// Registers script state that takes part in checkpoints and saves: `save()` returns
    /// plain data, `load(data)` restores it.
    pub fn register_state(&self, name: &str, save: Function, load: Function) -> mlua::Result<()> {
        let entry = self.lua.create_table()?;
        entry.raw_set("save", save)?;
        entry.raw_set("load", load)?;
        self.registry.raw_set(name, entry)
    }
}

impl Host for StoryHost {
    fn define(&mut self, name: &str, chunk: &Chunk) -> Result<(), String> {
        let v = self.call(chunk)?;
        self.consts.raw_set(name, v).map_err(|e| e.to_string())
    }

    fn set_default(&mut self, name: &str, chunk: &Chunk) -> Result<(), String> {
        let current: LuaValue = self.vars.raw_get(name).map_err(|e| e.to_string())?;
        if current.is_nil() {
            let v = self.call(chunk)?;
            self.vars.raw_set(name, v).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn eval(&mut self, chunk: &Chunk) -> Result<Value, String> {
        let v = self.call(chunk)?;
        to_value(&v, &chunk.source, false, 0)
    }

    fn exec(&mut self, chunk: &Chunk) -> Result<(), String> {
        self.call(chunk).map(|_| ())
    }

    /// `{ vars = <story variables>, state = { <registered name> = <its save()> } }`.
    fn snapshot(&mut self) -> Result<Value, String> {
        let vars = to_value(&LuaValue::Table(self.vars.clone()), "story", true, 0)?;
        let vars = if let Value::List(_) = vars { Value::map() } else { vars };
        let mut state = std::collections::BTreeMap::new();
        for pair in self.registry.pairs::<String, Table>() {
            let (name, entry) = pair.map_err(|e| e.to_string())?;
            let save: Function = entry.raw_get("save").map_err(|e| e.to_string())?;
            let data: LuaValue = save.call(()).map_err(|e| format!("state `{name}`: save failed: {e}"))?;
            state.insert(name.clone(), to_value(&data, &name, true, 0)?);
        }
        Ok(Value::map().with("vars", vars).with("state", Value::Map(state)))
    }

    fn restore(&mut self, snapshot: &Value) -> Result<(), String> {
        self.vars.clear().map_err(|e| e.to_string())?;
        if let Value::Map(m) = snapshot.get("vars") {
            for (k, v) in m {
                let v = from_value(&self.lua, v).map_err(|e| e.to_string())?;
                self.vars.raw_set(k.as_str(), v).map_err(|e| e.to_string())?;
            }
        }
        for pair in self.registry.pairs::<String, Table>() {
            let (name, entry) = pair.map_err(|e| e.to_string())?;
            let data = snapshot.get("state").get(&name);
            let load: Function = entry.raw_get("load").map_err(|e| e.to_string())?;
            let data = from_value(&self.lua, data).map_err(|e| e.to_string())?;
            load.call::<()>(data).map_err(|e| format!("state `{name}`: load failed: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use kulisa_story::{Event, Vm, compile};

    fn setup(src: &str) -> (Vm, StoryHost) {
        let program = compile(&[("t.kulisa", src)]).unwrap_or_else(|e| panic!("{e}"));
        let lua = Lua::new();
        let mut host = StoryHost::new(&lua).unwrap();
        host.prepare(&program).unwrap_or_else(|e| panic!("{e}"));
        let mut vm = Vm::new(Arc::new(program));
        vm.start(&mut host, "start").unwrap();
        (vm, host)
    }

    fn say(vm: &mut Vm, h: &mut StoryHost) -> String {
        loop {
            match vm.next(h).unwrap() {
                Event::Say(l) => return l.text,
                Event::End => return "end".into(),
                _ => {}
            }
        }
    }

    const STORY: &str = r##"define mila = Character("Mila", { color = "#e8a0bf" })
default trust = 0
default bag = { "key", "map" }

lua:
    function greet(n) return "hi " .. n end

label start:
    $ trust += 2
    if trust > 1 and #bag == 2:
        mila "[greet(mila.name)], trust [trust], [bag[2]]."
    "Done."
"##;

    #[test]
    fn runs_luau_in_stories() {
        let (mut vm, mut h) = setup(STORY);
        let Event::Say(line) = vm.next(&mut h).unwrap() else { panic!() };
        assert_eq!(line.text, "hi Mila, trust 2, map.");
        let who = line.who.unwrap();
        assert_eq!(who.data.get("color").as_str(), Some("#e8a0bf"));
        assert_eq!(h.var("trust"), Value::Int(2));
        assert_eq!(say(&mut vm, &mut h), "Done.");
    }

    #[test]
    fn snapshots_only_story_variables() {
        let (mut vm, mut h) = setup(STORY);
        vm.next(&mut h).unwrap();
        let snap = h.snapshot().unwrap();
        let vars = snap.get("vars");
        assert_eq!(vars.get("trust"), &Value::Int(2));
        assert_eq!(vars.get("bag"), &Value::List(vec!["key".into(), "map".into()]));
        assert_eq!(vars.get("mila"), &Value::Nil, "defines are constants, not saved");
        assert_eq!(vars.get("greet"), &Value::Nil, "top-level lua: helpers are constants, not saved");
        h.restore(&Value::map().with("vars", Value::map().with("trust", 9i64))).unwrap();
        assert_eq!(h.var("trust"), Value::Int(9));
        assert_eq!(h.var("bag"), Value::Nil);
    }

    #[test]
    fn rollback_restores_luau_state() {
        let (mut vm, mut h) = setup("default n = 0\nlabel start:\n    \"a\"\n    $ n += 1\n    \"b [n]\"\n");
        assert_eq!(say(&mut vm, &mut h), "a");
        assert_eq!(say(&mut vm, &mut h), "b 1");
        vm.rollback(&mut h).unwrap();
        assert_eq!(h.var("n"), Value::Int(0));
        assert_eq!(say(&mut vm, &mut h), "a");
        assert_eq!(say(&mut vm, &mut h), "b 1");
    }

    #[test]
    fn registered_state_rolls_back_with_the_story() {
        let (mut vm, mut h) = setup(
            "label start:
    \"a\"
    \"b\"
",
        );
        let lua = h.lua.clone();
        lua.load("inventory = { coins = 1 }").exec().unwrap();
        let save: Function =
            lua.load("return function() return { coins = inventory.coins } end").eval().unwrap();
        let load: Function =
            lua.load("return function(t) inventory.coins = t and t.coins or 0 end").eval().unwrap();
        h.register_state("inventory", save, load).unwrap();
        assert_eq!(say(&mut vm, &mut h), "a");
        lua.load("inventory.coins = 50").exec().unwrap();
        assert_eq!(say(&mut vm, &mut h), "b");
        vm.rollback(&mut h).unwrap();
        let coins: i64 = lua.load("return inventory.coins").eval().unwrap();
        assert_eq!(coins, 1, "registered state rolled back");
        let saved = vm.save().unwrap();
        assert_eq!(saved.get("vars").get("state").get("inventory").get("coins"), &Value::Int(1));
    }

    #[test]
    fn refuses_unsaveable_variables() {
        let (mut vm, mut h) = setup("label start:\n    $ cb = function() end\n    \"x\"\n");
        let err = vm.next(&mut h).unwrap_err();
        assert!(err.message.contains("story variable `story.cb` holds a function"), "{err}");
    }

    #[test]
    fn luau_errors_point_into_the_story() {
        let program = compile(&[(
            "t.kulisa",
            "label start:\n    $ trust +=\n    lua:\n        local a = 1\n        a = = 2\n",
        )])
        .unwrap();
        let mut host = StoryHost::new(&Lua::new()).unwrap();
        let errs = host.prepare(&program).unwrap_err();
        assert_eq!(errs.0.len(), 2, "{errs}");
        assert_eq!((errs.0[0].loc.line, errs.0[0].loc.col), (2, 7));
        assert!(errs.0[0].message.starts_with("Luau:"), "{}", errs.0[0].message);
        assert_eq!(errs.0[1].loc.line, 5, "second line of the lua: block");

        let (mut vm, mut h) = setup("label start:\n    $ missing_function()\n");
        let err = vm.next(&mut h).unwrap_err().to_string();
        assert_eq!(err, "t.kulisa:2:7: error: attempt to call a nil value");
    }
}
