//! Parser: lines into a syntax tree, recovering at statement boundaries so one run reports
//! every error.

use crate::ast::*;
use crate::diag::{Diagnostic, Loc};
use crate::lex::{Cursor, Line, LineKind, lines};

struct Parser<'a> {
    file: &'a str,
    lines: Vec<Line>,
    i: usize,
    errors: Vec<Diagnostic>,
}

/// Parses one file. On failure returns every error found.
pub fn parse(file: &str, src: &str) -> Result<File, Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let lines = lines(file, src, &mut errors);
    let mut p = Parser { file, lines, i: 0, errors };
    let ast = p.file();
    if p.errors.is_empty() { Ok(ast) } else { Err(p.errors) }
}

impl<'a> Parser<'a> {
    fn loc(&self, line: &Line) -> Loc {
        Loc { file: self.file.to_string(), line: line.no, col: line.indent + 1 }
    }

    /// Index of the next code line at or after `self.i`.
    fn next_code(&self) -> Option<usize> {
        (self.i..self.lines.len()).find(|&j| self.lines[j].kind == LineKind::Code)
    }

    /// Skips a line and everything indented under it (error recovery).
    fn skip_block(&mut self, at: usize) {
        let indent = self.lines[at].indent;
        self.i = at + 1;
        while let Some(j) = self.next_code() {
            if self.lines[j].indent <= indent {
                break;
            }
            self.i = j + 1;
        }
    }

    fn file(&mut self) -> File {
        let mut f =
            File { name: self.file.into(), defines: vec![], defaults: vec![], init: vec![], labels: vec![] };
        while let Some(j) = self.next_code() {
            let line = self.lines[j].clone();
            self.i = j + 1;
            if line.indent > 0 {
                self.errors.push(Diagnostic::new(self.loc(&line), "unexpected indentation", &line.raw));
                self.skip_block(j);
                continue;
            }
            let mut c = Cursor::new(self.file, &line);
            let result = if c.keyword("label") {
                self.label(&line, c).map(|l| f.labels.push(l))
            } else if c.keyword("define") {
                self.binding(&line, c).map(|b| f.defines.push(b))
            } else if c.keyword("default") {
                self.binding(&line, c).map(|b| f.defaults.push(b))
            } else if c.keyword("lua") {
                self.lua_block(&line, c).map(|code| f.init.push(code))
            } else {
                Err(c.error("expected `label`, `define`, `default` or `lua:` here; story statements go inside a label"))
            };
            if let Err(e) = result {
                self.errors.push(e);
                self.skip_block(j);
            }
        }
        f
    }

    fn label(&mut self, line: &Line, mut c: Cursor) -> Result<Label, Diagnostic> {
        let name = c.ident(true).ok_or_else(|| c.error("expected a label name, like `label start:`"))?;
        if !c.eat(':') {
            return Err(c.error("expected `:` after the label name"));
        }
        if !c.at_end() {
            let rest = c.rest_str().trim().to_string();
            return Err(c.error(format!("unexpected `{rest}` after `label {name}:`")));
        }
        let body = self.block(line, &format!("label {name}:"))?;
        Ok(Label { name, loc: self.loc(line), body })
    }

    fn binding(&mut self, line: &Line, mut c: Cursor) -> Result<Binding, Diagnostic> {
        let name_loc = c.loc();
        let name = c
            .ident(false)
            .ok_or_else(|| c.error("expected a name, like `define mila = Character(\"Mila\")`"))?;
        if KEYWORDS.contains(&name.as_str()) {
            return Err(Diagnostic::new(
                name_loc,
                format!("`{name}` is a statement keyword and cannot be used as a name"),
                &line.raw,
            ));
        }
        if !c.eat('=') {
            return Err(c.error(format!("expected `=` after `{name}`")));
        }
        let expr_loc = {
            c.skip_ws();
            c.loc()
        };
        let expr = c.rest();
        if expr.is_empty() {
            return Err(c.error(format!("expected a value after `{name} =`")));
        }
        Ok(Binding { name, expr, loc: self.loc(line), expr_loc })
    }

