//! The `kulisa` command. See `kulisa help`.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kulisa_cli::{Game, Options, Result, android, desktop, engine_root, ios, run, serve, web};

const HELP: &str = "kulisa: make and build Kulisa games

  kulisa new NAME [--template T] [--dir DIR]    start a game from a template (default classic-vn)
  kulisa templates                              list the templates
  kulisa import RENPY [--template T] [--dir DIR]
                                                a new game from a Ren'Py project's scripts and art
  kulisa run [GAME] [--release] [--profile P]   play it on this computer with live reload
             [--web] [--port N]                   in the browser, with live reload
             [--android] [--abi LIST]             on a connected Android device or emulator
  kulisa build TARGET [GAME] [--release] [--out DIR] [--abi LIST] [--sim]
                                                TARGET: windows, linux, macos (this computer's),
                                                web, android, ios (on a Mac)
  kulisa pack [GAME] [--out FILE]               pack the game into a .kpak
  kulisa serve [DIR] [--port N]                 serve a web build

GAME is a game folder (default: the current folder). Builds go to GAME/dist/TARGET.
The engine source is found where this tool was built, or at KULISA_ENGINE.";

/// Options that take a value.
const VALUED: &[&str] = &["--template", "--dir", "--port", "--abi", "--out", "--profile"];
const SWITCHES: &[&str] = &["--release", "--web", "--android", "--sim"];

struct Args {
    positional: Vec<String>,
    options: BTreeMap<String, String>,
}

impl Args {
    fn parse(args: &[String]) -> Result<Args> {
        let mut out = Args { positional: Vec::new(), options: BTreeMap::new() };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            if VALUED.contains(&a.as_str()) {
                let v = it.next().ok_or(format!("{a} needs a value"))?;
                out.options.insert(a.clone(), v.clone());
            } else if SWITCHES.contains(&a.as_str()) {
                out.options.insert(a.clone(), String::new());
            } else if a.starts_with("--") {
                return Err(format!("unknown option {a}; see `kulisa help`"));
            } else {
                out.positional.push(a.clone());
            }
        }
        Ok(out)
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }

    fn has(&self, name: &str) -> bool {
        self.options.contains_key(name)
    }

    /// The game folder: positional argument `i`, or the current folder.
    fn game(&self, i: usize) -> Result<Game> {
        let dir = self.positional.get(i).map_or_else(|| PathBuf::from("."), PathBuf::from);
        let dir = fs::canonicalize(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Game::open(&strip_verbatim(dir))
    }

    fn options(&self) -> Result<Options> {
        Ok(Options { engine: strip_verbatim(engine_root()?), release: self.has("--release") })
    }

    fn port(&self) -> Result<u16> {
        self.get("--port").map_or(Ok(8080), |p| p.parse().map_err(|_| format!("bad port {p}")))
    }

    fn abis(&self, default: &str) -> Vec<String> {
        self.get("--abi").unwrap_or(default).split(',').map(str::to_string).collect()
    }
}

/// Windows `canonicalize` gives `\\?\C:\...`, which some tools (cargo, aapt) refuse.
fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p,
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let (command, rest) = match args.split_first() {
        Some((c, rest)) => (c.as_str(), rest),
        None => ("help", &[][..]),
    };
    let result = Args::parse(rest).and_then(|a| match command {
        "new" => new(&a),
        "import" => import(&a),
        "templates" => templates().map(|list| println!("{}", list.join("\n"))),
        "run" => play(&a),
        "build" => build(&a),
        "pack" => pack(&a),
        "serve" => {
            let dir = a.positional.first().map_or_else(|| PathBuf::from("dist/web"), PathBuf::from);
            serve::serve(&dir, a.port()?, None)
        }
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            Ok(())
        }
        "version" | "--version" | "-V" => {
            println!("kulisa {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        other => Err(format!("unknown command `{other}`; see `kulisa help`")),
    });
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn templates() -> Result<Vec<String>> {
    let dir = engine_root()?.join("templates");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .filter(|e| e.path().join("main.luau").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}

/// An id from a game name: lowercase letters, digits and `_` (`Cat Tales!` -> `cat_tales`).
fn id_from(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            id.push(c);
        } else if !id.ends_with('_') && !id.is_empty() {
            id.push('_');
        }
    }
    let id = id.trim_end_matches('_').to_string();
    if id.is_empty() { "my_game".into() } else { id }
}

/// A TOML string.
fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Sets `[game] id` and `title` in a template's `kulisa.toml` source.
fn set_identity(toml: &str, id: &str, title: &str) -> String {
    set_values(toml, "game", &[("id", toml_str(id)), ("title", toml_str(title))])
}

/// Sets keys of `[section]` (values already in TOML form) in `kulisa.toml` source, keeping
/// everything else; missing keys go at the end of the section, a missing section at the end.
fn set_values(toml: &str, section: &str, values: &[(&str, String)]) -> String {
    let header = format!("[{section}]");
    let mut done = vec![false; values.len()];
    let mut out: Vec<String> = Vec::new();
    let flush = |out: &mut Vec<String>, done: &mut [bool]| {
        for ((key, value), done) in values.iter().zip(done.iter_mut()) {
            if !*done {
                out.push(format!("{key} = {value}"));
                *done = true;
            }
        }
    };
    let (mut inside, mut seen) = (false, false);
    for line in toml.lines() {
        let t = line.trim_start();
        if t.starts_with('[') {
            if inside {
                flush(&mut out, &mut done);
            }
            inside = t.starts_with(&header);
            seen |= inside;
            out.push(line.to_string());
            continue;
        }
        let key = t.split('=').next().unwrap_or("").trim();
        match values.iter().position(|(k, _)| *k == key).filter(|_| inside && t.contains('=')) {
            Some(i) => {
                out.push(format!("{key} = {}", values[i].1));
                done[i] = true;
            }
            None => out.push(line.to_string()),
        }
    }
    if !seen {
        out.extend([String::new(), header]);
    }
    if inside || !seen {
        flush(&mut out, &mut done);
    }
    out.join("\n") + "\n"
}

fn copy_dir(from: &Path, to: &Path) -> Result {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?.flatten() {
        let name = entry.file_name();
        if !kulisa_platform::is_game_file(&name.to_string_lossy()) {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// Copies `template` to `dest` (which must be new or empty) and names the game `title`.
fn create_game(template: &str, dest: &Path, title: &str) -> Result {
    let available = templates()?;
    if !available.iter().any(|t| t == template) {
        return Err(format!("no template `{template}`; templates: {}", available.join(", ")));
    }
    if dest.exists() && fs::read_dir(dest).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(format!("{} already exists and is not empty", dest.display()));
    }
    copy_dir(&engine_root()?.join("templates").join(template), dest)?;
    edit_config(dest, |toml| set_identity(toml, &id_from(title), title))
}

fn edit_config(game: &Path, f: impl FnOnce(&str) -> String) -> Result {
    let path = game.join(kulisa_platform::CONFIG_FILE);
    let toml = fs::read_to_string(&path).unwrap_or_default();
    fs::write(&path, f(&toml)).map_err(|e| format!("{}: {e}", path.display()))
}

fn new(a: &Args) -> Result {
    let name = a.positional.first().ok_or("kulisa new NAME [--template T]")?;
    let template = a.get("--template").unwrap_or("classic-vn");
    let dest = a.get("--dir").map_or_else(|| PathBuf::from(name), PathBuf::from);
    create_game(template, &dest, name)?;
    println!("Created {} from the {template} template.", dest.display());
    println!("Play it with live reload:\n  cd {}\n  kulisa run", dest.display());
    Ok(())
}

/// `.kulisa` files under `dir`, as (path relative to the game, source).
fn stories(game: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![game.join("story")];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "kulisa")
                && let (Ok(src), Ok(rel)) = (fs::read_to_string(&p), p.strip_prefix(game))
            {
                out.push((rel.to_string_lossy().replace('\\', "/"), src));
            }
        }
    }
    out.sort();
    out
}

