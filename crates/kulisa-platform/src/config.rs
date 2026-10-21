//! The game's `kulisa.toml`: a TOML subset (tables, dotted keys, strings, numbers, booleans,
//! arrays, inline tables; no dates or multi-line strings) read by the runtime, the scripts
//! (`kulisa.config`) and the build tools (manifests, plists, the web page).

use std::collections::BTreeMap;

pub const CONFIG_FILE: &str = "kulisa.toml";

#[derive(Clone, Debug, PartialEq)]
pub enum ConfigValue {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<ConfigValue>),
    Table(BTreeMap<String, ConfigValue>),
}

impl ConfigValue {
    fn kind(&self) -> &'static str {
        match self {
            ConfigValue::String(_) => "a string",
            ConfigValue::Integer(_) | ConfigValue::Float(_) => "a number",
            ConfigValue::Bool(_) => "a boolean",
            ConfigValue::Array(_) => "an array",
            ConfigValue::Table(_) => "a table",
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ConfigValue::Integer(i) => Some(*i as f64),
            ConfigValue::Float(f) => Some(*f),
            _ => None,
        }
    }
}

/// A parsed `kulisa.toml`. Lookups take dotted paths like `"display.width"`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    pub root: BTreeMap<String, ConfigValue>,
}

impl Config {
    /// Parses `source`; errors read `kulisa.toml:line:col: message`.
    pub fn parse(source: &str) -> Result<Config, String> {
        Parser { src: source.as_bytes(), pos: 0, line: 1, line_start: 0 }.document()
    }

    pub fn get(&self, path: &str) -> Option<&ConfigValue> {
        let mut parts = path.split('.');
        let mut value = self.root.get(parts.next()?)?;
        for p in parts {
            match value {
                ConfigValue::Table(t) => value = t.get(p)?,
                _ => return None,
            }
        }
        Some(value)
    }

    fn typed<'a, T>(
        &'a self,
        path: &str,
        want: &str,
        f: impl Fn(&'a ConfigValue) -> Option<T>,
    ) -> Result<Option<T>, String> {
        match self.get(path) {
            None => Ok(None),
            Some(v) => f(v)
                .map(Some)
                .ok_or_else(|| format!("{CONFIG_FILE}: `{path}` must be {want}, not {}", v.kind())),
        }
    }

    pub fn str(&self, path: &str) -> Result<Option<&str>, String> {
        self.typed(path, "a string", |v| match v {
            ConfigValue::String(s) => Some(s.as_str()),
            _ => None,
        })
    }

    pub fn number(&self, path: &str) -> Result<Option<f64>, String> {
        self.typed(path, "a number", ConfigValue::as_f64)
    }

    pub fn bool(&self, path: &str) -> Result<Option<bool>, String> {
        self.typed(path, "true or false", |v| match v {
            ConfigValue::Bool(b) => Some(*b),
            _ => None,
        })
    }

    /// A pair of numbers, like `window = [1280, 720]`.
    pub fn pair(&self, path: &str) -> Result<Option<(f64, f64)>, String> {
        self.typed(path, "two numbers like [1280, 720]", |v| match v {
            ConfigValue::Array(a) if a.len() == 2 => Some((a[0].as_f64()?, a[1].as_f64()?)),
            _ => None,
        })
    }
}

/// The settings the engine and the build tools use, with their defaults filled in.
#[derive(Clone, Debug, PartialEq)]
pub struct GameConfig {
    /// Storage and package id: letters, digits, `_` and `-` (`[game] id`).
    pub id: String,
    pub title: String,
    /// Shown to players (`1.2.0`).
    pub version: String,
    /// Increases with every store upload (Android `versionCode`, iOS `CFBundleVersion`).
    pub build: u32,
    /// `[display] orientation`: `auto`, `landscape`, `portrait` or `any`.
    pub orientation: String,
    /// `[display] width` and `height` (the design resolution).
    pub design: (f64, f64),
    /// Desktop window size in logical pixels (`[window] size`).
    pub window: Option<(f64, f64)>,
    pub fullscreen: bool,
    /// Page and window background before the game draws (`[window] background`).
    pub background: String,
    /// `[quality] tier`: `auto`, `low`, `medium` or `high`.
    pub quality: String,
    pub android_package: String,
    pub ios_bundle_id: String,
    pub raw: Config,
}

pub const ORIENTATIONS: &[&str] = &["auto", "landscape", "portrait", "any"];
pub const QUALITY_TIERS: &[&str] = &["auto", "low", "medium", "high"];