    /// A raw Luau block: every following line indented deeper (blank lines included).
    fn lua_block(&mut self, line: &Line, mut c: Cursor) -> Result<Code, Diagnostic> {
        if !c.eat(':') || !c.at_end() {
            return Err(c.error("expected `lua:` followed by an indented block of Luau"));
        }
        let start = self.i;
        let mut end = start;
        while end < self.lines.len() {
            let l = &self.lines[end];
            if l.kind == LineKind::Blank || l.indent > line.indent {
                end += 1;
            } else {
                break;
            }
        }
        while end > start && self.lines[end - 1].kind == LineKind::Blank {
            end -= 1;
        }
        let body = &self.lines[start..end];
        let dedent = body.iter().filter(|l| l.kind != LineKind::Blank).map(|l| l.indent).min();
        let Some(dedent) = dedent else {
            return Err(Diagnostic::new(
                self.loc(line),
                "expected an indented block of Luau after `lua:`",
                &line.raw,
            ));
        };
        let code = body
            .iter()
            .map(|l| l.raw.chars().skip(dedent).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        self.i = end;
        Ok(Code { code, loc: Loc { file: self.file.into(), line: line.no + 1, col: dedent + 1 } })
    }

    /// Statements indented under `header`.
    fn block(&mut self, header: &Line, what: &str) -> Result<Vec<Stmt>, Diagnostic> {
        let Some(first) = self.next_code().filter(|&j| self.lines[j].indent > header.indent) else {
            return Err(Diagnostic::new(
                self.loc(header),
                format!("expected an indented block after `{what}`"),
                &header.raw,
            ));
        };
        let indent = self.lines[first].indent;
        let mut body = Vec::new();
        while let Some(j) = self.next_code() {
            let line = self.lines[j].clone();
            if line.indent <= header.indent {
                break;
            }
            if line.indent != indent {
                let msg = if line.indent > indent {
                    "unexpected indentation (deeper than the lines above)"
                } else {
                    "this line does not line up with the block it belongs to"
                };
                self.errors.push(Diagnostic::new(self.loc(&line), msg, &line.raw));
                self.skip_block(j);
                continue;
            }
            self.i = j + 1;
            match self.statement(&line) {
                Ok(s) => body.push(s),
                Err(e) => {
                    self.errors.push(e);
                    self.skip_block(j);
                }
            }
        }
        Ok(body)
    }

    fn statement(&mut self, line: &Line) -> Result<Stmt, Diagnostic> {
        let mut c = Cursor::new(self.file, line);
        let (loc, source) = (self.loc(line), line.text.clone());
        let stmt = move |kind| Stmt { kind, loc: loc.clone(), source: source.clone() };
        let ensure_end = |c: &mut Cursor, after: &str| {
            if c.at_end() {
                Ok(())
            } else {
                Err(c.error(format!("unexpected `{}` after {after}", c.rest_str().trim())))
            }
        };

        if c.eat('$') {
            let loc = {
                c.skip_ws();
                c.loc()
            };
            let code = c.rest();
            if code.is_empty() {
                return Err(c.error("expected Luau code after `$`"));
            }
            return Ok(stmt(StmtKind::Lua(Code { code, loc })));
        }
        if c.keyword("lua") {
            let code = self.lua_block(line, c)?;
            return Ok(stmt(StmtKind::Lua(code)));
        }
        for kw in ["jump", "call"] {
            if c.keyword(kw) {
                let target_loc = {
                    c.skip_ws();
                    c.loc()
                };
                let target =
                    c.ident(true).ok_or_else(|| c.error(format!("expected a label name after `{kw}`")))?;
                ensure_end(&mut c, &format!("`{kw} {target}`"))?;
                return Ok(stmt(if kw == "jump" {
                    StmtKind::Jump { target, target_loc }
                } else {
                    StmtKind::Call { target, target_loc }
                }));
            }
        }
        if c.keyword("return") {
            ensure_end(&mut c, "`return`")?;
            return Ok(stmt(StmtKind::Return));
        }
        if c.keyword("pass") {
            ensure_end(&mut c, "`pass`")?;
            return Ok(stmt(StmtKind::Pass));
        }
        if c.keyword("menu") {
            if c.until_block_colon().as_deref() != Some("") {
                return Err(c.error("expected `menu:` followed by an indented list of choices"));
            }
            return self.menu(line).map(stmt);
        }
        if c.keyword("if") {
            return self.if_chain(line, c).map(stmt);
        }
        if c.keyword("elif") || c.keyword("else") {
            return Err(Diagnostic::new(
                self.loc(line),
                "`elif`/`else` without a matching `if` above it",
                &line.raw,
            ));
        }
        if c.keyword("scene") {
            let image = self.idents(&mut c);
            let with = self.with_clause(&mut c)?;
            ensure_end(&mut c, "the scene")?;
            return Ok(stmt(StmtKind::Scene { image, with }));
        }
        if c.keyword("show") {
            let image = self.idents(&mut c);
            if image.is_empty() {
                return Err(c.error("expected an image after `show`, like `show mila smile`"));
            }
            let at = if c.keyword("at") {
                Some(
                    c.ident(false)
                        .ok_or_else(|| c.error("expected a position after `at`, like `at left`"))?,
                )
            } else {
                None
            };
            let with = self.with_clause(&mut c)?;
            ensure_end(&mut c, "the image")?;
            return Ok(stmt(StmtKind::Show { image, at, with }));
        }
        if c.keyword("hide") {
            let tag = c.ident(false).ok_or_else(|| c.error("expected an image tag after `hide`"))?;
            let with = self.with_clause(&mut c)?;
            ensure_end(&mut c, &format!("`hide {tag}`"))?;
            return Ok(stmt(StmtKind::Hide { tag, with }));
        }
        if c.keyword("with") {
            let t = c
                .ident(false)
                .ok_or_else(|| c.error("expected a transition after `with`, like `with fade`"))?;
            ensure_end(&mut c, &format!("`with {t}`"))?;
            return Ok(stmt(StmtKind::With(t)));
        }
        if c.keyword("play") {
            let channel = c
                .ident(false)
                .ok_or_else(|| c.error("expected a channel, like `play music \"theme.ogg\"`"))?;
            let file = c
                .string()?
                .ok_or_else(|| c.error(format!("expected a quoted file name after `play {channel}`")))?;
            let (mut fade, mut looping) = (None, None);
            loop {
                if c.keyword("fadein") {
                    fade = Some(c.number().ok_or_else(|| c.error("expected seconds after `fadein`"))?);
                } else if c.keyword("loop") {
                    looping = Some(true);
                } else if c.keyword("noloop") {
                    looping = Some(false);
                } else {
                    break;
                }
            }
            ensure_end(&mut c, "the file name (options are `fadein <secs>`, `loop`, `noloop`)")?;
            return Ok(stmt(StmtKind::Play { channel, file, fade, looping }));
        }
        if c.keyword("stop") {
            let channel = c
                .ident(false)
                .ok_or_else(|| c.error("expected a channel after `stop`, like `stop music`"))?;
            let fade = if c.keyword("fadeout") {
                Some(c.number().ok_or_else(|| c.error("expected seconds after `fadeout`"))?)
            } else {
                None
            };
            ensure_end(&mut c, &format!("`stop {channel}`"))?;
            return Ok(stmt(StmtKind::Stop { channel, fade }));
        }
        if c.keyword("pause") {
            let secs = if c.at_end() {
                None
            } else {
                Some(c.number().ok_or_else(|| c.error("expected seconds after `pause`"))?)
            };
            ensure_end(&mut c, "`pause`")?;
            return Ok(stmt(StmtKind::Pause(secs)));
        }
        if c.keyword("label") || c.keyword("define") || c.keyword("default") {
            return Err(Diagnostic::new(
                self.loc(line),
                "this belongs at the top level of the file, not inside a label",
                &line.raw,
            ));
        }
        let say = self.say(&mut c)?;
        ensure_end(&mut c, "the dialogue")?;
        Ok(stmt(StmtKind::Say(say)))
    }

    /// `"text"` or `who "text"`.
    fn say(&self, c: &mut Cursor) -> Result<Say, Diagnostic> {
        c.skip_ws();
        let who_loc = c.loc();
        let who = c.ident(false).map(|w| (w, who_loc));
        c.skip_ws();
        let text_loc = c.loc();
        match c.string()? {
            Some(text) => Ok(Say { who, text, text_loc }),
            None if who.is_some() => {
                Err(c.error("expected the line in quotes after the character, like `mila \"Hello.\"`"))
            }
            None => Err(c.error("expected a statement or a line of dialogue in quotes")),
        }
    }

    fn idents(&self, c: &mut Cursor) -> Vec<String> {
        let mut out = Vec::new();
        loop {
            c.skip_ws();
            let save = c.rest_str();
            if save.starts_with("with ") || save == "with" || save.starts_with("at ") || save == "at" {
                break;
            }
            match c.ident(false) {
                Some(id) => out.push(id),
                None => break,
            }
        }
        out
    }

    fn with_clause(&self, c: &mut Cursor) -> Result<Option<String>, Diagnostic> {
        if c.keyword("with") {
            Ok(Some(c.ident(false).ok_or_else(|| c.error("expected a transition after `with`"))?))
        } else {
            Ok(None)
        }
    }

    fn menu(&mut self, header: &Line) -> Result<StmtKind, Diagnostic> {
        let Some(first) = self.next_code().filter(|&j| self.lines[j].indent > header.indent) else {
            return Err(Diagnostic::new(
                self.loc(header),
                "expected an indented list of choices after `menu:`",
                &header.raw,
            ));
        };
        let indent = self.lines[first].indent;
        let (mut caption, mut choices) = (None, Vec::new());
        while let Some(j) = self.next_code() {
            let line = self.lines[j].clone();
            if line.indent <= header.indent {
                break;
            }
            self.i = j + 1;
            if line.indent != indent {
                self.errors.push(Diagnostic::new(
                    self.loc(&line),
                    "this line does not line up with the other choices",
                    &line.raw,
                ));
                self.skip_block(j);
                continue;
            }
            match self.choice_or_caption(&line, caption.is_none() && choices.is_empty()) {
                Ok(Ok(choice)) => choices.push(choice),
                Ok(Err(say)) => caption = Some(say),
                Err(e) => {
                    self.errors.push(e);
                    self.skip_block(j);
                }
            }
        }
        if choices.is_empty() {
            return Err(Diagnostic::new(
                self.loc(header),
                "a menu needs at least one choice, like `\"Yes\":`",
                &header.raw,
            ));
        }
        Ok(StmtKind::Menu { caption, choices })
    }

    /// A `"text" [if cond]:` choice, or (first line only) a caption say line.
    fn choice_or_caption(
        &mut self,
        line: &Line,
        caption_allowed: bool,
    ) -> Result<Result<Choice, Say>, Diagnostic> {
        let mut c = Cursor::new(self.file, line);
        if !line.text.ends_with(':') {
            if caption_allowed {
                let say = self.say(&mut c)?;
                if c.at_end() {
                    return Ok(Err(say));
                }
            }
            return Err(c.error("menu choices look like `\"text\":` (optionally `\"text\" if condition:`)"));
        }
        c.skip_ws();
        let text_loc = c.loc();
        let text = c
            .string()?
            .ok_or_else(|| c.error("expected the choice text in quotes, like `\"Of course.\":`"))?;
        let guard = if c.keyword("if") {
            c.skip_ws();
            let loc = c.loc();
            let cond = c
                .until_block_colon()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| c.error("expected a condition after `if`"))?;
            Some((cond, loc))
        } else {
            if c.until_block_colon().as_deref() != Some("") {
                return Err(c.error("expected `:` after the choice text"));
            }
            None
        };
        let body = self.block(line, &format!("\"{text}\":"))?;
        Ok(Ok(Choice { text, text_loc, guard, body }))
    }

