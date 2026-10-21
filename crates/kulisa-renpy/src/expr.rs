//! Python expressions and simple statements as Luau: literals, operators, calls (keyword
//! arguments become a trailing table, as `Character("Eileen", { color = "#fff" })`), lists,
//! dicts and a few builtins. Anything else (indexing, comprehensions, methods, string
//! formatting) is reported as unsupported, so the line can be left for a person to port.

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Tok {
    /// A decoded string literal.
    Str(String),
    Num(String),
    Ident(String),
    Op(String),
    Open(char),
    Close(char),
    Comma,
    Colon,
}

pub(crate) type Result<T> = std::result::Result<T, String>;

/// Reads a Python string literal at `s[i..]` (prefixes `r`, `u`, `b`; single, double and
/// triple quotes). Returns the value and the index after it.
pub(crate) fn string_at(s: &[char], start: usize) -> Result<Option<(String, usize)>> {
    let mut i = start;
    while i < s.len() && i - start < 2 && "rRuUbBfF".contains(s[i]) {
        i += 1;
    }
    let Some(&q) = s.get(i).filter(|c| **c == '"' || **c == '\'') else { return Ok(None) };
    let prefix: String = s[start..i].iter().collect();
    if prefix.contains(['f', 'F']) {
        return Err("f-strings are not converted".into());
    }
    let raw = prefix.contains(['r', 'R']);
    let triple = s.get(i + 1) == Some(&q) && s.get(i + 2) == Some(&q);
    i += if triple { 3 } else { 1 };
    let mut out = String::new();
    loop {
        let Some(&c) = s.get(i) else { return Err("unterminated string".into()) };
        if c == q && (!triple || (s.get(i + 1) == Some(&q) && s.get(i + 2) == Some(&q))) {
            return Ok(Some((out, i + if triple { 3 } else { 1 })));
        }
        if c == '\\' && !raw {
            let n = s.get(i + 1).copied().unwrap_or('\\');
            match n {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '\n' => {}
                '\\' | '\'' | '"' => out.push(n),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
}

pub(crate) fn tokenize(src: &str) -> Result<Vec<Tok>> {
    let s: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if let Some((v, next)) = string_at(&s, i)? {
            out.push(Tok::Str(v));
            i = next;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && s.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let start = i;
            while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == '.' || s[i] == '_') {
                i += 1;
            }
            out.push(Tok::Num(s[start..i].iter().filter(|c| **c != '_').collect()));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < s.len() && (s[i].is_alphanumeric() || s[i] == '_') {
                i += 1;
            }
            out.push(Tok::Ident(s[start..i].iter().collect()));
            continue;
        }
        let two: String = s[i..(i + 2).min(s.len())].iter().collect();
        let three: String = s[i..(i + 3).min(s.len())].iter().collect();
        if ["**=", "//="].contains(&three.as_str()) {
            return Err(format!("`{three}` is not converted"));
        }
        if ["==", "!=", "<=", ">=", "**", "//", "+=", "-=", "*=", "/=", "%="].contains(&two.as_str()) {
            out.push(Tok::Op(two));
            i += 2;
            continue;
        }
        i += 1;
        out.push(match c {
            '(' | '[' | '{' => Tok::Open(c),
            ')' | ']' | '}' => Tok::Close(c),
            ',' => Tok::Comma,
            ':' => Tok::Colon,
            '+' | '-' | '*' | '/' | '%' | '<' | '>' | '=' | '.' => Tok::Op(c.to_string()),
            other => return Err(format!("`{other}` is not converted")),
        });
    }
    Ok(out)
}

/// A Luau string literal.
pub(crate) fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The index of the bracket closing the one at `open`.
fn matching(toks: &[Tok], open: usize) -> Result<usize> {
    let mut depth = 0;
    for (i, t) in toks.iter().enumerate().skip(open) {
        match t {
            Tok::Open(_) => depth += 1,
            Tok::Close(_) => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
    }
    Err("unbalanced brackets".into())
}

/// Splits at top-level commas.
fn split_commas(toks: &[Tok]) -> Vec<&[Tok]> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::Open(_) => depth += 1,
            Tok::Close(_) => depth -= 1,
            Tok::Comma if depth == 0 => {
                parts.push(&toks[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < toks.len() {
        parts.push(&toks[start..]);
    }
    parts
}

/// Python names with a Luau counterpart, called as functions.
fn builtin(name: &str) -> Option<&'static str> {
    Some(match name {
        "str" => "tostring",
        "float" => "tonumber",
        "int" => "math.floor",
        "abs" => "math.abs",
        "max" => "math.max",
        "min" => "math.min",
        "round" => "math.round",
        "renpy.random.randint" | "renpy.random.random" => "math.random",
        "Character" | "DynamicCharacter" => "Character",
        _ => return None,
    })
}

fn is_ident(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

const UNARY_MINUS_TOKEN: &str = "-\u{1}";
const UNARY_MINUS: &str = "-\u{1} ";

/// Whether the token before `i` ends an operand (so a `-` there subtracts).
fn after_operand(toks: &[Tok], i: usize) -> bool {
    match i.checked_sub(1).map(|p| &toks[p]) {
        Some(Tok::Ident(k)) => !["and", "or", "not"].contains(&k.as_str()),
        Some(Tok::Num(_) | Tok::Str(_) | Tok::Close(_)) => true,
        _ => false,
    }
}

/// Converts a Python expression to Luau.
pub(crate) fn expr(src: &str) -> Result<String> {
    expr_toks(&tokenize(src)?)
}

pub(crate) fn expr_toks(toks: &[Tok]) -> Result<String> {
    if toks.is_empty() {
        return Err("empty expression".into());
    }
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        // What came before decides whether `(` calls and `[` indexes.
        let after_value =
            matches!(i.checked_sub(1).map(|p| &toks[p]), Some(Tok::Ident(_) | Tok::Str(_) | Tok::Close(_)));
        match t {
            Tok::Str(s) => out.push(quote(s)),
            Tok::Num(n) => {
                if n.contains(|c: char| c.is_ascii_alphabetic() && !"eExX".contains(c)) {
                    return Err(format!("number `{n}` is not converted"));
                }
                out.push(n.clone());
            }
            Tok::Ident(name) => match name.as_str() {
                "True" => out.push("true".into()),
                "False" => out.push("false".into()),
                "None" => out.push("nil".into()),
                "and" | "or" => out.push(name.clone()),
                "not" if matches!(toks.get(i + 1), Some(Tok::Ident(n)) if n == "in") => {
                    return Err("`not in` is not converted".into());
                }
                "not" => out.push("not".into()),
                "in" | "is" | "lambda" | "for" | "if" | "else" | "yield" | "await" => {
                    return Err(format!("`{name}` expressions are not converted"));
                }
                _ => {
                    // A dotted name: a.b.c.
                    let mut path = name.clone();
                    let mut j = i + 1;
                    while let (Some(Tok::Op(dot)), Some(Tok::Ident(part))) = (toks.get(j), toks.get(j + 1))
                        && dot == "."
                    {
                        path = format!("{path}.{part}");
                        j += 2;
                    }
                    let path = path.strip_prefix("store.").map(str::to_string).unwrap_or(path);
                    let call = matches!(toks.get(j), Some(Tok::Open('(')));
                    if call {
                        let close = matching(toks, j)?;
                        out.push(call_expr(&path, &toks[j + 1..close])?);
                        i = close + 1;
                        continue;
                    }
                    if path.starts_with("renpy.")
                        || path.starts_with("persistent.")
                        || path.starts_with("config.")
                    {
                        return Err(format!("`{path}` is Ren'Py's own and is not converted"));
                    }
                    out.push(path);
                    i = j;
                    continue;
                }
            },
            Tok::Op(op) => out.push(match op.as_str() {
                "!=" => "~=".into(),
                // A unary minus sticks to what follows (marked here, joined at the end).
                "-" if !after_operand(toks, i) => UNARY_MINUS_TOKEN.into(),
                "**" => "^".into(),
                "=" => return Err("assignment inside an expression".into()),
                "." => return Err("method calls are not converted".into()),
                // Joining strings: Luau uses `..`.
                "+" if matches!(toks.get(i + 1), Some(Tok::Str(_)))
                    || matches!(i.checked_sub(1).map(|p| &toks[p]), Some(Tok::Str(_))) =>
                {
                    "..".into()
                }
                "%" if matches!(i.checked_sub(1).map(|p| &toks[p]), Some(Tok::Str(_))) => {
                    return Err("`%` string formatting is not converted".into());
                }
                other => other.to_string(),
            }),
            Tok::Open(b) => {
                let close = matching(toks, i)?;
                let inner = &toks[i + 1..close];
                match b {
                    '(' if after_value => return Err("calling a computed value is not converted".into()),
                    '(' => {
                        let parts = split_commas(inner);
                        if parts.len() > 1 || matches!(inner.last(), Some(Tok::Comma)) {
                            out.push(table(&parts)?);
                        } else {
                            out.push(format!("({})", expr_toks(inner)?));
                        }
                    }
                    '[' if after_value => {
                        return Err("indexing is not converted (Python counts from 0, Luau from 1)".into());
                    }
                    '[' => out.push(table(&split_commas(inner))?),
                    _ => out.push(dict(inner)?),
                }
                i = close + 1;
                continue;
            }
            Tok::Close(_) => return Err("unbalanced brackets".into()),
            Tok::Comma => return Err("a tuple is not converted here".into()),
            Tok::Colon => return Err("`:` is not converted here".into()),
        }
        i += 1;
    }
    Ok(out.join(" ").replace("( ", "(").replace(" )", ")").replace(UNARY_MINUS, "-"))
}

/// A list or tuple as a Luau array.
fn table(parts: &[&[Tok]]) -> Result<String> {
    let items: Result<Vec<String>> = parts.iter().filter(|p| !p.is_empty()).map(|p| expr_toks(p)).collect();
    Ok(format!("{{ {} }}", items?.join(", ")).replace("{  }", "{}"))
}

fn dict(inner: &[Tok]) -> Result<String> {
    let mut items = Vec::new();
    for part in split_commas(inner).into_iter().filter(|p| !p.is_empty()) {
        let colon = part.iter().position(|t| *t == Tok::Colon).ok_or("sets are not converted")?;
        let value = expr_toks(&part[colon + 1..])?;
        match &part[..colon] {
            [Tok::Str(k)] if is_ident(k) => items.push(format!("{k} = {value}")),
            key => items.push(format!("[{}] = {value}", expr_toks(key)?)),
        }
    }
    Ok(format!("{{ {} }}", items.join(", ")).replace("{  }", "{}"))
}

/// A call: keyword arguments go into a trailing table.
fn call_expr(name: &str, args: &[Tok]) -> Result<String> {
    let (mut positional, mut keywords) = (Vec::new(), Vec::new());
    for part in split_commas(args).into_iter().filter(|p| !p.is_empty()) {
        match part {
            [Tok::Ident(k), Tok::Op(eq), rest @ ..] if eq == "=" => {
                keywords.push(format!("{k} = {}", expr_toks(rest)?));
            }
            [Tok::Op(star), ..] if star == "*" || star == "**" => {
                return Err("`*args` calls are not converted".into());
            }
            _ => positional.push(expr_toks(part)?),
        }
    }
    match name {
        // Translation marker: the text itself.
        "_" | "__" if positional.len() == 1 && keywords.is_empty() => return Ok(positional.remove(0)),
        "len" if positional.len() == 1 => return Ok(format!("#{}", positional[0])),
        "renpy.random.random" if positional.is_empty() => return Ok("math.random()".into()),
        _ => {}
    }
    let func = match builtin(name) {
        Some(f) => f.to_string(),
        None if name.starts_with("renpy.") => {
            return Err(format!("`{name}` is Ren'Py's own and is not converted"));
        }
        // `list.append(x)`, `s.upper()`: Python methods have no Luau equivalent here.
        None if name.contains('.') => {
            return Err(format!("`{name}(...)` (a Python method or module call) is not converted"));
        }
        None => name.to_string(),
    };
    if func == "Character" && positional.first().is_some_and(|p| p == "nil") {
        positional[0] = "\"\"".into();
    }
    if !keywords.is_empty() {
        positional.push(format!("{{ {} }}", keywords.join(", ")));
    }
    Ok(format!("{func}({})", positional.join(", ")))
}

/// Converts a Python statement (a `$` line or a line of a `python:` block) to Luau.
pub(crate) fn statement(src: &str) -> Result<String> {
    let toks = tokenize(src)?;
    if let [Tok::Ident(kw), ..] = toks.as_slice()
        && [
            "import", "from", "def", "class", "for", "while", "if", "elif", "else", "try", "with", "global",
            "del", "return", "raise", "assert",
        ]
        .contains(&kw.as_str())
    {
        return Err(format!("Python `{kw}` statements are not converted"));
    }
    if toks == [Tok::Ident("pass".into())] {
        return Ok(String::new());
    }
    // `target = value` or `target op= value`, with a plain (dotted) name as the target.
    let assign = toks
        .iter()
        .position(|t| matches!(t, Tok::Op(op) if ["=", "+=", "-=", "*=", "/=", "%="].contains(&op.as_str())));
    if let Some(at) = assign {
        let target = &toks[..at];
        let simple = !target.is_empty()
            && target.iter().enumerate().all(|(k, t)| match t {
                Tok::Ident(_) => k % 2 == 0,
                Tok::Op(dot) => dot == "." && k % 2 == 1,
                _ => false,
            });
        if !simple {
            return Err("only assignments to plain names are converted".into());
        }
        let name = expr_toks(target)?;
        let Tok::Op(op) = &toks[at] else { unreachable!() };
        let value = expr_toks(&toks[at + 1..])?;
        let op = if op == "+=" && value.starts_with('"') { "..=" } else { op.as_str() };
        return Ok(format!("{name} {op} {value}"));
    }
    expr_toks(&toks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions() {
        assert_eq!(expr("True and not flag").unwrap(), "true and not flag");
        assert_eq!(expr("points != 3 or x >= 2**3").unwrap(), "points ~= 3 or x >= 2 ^ 3");
        assert_eq!(expr("'Hi, ' + name").unwrap(), "\"Hi, \" .. name");
        assert_eq!(expr("[1, 2, 3]").unwrap(), "{ 1, 2, 3 }");
        assert_eq!(expr("{'a': 1, 2: 'b'}").unwrap(), "{ a = 1, [2] = \"b\" }");
        assert_eq!(expr("len(bag) > 0").unwrap(), "#bag > 0");
        assert_eq!(expr("_(\"Hello\")").unwrap(), "\"Hello\"");
        assert_eq!(expr("renpy.random.randint(1, 6)").unwrap(), "math.random(1, 6)");
        assert_eq!(expr("store.money - 5").unwrap(), "money - 5");
        assert_eq!(
            expr("Character(\"Eileen\", color=\"#c8ffc8\", who_bold=True)").unwrap(),
            "Character(\"Eileen\", { color = \"#c8ffc8\", who_bold = true })"
        );
        assert_eq!(expr("Character(None, kind=nvl)").unwrap(), "Character(\"\", { kind = nvl })");
        assert_eq!(expr("-1").unwrap(), "-1");
        assert_eq!(expr("(a + b) * 2").unwrap(), "(a + b) * 2");
    }

    #[test]
    fn unsupported_expressions() {
        for src in
            ["x in bag", "bag[0]", "f'{x}'", "'%d' % n", "a if b else c", "s.upper()", "renpy.showing('x')"]
        {
            assert!(expr(src).is_err(), "{src}");
        }
    }

    #[test]
    fn statements() {
        assert_eq!(statement("points += 1").unwrap(), "points += 1");
        assert_eq!(statement("name = 'Sam'").unwrap(), "name = \"Sam\"");
        assert_eq!(statement("log += ' and more'").unwrap(), "log ..= \" and more\"");
        assert_eq!(statement("pass").unwrap(), "");
        assert!(statement("import random").is_err());
        assert!(statement("a, b = 1, 2").is_err());
        assert!(statement("bag[0] = 1").is_err());
    }

    #[test]
    fn strings() {
        let s: Vec<char> = r#"r"a\n" 'it\'s' """x"y""" "#.chars().collect();
        let (a, i) = string_at(&s, 0).unwrap().unwrap();
        assert_eq!(a, "a\\n");
        let (b, j) = string_at(&s, i + 1).unwrap().unwrap();
        assert_eq!(b, "it's");
        let (c, _) = string_at(&s, j + 1).unwrap().unwrap();
        assert_eq!(c, "x\"y");
        assert_eq!(quote("say \"hi\"\n"), r#""say \"hi\"\n""#);
    }
}