/// Whether `id` can name storage folders and packages.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl GameConfig {
    /// Reads the settings from `kulisa.toml` source; `None` gives all defaults.
    pub fn parse(source: Option<&str>) -> Result<GameConfig, String> {
        let raw = source.map(Config::parse).transpose()?.unwrap_or_default();
        let id = raw.str("game.id")?.unwrap_or("kulisa_game").to_string();
        if !valid_id(&id) {
            return Err(format!("{CONFIG_FILE}: `game.id` may only use letters, digits, `_` and `-`"));
        }
        let one_of = |path: &str, allowed: &[&str], default: &str| -> Result<String, String> {
            let v = raw.str(path)?.unwrap_or(default);
            if allowed.contains(&v) {
                Ok(v.to_string())
            } else {
                Err(format!("{CONFIG_FILE}: `{path}` must be one of {}", allowed.join(", ")))
            }
        };
        // Package ids allow only letters, digits and `_` between dots.
        let package_part: String = id.chars().map(|c| if c == '-' { '_' } else { c }).collect();
        let package_part = if package_part.starts_with(|c: char| c.is_ascii_digit()) {
            format!("g{package_part}")
        } else {
            package_part
        };
        let build = raw.number("game.build")?.unwrap_or(1.0);
        if build < 1.0 || build.fract() != 0.0 {
            return Err(format!("{CONFIG_FILE}: `game.build` must be a whole number from 1"));
        }
        Ok(GameConfig {
            title: raw.str("game.title")?.unwrap_or("Kulisa").to_string(),
            version: raw.str("game.version")?.unwrap_or("1.0.0").to_string(),
            build: build as u32,
            orientation: one_of("display.orientation", ORIENTATIONS, "auto")?,
            design: (
                raw.number("display.width")?.unwrap_or(1920.0),
                raw.number("display.height")?.unwrap_or(1080.0),
            ),
            window: raw.pair("window.size")?,
            fullscreen: raw.bool("window.fullscreen")?.unwrap_or(false),
            background: raw.str("window.background")?.unwrap_or("#000000").to_string(),
            quality: one_of("quality.tier", QUALITY_TIERS, "auto")?,
            android_package: raw
                .str("android.package")?
                .map_or_else(|| format!("dev.kulisa.{package_part}"), str::to_string),
            ios_bundle_id: raw
                .str("ios.bundle_id")?
                .map_or_else(|| format!("dev.kulisa.{}", id.replace('_', "-")), str::to_string),
            id,
            raw,
        })
    }

    /// The orientation the device should lock to: `auto` follows the design's shape.
    pub fn locked_orientation(&self) -> &str {
        match self.orientation.as_str() {
            "auto" if self.design.1 > self.design.0 => "portrait",
            "auto" => "landscape",
            o => o,
        }
    }
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
    line_start: usize,
}

type Table = BTreeMap<String, ConfigValue>;