fn import(a: &Args) -> Result {
    let src = a.positional.first().ok_or("kulisa import RENPY_PROJECT [--template T] [--dir DIR]")?;
    let src = strip_verbatim(fs::canonicalize(src).map_err(|e| format!("{src}: {e}"))?);
    let imported = kulisa_renpy::import(&src)?;
    let project =
        if src.file_name().is_some_and(|n| n == "game") { src.parent().unwrap_or(&src) } else { &src };
    let folder = project.file_name().map_or("renpy-game".into(), |n| n.to_string_lossy().into_owned());
    let title = imported.info.title.clone().unwrap_or_else(|| folder.clone());
    let template = a.get("--template").unwrap_or("classic-vn");
    let dest = a.get("--dir").map_or_else(|| PathBuf::from(format!("{folder}-kulisa")), PathBuf::from);
    create_game(template, &dest, &title)?;

    // The template's sample story goes; the Ren'Py one takes its place.
    for (path, _) in stories(&dest) {
        fs::remove_file(dest.join(&path)).map_err(|e| format!("{path}: {e}"))?;
    }
    for (path, source) in &imported.stories {
        let p = dest.join(path);
        fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&p, source).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let mut copied = 0;
    let mut missing = Vec::new();
    for (from, to) in &imported.copies {
        let p = dest.join(to);
        fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        match fs::copy(from, &p) {
            Ok(_) => copied += 1,
            Err(e) => missing.push(format!("{}: {e}", from.display())),
        }
    }
    let info = &imported.info;
    edit_config(&dest, |toml| {
        let mut toml = toml.to_string();
        if let Some(v) = &info.version {
            toml = set_values(&toml, "game", &[("version", toml_str(v))]);
        }
        if let Some((w, h)) = info.size {
            toml = set_values(&toml, "display", &[("width", w.to_string()), ("height", h.to_string())]);
        }
        toml
    })?;

    let sources = stories(&dest);
    let refs: Vec<(&str, &str)> = sources.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
    let compiled = kulisa_story::compile(&refs).map(|_| ()).map_err(|e| e.to_string());
    let report = import_report(&src, &imported, copied, &missing, &compiled);
    fs::write(dest.join("IMPORT.md"), report).map_err(|e| e.to_string())?;

    println!(
        "Imported {} into {}: {} story files, {copied} files copied, {} notes (see IMPORT.md).",
        src.display(),
        dest.display(),
        imported.stories.len(),
        imported.notes.len()
    );
    match compiled {
        Ok(()) => println!("The story compiles. Play it with:\n  cd {}\n  kulisa run", dest.display()),
        Err(e) => println!("The story does not compile yet (also in IMPORT.md):\n{e}"),
    }
    Ok(())
}