    fn if_chain(&mut self, header: &Line, mut c: Cursor) -> Result<StmtKind, Diagnostic> {
        let cond = |c: &mut Cursor, kw: &str| {
            c.skip_ws();
            let loc = c.loc();
            c.until_block_colon()
                .filter(|s| !s.is_empty())
                .map(|s| (s, loc))
                .ok_or_else(|| c.error(format!("expected `{kw} <condition>:`")))
        };
        let mut branches =
            vec![Branch { cond: Some(cond(&mut c, "if")?), body: self.block(header, "if …:")? }];
        while let Some(j) = self.next_code().filter(|&j| self.lines[j].indent == header.indent) {
            let line = self.lines[j].clone();
            let mut c = Cursor::new(self.file, &line);
            if c.keyword("elif") {
                self.i = j + 1;
                let cond = cond(&mut c, "elif")?;
                branches.push(Branch { cond: Some(cond), body: self.block(&line, "elif …:")? });
            } else if c.keyword("else") {
                self.i = j + 1;
                if c.until_block_colon().as_deref() != Some("") {
                    return Err(c.error("expected `else:`"));
                }
                branches.push(Branch { cond: None, body: self.block(&line, "else:")? });
                break;
            } else {
                break;
            }
        }
        Ok(StmtKind::If { branches })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"define mila = Character("Mila", color="#e8a0bf")
default trust = 0

# Cafe scene
label start:
    scene cafe_night with fade
    show mila smile at left

    mila "You actually came."
    "The rain kept going."

    menu:
        mila "So?"
        "Of course.":
            $ trust += 1
            jump cafe_talk
        "Got lost on the way." if trust < 2:
            call lost_route
    if trust > 0 and player:mood() == "good":
        play music "warm.ogg" fadein 2.0 loop
    elif trust == 0:
        stop music fadeout 1.5
    else:
        pause 0.5
    return

label cafe_talk:
    lua:
        local n = #inventory
        trust = trust + n

    hide mila with dissolve
    with fade
    pause
"##;

    #[test]
    fn parses_the_sample() {
        let f = parse("cafe.kulisa", SAMPLE).unwrap_or_else(|e| panic!("{}", crate::Diagnostics(e)));
        assert_eq!(f.defines[0].name, "mila");
        assert_eq!(f.defines[0].expr, "Character(\"Mila\", color=\"#e8a0bf\")");
        assert_eq!(f.defaults[0].name, "trust");
        assert_eq!(f.labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["start", "cafe_talk"]);

        let start = &f.labels[0].body;
        assert!(
            matches!(&start[0].kind, StmtKind::Scene { image, with } if image == &["cafe_night"] && with.as_deref() == Some("fade"))
        );
        assert!(
            matches!(&start[1].kind, StmtKind::Show { image, at, .. } if image == &["mila", "smile"] && at.as_deref() == Some("left"))
        );
        assert!(
            matches!(&start[2].kind, StmtKind::Say(Say { who: Some((w, _)), text, .. }) if w == "mila" && text == "You actually came.")
        );
        assert!(matches!(&start[3].kind, StmtKind::Say(Say { who: None, .. })));
        let StmtKind::Menu { caption, choices } = &start[4].kind else { panic!("menu") };
        assert_eq!(caption.as_ref().unwrap().text, "So?");
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[1].guard.as_ref().unwrap().0, "trust < 2");
        assert!(matches!(&choices[0].body[0].kind, StmtKind::Lua(c) if c.code == "trust += 1"));
        let StmtKind::If { branches } = &start[5].kind else { panic!("if") };
        assert_eq!(branches.len(), 3);
        assert_eq!(branches[0].cond.as_ref().unwrap().0, "trust > 0 and player:mood() == \"good\"");
        assert!(matches!(
            &branches[0].body[0].kind,
            StmtKind::Play { fade: Some(2.0), looping: Some(true), .. }
        ));
        assert!(branches[2].cond.is_none());
        assert!(matches!(start[6].kind, StmtKind::Return));

        let talk = &f.labels[1].body;
        let StmtKind::Lua(code) = &talk[0].kind else { panic!("lua") };
        assert_eq!(code.code, "local n = #inventory\ntrust = trust + n");
        assert_eq!(code.loc.line, 29, "first line of the block body");
        assert!(matches!(talk[3].kind, StmtKind::Pause(None)));
    }