impl Parser<'_> {
    fn error(&self, msg: impl std::fmt::Display) -> String {
        format!("{CONFIG_FILE}:{}:{}: {msg}", self.line, self.pos - self.line_start + 1)
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
            self.line_start = self.pos;
        }
        Some(c)
    }

    /// Skips spaces and tabs (and newlines and comments when `lines`).
    fn skip(&mut self, lines: bool) {
        while let Some(c) = self.peek() {
            match c {
                b' ' | b'\t' | b'\r' => {}
                b'\n' if lines => {}
                b'#' => {
                    while self.peek().is_some_and(|c| c != b'\n') {
                        self.pos += 1;
                    }
                    continue;
                }
                _ => return,
            }
            self.bump();
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.bump();
            Ok(())
        } else {
            Err(self.error(format!("expected `{}`", c as char)))
        }
    }

    /// The rest of the line must be empty (or a comment).
    fn end_of_line(&mut self) -> Result<(), String> {
        self.skip(false);
        match self.peek() {
            None => Ok(()),
            Some(b'\n') => {
                self.bump();
                Ok(())
            }
            Some(_) => Err(self.error("expected the end of the line")),
        }
    }

    fn document(mut self) -> Result<Config, String> {
        let mut root = Table::new();
        let mut current: Vec<String> = Vec::new();
        let mut defined: Vec<Vec<String>> = Vec::new();
        loop {
            self.skip(true);
            match self.peek() {
                None => return Ok(Config { root }),
                Some(b'[') => {
                    self.bump();
                    self.skip(false);
                    current = self.key()?;
                    self.skip(false);
                    self.expect(b']')?;
                    if defined.contains(&current) {
                        return Err(self.error(format!("table `[{}]` is defined twice", current.join("."))));
                    }
                    defined.push(current.clone());
                    table_at(&mut root, &current).map_err(|e| self.error(e))?;
                    self.end_of_line()?;
                }
                Some(_) => {
                    let (path, value) = self.pair()?;
                    let mut full = current.clone();
                    full.extend(path);
                    insert(&mut root, &full, value).map_err(|e| self.error(e))?;
                    self.end_of_line()?;
                }
            }
        }
    }

    /// `a.b.c` (bare or quoted parts).
    fn key(&mut self) -> Result<Vec<String>, String> {
        let mut parts = vec![self.key_part()?];
        loop {
            self.skip(false);
            if self.peek() != Some(b'.') {
                return Ok(parts);
            }
            self.bump();
            self.skip(false);
            parts.push(self.key_part()?);
        }
    }

    fn key_part(&mut self) -> Result<String, String> {
        if self.peek() == Some(b'"') {
            return self.string();
        }
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(self.error("expected a key"));
        }
        Ok(String::from_utf8_lossy(&self.src[start..self.pos]).into_owned())
    }

    fn pair(&mut self) -> Result<(Vec<String>, ConfigValue), String> {
        let key = self.key()?;
        self.skip(false);
        self.expect(b'=')?;
        self.skip(false);
        Ok((key, self.value()?))
    }

    fn value(&mut self) -> Result<ConfigValue, String> {
        match self.peek() {
            Some(b'"') => self.string().map(ConfigValue::String),
            Some(b'\'') => {
                self.bump();
                let start = self.pos;
                while self.peek().is_some_and(|c| c != b'\'' && c != b'\n') {
                    self.pos += 1;
                }
                let s = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
                self.expect(b'\'')?;
                Ok(ConfigValue::String(s))
            }
            Some(b'[') => {
                self.bump();
                let mut items = Vec::new();
                loop {
                    self.skip(true);
                    if self.peek() == Some(b']') {
                        self.bump();
                        return Ok(ConfigValue::Array(items));
                    }
                    items.push(self.value()?);
                    self.skip(true);
                    match self.peek() {
                        Some(b',') => {
                            self.bump();
                        }
                        Some(b']') => {}
                        _ => return Err(self.error("expected `,` or `]`")),
                    }
                }
            }
            Some(b'{') => {
                self.bump();
                let mut table = Table::new();
                self.skip(false);
                if self.peek() == Some(b'}') {
                    self.bump();
                    return Ok(ConfigValue::Table(table));
                }
                loop {
                    self.skip(false);
                    let (path, value) = self.pair()?;
                    insert(&mut table, &path, value).map_err(|e| self.error(e))?;
                    self.skip(false);
                    match self.bump() {
                        Some(b',') => {}
                        Some(b'}') => return Ok(ConfigValue::Table(table)),
                        _ => return Err(self.error("expected `,` or `}`")),
                    }
                }
            }
            Some(_) => {
                let start = self.pos;
                while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || b"+-._".contains(&c)) {
                    self.pos += 1;
                }
                let word = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").replace('_', "");
                match word.as_str() {
                    "" => Err(self.error("expected a value")),
                    "true" => Ok(ConfigValue::Bool(true)),
                    "false" => Ok(ConfigValue::Bool(false)),
                    w => w
                        .parse::<i64>()
                        .map(ConfigValue::Integer)
                        .or_else(|_| w.parse::<f64>().map(ConfigValue::Float))
                        .map_err(|_| {
                            self.pos = start;
                            self.error(format!("`{w}` is not a value (strings need quotes)"))
                        }),
                }
            }
            None => Err(self.error("expected a value")),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            match self.bump() {
                None | Some(b'\n') => return Err(self.error("unterminated string")),
                Some(b'"') => return String::from_utf8(out).map_err(|_| self.error("invalid UTF-8")),
                Some(b'\\') => match self.bump() {
                    Some(b'n') => out.push(b'\n'),
                    Some(b't') => out.push(b'\t'),
                    Some(b'"') => out.push(b'"'),
                    Some(b'\\') => out.push(b'\\'),
                    Some(b'u') => {
                        let hex = self.src.get(self.pos..self.pos + 4).unwrap_or_default();
                        let c = std::str::from_utf8(hex)
                            .ok()
                            .and_then(|h| u32::from_str_radix(h, 16).ok())
                            .and_then(char::from_u32)
                            .ok_or_else(|| self.error("bad \\u escape"))?;
                        self.pos += 4;
                        out.extend(c.to_string().bytes());
                    }
                    _ => return Err(self.error("unknown escape")),
                },
                Some(c) => out.push(c),
            }
        }
    }
}