fn import_report(
    src: &Path,
    imported: &kulisa_renpy::Import,
    copied: usize,
    missing: &[String],
    compiled: &std::result::Result<(), String>,
) -> String {
    let mut r = format!(
        "# Imported from Ren'Py\n\nFrom `{}`: {} story files in `story/`, {copied} images and sounds copied.\n\n",
        src.display(),
        imported.stories.len()
    );
    r.push_str(match compiled {
        Ok(()) => "The story compiles.\n\n",
        Err(_) => "**The story does not compile yet:**\n\n",
    });
    if let Err(e) = compiled {
        r.push_str(&format!("```\n{e}\n```\n\n"));
    }
    r.push_str(
        "## What to check\n\n\
         - Lines the import could not convert are in the story as `# Ren'Py:` comments.\n\
         - Screens, transforms, styles and Python code are Ren'Py's own; Kulisa games do them in Luau \
         (see `ui/` and `game/` in this template, and docs/API.md).\n\
         - Sprites stand at the template's positions (`left`, `center`, `right`); transitions are the \
         template's (`fade`, `dissolve`, wipes).\n\
         - The template's sample images are still in `images/`; delete the ones the game does not use.\n\n",
    );
    if !missing.is_empty() {
        r.push_str("## Files that could not be copied\n\n");
        for m in missing {
            r.push_str(&format!("- {m}\n"));
        }
        r.push('\n');
    }
    if !imported.notes.is_empty() {
        r.push_str("## Notes\n");
        let mut file = None;
        for n in &imported.notes {
            if file != Some(&n.file) {
                r.push_str(&format!("\n### {}\n\n", n.file));
                file = Some(&n.file);
            }
            match n.line {
                0 => r.push_str(&format!("- {}\n", n.message)),
                l => r.push_str(&format!("- line {l}: {}\n", n.message)),
            }
        }
    }
    r
}

fn play(a: &Args) -> Result {
    let game = a.game(0)?;
    let opts = a.options()?;
    if a.has("--web") {
        let out = game.dir.join("dist/web-dev");
        web::build(&game, &opts, &out)?;
        return serve::serve(&out, a.port()?, Some(&game.dir));
    }
    if a.has("--android") {
        let apk = game.dir.join("dist/android").join(format!("{}.apk", game.config.id));
        let abis = a.abis("arm64-v8a,x86_64");
        let abis: Vec<&str> = abis.iter().map(String::as_str).collect();
        android::build(&game, &opts, &abis, &apk)?;
        return android::install_and_start(&game, &apk);
    }
    let mut cargo = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo.current_dir(&opts.engine).args(["run", "-p", "kulisa-player"]);
    if opts.release {
        cargo.arg("--release");
    }
    cargo.arg("--").arg("--dev").arg(&game.dir);
    if let Some(p) = a.get("--profile") {
        cargo.env("KULISA_PROFILE", p);
    }
    run(&mut cargo)
}