    fn errors(src: &str) -> Vec<String> {
        parse("e.kulisa", src)
            .unwrap_err()
            .iter()
            .map(|d| format!("{}:{} {}", d.loc.line, d.loc.col, d.message))
            .collect()
    }

    #[test]
    fn reports_every_error_with_position() {
        let errs = errors(
            "label start:\n    jump\n    mila Hello\n    \"ok\"\n      \"too deep\"\n    show\n\nlabel b\n",
        );
        assert_eq!(errs.len(), 5, "{errs:#?}");
        assert!(errs[0].starts_with("2:9 expected a label name"), "{}", errs[0]);
        assert!(errs[1].starts_with("3:10 expected the line in quotes"), "{}", errs[1]);
        assert!(errs[2].starts_with("5:7 unexpected indentation"), "{}", errs[2]);
        assert!(errs[3].starts_with("6:9 expected an image after `show`"), "{}", errs[3]);
        assert!(errs[4].starts_with("8:8 expected `:` after the label name"), "{}", errs[4]);
    }

    #[test]
    fn structural_errors() {
        assert!(errors("mila \"hi\"")[0].contains("go inside a label"));
        assert!(errors("label a:\n")[0].contains("expected an indented block"));
        assert!(errors("label a:\n    else:\n        pause")[0].contains("without a matching `if`"));
        assert!(errors("label a:\n    menu:\n        \"x\"\n")[0].contains("at least one choice"));
        assert!(
            errors("label a:\n    menu:\n        \"Yes\":\n            pause\n        \"x\"\n")[0]
                .contains("menu choices look like")
        );
        assert!(errors("define show = 1")[0].contains("statement keyword"));
        assert!(errors("label a:\n    play music theme.ogg")[0].contains("quoted file name"));
        assert!(errors("label a:\n    label b:\n        pause")[0].contains("top level"));
    }
}