/// The table at `path`, created on the way.
fn table_at<'t>(root: &'t mut Table, path: &[String]) -> Result<&'t mut Table, String> {
    let mut t = root;
    for (i, p) in path.iter().enumerate() {
        let entry = t.entry(p.clone()).or_insert_with(|| ConfigValue::Table(Table::new()));
        match entry {
            ConfigValue::Table(next) => t = next,
            _ => return Err(format!("`{}` is not a table", path[..=i].join("."))),
        }
    }
    Ok(t)
}

fn insert(root: &mut Table, path: &[String], value: ConfigValue) -> Result<(), String> {
    let (last, parents) = path.split_last().expect("keys have a part");
    let table = table_at(root, parents)?;
    if table.contains_key(last) {
        return Err(format!("`{}` is set twice", path.join(".")));
    }
    table.insert(last.clone(), value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# A game.
[game]
id = "cat_tales"
title = "Cat Tales \"2\""   # trailing comment
version = '1.2.0'
build = 3

[display]
width = 1_920
height = 1080.0
orientation = "landscape"
aspect = ["4:3", "21:9"]
phone = { width = 1080, height = 1920, orientation = "portrait" }

[display.tablet]
scale = "fit"

[window]
size = [
    1280,
    720,
]
fullscreen = false
"#;

    #[test]
    fn parses_tables_values_and_comments() {
        let c = Config::parse(SAMPLE).unwrap();
        assert_eq!(c.str("game.title").unwrap(), Some("Cat Tales \"2\""));
        assert_eq!(c.str("game.version").unwrap(), Some("1.2.0"));
        assert_eq!(c.number("display.width").unwrap(), Some(1920.0));
        assert_eq!(c.number("display.height").unwrap(), Some(1080.0));
        assert_eq!(c.number("display.phone.height").unwrap(), Some(1920.0));
        assert_eq!(c.str("display.tablet.scale").unwrap(), Some("fit"));
        assert_eq!(c.pair("window.size").unwrap(), Some((1280.0, 720.0)));
        assert_eq!(c.bool("window.fullscreen").unwrap(), Some(false));
        assert!(matches!(c.get("display.aspect"), Some(ConfigValue::Array(a)) if a.len() == 2));
        assert_eq!(c.get("nothing.here"), None);
    }

    #[test]
    fn errors_name_line_and_column() {
        let e = Config::parse("[game]\ntitle = Cats\n").unwrap_err();
        assert_eq!(e, "kulisa.toml:2:9: `Cats` is not a value (strings need quotes)");
        let e = Config::parse("a = 1\na = 2").unwrap_err();
        assert!(e.starts_with("kulisa.toml:2:") && e.contains("`a` is set twice"), "{e}");
        let e = Config::parse("[a]\n[a]").unwrap_err();
        assert!(e.contains("defined twice"), "{e}");
        let e = Config::parse("x = \"open").unwrap_err();
        assert!(e.contains("unterminated string"), "{e}");
        let e = Config::parse("x = 1 2").unwrap_err();
        assert!(e.contains("end of the line"), "{e}");
        let e = Config::parse("x = 1\n[x]").unwrap_err();
        assert!(e.contains("`x` is not a table"), "{e}");
    }

    #[test]
    fn game_config_defaults_and_checks() {
        let g = GameConfig::parse(None).unwrap();
        assert_eq!((g.id.as_str(), g.title.as_str(), g.build), ("kulisa_game", "Kulisa", 1));
        assert_eq!(g.locked_orientation(), "landscape");
        assert_eq!(g.android_package, "dev.kulisa.kulisa_game");

        let g = GameConfig::parse(Some(SAMPLE)).unwrap();
        assert_eq!((g.id.as_str(), g.version.as_str(), g.build), ("cat_tales", "1.2.0", 3));
        assert_eq!(g.window, Some((1280.0, 720.0)));
        assert_eq!(g.ios_bundle_id, "dev.kulisa.cat-tales");

        let g = GameConfig::parse(Some("[display]\nwidth = 1080\nheight = 1920")).unwrap();
        assert_eq!(g.locked_orientation(), "portrait");
        assert!(GameConfig::parse(Some("[game]\nid = \"a b\"")).is_err());
        assert!(GameConfig::parse(Some("[quality]\ntier = \"ultra\"")).is_err());
        let e = GameConfig::parse(Some("[game]\ntitle = 5")).unwrap_err();
        assert_eq!(e, "kulisa.toml: `game.title` must be a string, not a number");
    }
}