fn build(a: &Args) -> Result {
    let target = a.positional.first().ok_or("kulisa build TARGET [GAME]; see `kulisa help`")?.as_str();
    let game = a.game(1)?;
    let opts = a.options()?;
    let out = a.get("--out").map_or_else(|| game.dir.join("dist").join(target), PathBuf::from);
    match target {
        "windows" | "linux" | "macos" if target == desktop::host() => {
            desktop::build(&game, &opts, &out).map(|_| ())
        }
        "windows" | "linux" | "macos" => {
            Err(format!("{target} builds are made on {target} (this computer runs {})", desktop::host()))
        }
        "web" => web::build(&game, &opts, &out),
        "android" => {
            let abis = a.abis("arm64-v8a");
            let abis: Vec<&str> = abis.iter().map(String::as_str).collect();
            android::build(&game, &opts, &abis, &out.join(format!("{}.apk", game.config.id)))
        }
        "ios" if cfg!(target_os = "macos") => {
            ios::build(&game, &opts, a.has("--sim"), &out.join(format!("{}.app", game.file_name())))
        }
        "ios" => Err("iOS builds need a Mac with Xcode".into()),
        other => Err(format!("unknown target `{other}`: windows, linux, macos, web, android or ios")),
    }
}

fn pack(a: &Args) -> Result {
    let game = a.game(0)?;
    let out = a
        .get("--out")
        .map_or_else(|| game.dir.join("dist").join(format!("{}.kpak", game.config.id)), PathBuf::from);
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&out, game.pack()?).map_err(|e| format!("{}: {e}", out.display()))?;
    eprintln!("wrote {}", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_from_names() {
        assert_eq!(id_from("Cat Tales!"), "cat_tales");
        assert_eq!(id_from("  My  Game 2 "), "my_game_2");
        assert_eq!(id_from("???"), "my_game");
    }

    #[test]
    fn sets_the_identity() {
        let toml =
            "# hi\n[game]\nid = \"old\"\ntitle = \"Old\"\nversion = \"1.0.0\"\n\n[display]\nwidth = 1\n";
        let out = set_identity(toml, "cats", "Cats \"Two\"");
        assert_eq!(
            out,
            "# hi\n[game]\nid = \"cats\"\ntitle = \"Cats \\\"Two\\\"\"\nversion = \"1.0.0\"\n\n[display]\nwidth = 1\n"
        );
        let parsed = kulisa_platform::GameConfig::parse(Some(&out)).unwrap();
        assert_eq!((parsed.id.as_str(), parsed.title.as_str()), ("cats", "Cats \"Two\""));
        let out = set_identity("[display]\nwidth = 1\n", "cats", "Cats");
        assert!(out.ends_with("[game]\nid = \"cats\"\ntitle = \"Cats\"\n"), "{out}");
        let out = set_identity("[game]\nversion = \"2\"\n[display]\n", "cats", "Cats");
        assert_eq!(out, "[game]\nversion = \"2\"\nid = \"cats\"\ntitle = \"Cats\"\n[display]\n");
    }

    #[test]
    fn sets_values_in_sections() {
        let toml =
            "[game]\nid = \"a\"\n\n[display]\nwidth = 1920\nheight = 1080\n\n[display.phone]\nwidth = 1080\n";
        let out = set_values(toml, "display", &[("width", "1280".into()), ("height", "720".into())]);
        assert_eq!(
            out,
            "[game]\nid = \"a\"\n\n[display]\nwidth = 1280\nheight = 720\n\n[display.phone]\nwidth = 1080\n"
        );
        let out = set_values("[game]\nid = \"a\"\n", "display", &[("width", "1280".into())]);
        assert_eq!(out, "[game]\nid = \"a\"\n\n[display]\nwidth = 1280\n");
    }

    #[test]
    fn parses_arguments() {
        let args: Vec<String> = ["web", "games/cats", "--release", "--out", "x"].map(String::from).to_vec();
        let a = Args::parse(&args).unwrap();
        assert_eq!(a.positional, ["web", "games/cats"]);
        assert!(a.has("--release") && a.get("--out") == Some("x"));
        assert!(Args::parse(&["--relase".to_string()]).is_err());
        assert!(Args::parse(&["--out".to_string()]).is_err());
    }
}
