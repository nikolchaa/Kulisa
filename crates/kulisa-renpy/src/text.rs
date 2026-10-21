//! Ren'Py say text as Kulisa markup. Shared tags (`b`, `i`, `color`, `w=`) stay, a few map
//! (`{w}`, `{p}`, `{size}`, `{cps=*n}`), and the rest are dropped with their closing tags,
//! keeping the text. `[interpolation]` stays, without Ren'Py's `!t` flags and `:formats`.

/// Converted text, and the tags that were changed or dropped (for the import notes).
pub(crate) fn say_text(s: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut notes = Vec::new();
    // Open tags: (Ren'Py name, the Kulisa name it became, if kept).
    let mut open: Vec<(String, Option<String>)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (c == '{' || c == '[') && chars.get(i + 1) == Some(&c) {
            out.push(c);
            out.push(c);
            i += 2;
            continue;
        }
        if c == '[' {
            let Some(end) = closing(&chars, i, '[', ']') else {
                out.push_str("[[");
                i += 1;
                continue;
            };
            let inner: String = chars[i + 1..end].iter().collect();
            // `[name!t]`, `[name!u]`, `[value:.2f]`: the expression only.
            let expr = inner.split(['!', ':']).next().unwrap_or("").trim();
            out.push('[');
            out.push_str(expr);
            out.push(']');
            i = end + 1;
            continue;
        }
        if c != '{' {
            out.push(c);
            i += 1;
            continue;
        }
        let Some(end) = chars[i..].iter().position(|c| *c == '}').map(|p| p + i) else {
            out.push_str("{{");
            i += 1;
            continue;
        };
        let tag: String = chars[i + 1..end].iter().collect();
        i = end + 1;
        if let Some(name) = tag.strip_prefix('/') {
            if let Some(at) = open.iter().rposition(|(n, _)| n == name)
                && let (_, Some(kept)) = open.remove(at)
            {
                out.push_str(&format!("{{/{kept}}}"));
            }
            continue;
        }
        let (name, value) = match tag.split_once('=') {
            Some((n, v)) => (n.trim().to_string(), Some(v.trim().to_string())),
            None => (tag.trim().to_string(), None),
        };
        let mut note = |what: String| {
            if !notes.contains(&what) {
                notes.push(what);
            }
        };
        let kept: Option<String> = match (name.as_str(), value.as_deref()) {
            ("b" | "i", None) => Some(name.clone()),
            ("color", Some(v)) => {
                out.push_str(&format!("{{color={v}}}"));
                open.push((name, Some("color".into())));
                continue;
            }
            ("w", v) => {
                if v.is_none() {
                    note("`{w}` (wait for a click) became a half-second pause".into());
                }
                out.push_str(&format!("{{w={}}}", v.unwrap_or("0.5")));
                continue;
            }
            ("p", v) => {
                note("`{p}` became a pause and a line break".into());
                out.push_str(&format!("{{w={}}}\n", v.unwrap_or("0.5")));
                continue;
            }
            ("size", Some(v)) => {
                let size = if let Some(scale) = v.strip_prefix('*') {
                    Some(scale.to_string())
                } else if v.starts_with(['+', '-']) {
                    None
                } else {
                    Some(format!("{v}px"))
                };
                match size {
                    Some(s) => {
                        out.push_str(&format!("{{size={s}}}"));
                        open.push((name, Some("size".into())));
                    }
                    None => {
                        note(format!("`{{size={v}}}` (relative sizes) was dropped"));
                        open.push((name, None));
                    }
                }
                continue;
            }
            ("cps", Some(v)) if v.starts_with('*') => {
                out.push_str(&format!("{{speed={}}}", &v[1..]));
                open.push((name, Some("speed".into())));
                continue;
            }
            ("nw" | "fast" | "done" | "image" | "space" | "vspace" | "alt" | "noalt", _) => {
                note(format!("`{{{name}}}` was dropped"));
                continue;
            }
            _ => {
                note(format!("`{{{name}}}` was dropped (its text stays)"));
                None
            }
        };
        if let Some(k) = &kept {
            out.push_str(&format!("{{{k}}}"));
        }
        open.push((name, kept));
    }
    (out, notes)
}

fn closing(chars: &[char], open: usize, o: char, c: char) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in chars.iter().enumerate().skip(open) {
        if *ch == o {
            depth += 1;
        } else if *ch == c {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_shared_tags_and_maps_the_rest() {
        let (t, notes) = say_text("{b}Hi{/b} [name!t], {color=#f00}red{/color}{w=1.0} {i}it{/i}");
        assert_eq!(t, "{b}Hi{/b} [name], {color=#f00}red{/color}{w=1.0} {i}it{/i}");
        assert!(notes.is_empty());

        let (t, notes) =
            say_text("Wait{w} for it{p}{u}under{/u} {size=+10}big{/size} {size=40}px{/size}{nw}");
        assert_eq!(t, "Wait{w=0.5} for it{w=0.5}\nunder big {size=40px}px{/size}");
        assert_eq!(notes.len(), 5, "{notes:?}");

        let (t, _) = say_text("{cps=*2}fast{/cps} {{literal}} [[not] [score:.1f]");
        assert_eq!(t, "{speed=2}fast{/speed} {{literal}} [[not] [score]");
    }
}
