//! Game config, the error view and live reload, on a headless renderer.
//!
//! Without a GPU (or software adapter) the tests skip, unless `KULISA_REQUIRE_GPU` is set.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use kulisa_lua::{Device, Game};
use kulisa_platform::Assets;
use kulisa_render::Renderer;

/// Writes `files` into a fresh game folder.
fn folder(files: &[(&str, &[u8])]) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kulisa-dev-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, bytes) in files {
        write(&dir, path, bytes);
    }
    dir
}

fn write(dir: &Path, path: &str, bytes: &[u8]) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn renderer() -> Option<Renderer> {
    match pollster::block_on(Renderer::headless(64, 64)) {
        Ok(r) => Some(r),
        Err(e) if std::env::var_os("KULISA_REQUIRE_GPU").is_some() => panic!("no GPU adapter: {e}"),
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

fn dev_game(dir: &Path) -> Option<Game> {
    Some(Game::start(Assets::dir(dir), renderer()?, Device::default(), true))
}

fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    kulisa_render::Image::solid(w, h, rgba).encode_png().unwrap()
}

/// A storage id no other test (or run) uses.
fn unique_id(name: &str) -> String {
    format!("kulisa_test_{name}_{}", std::process::id())
}

#[test]
fn reads_the_game_config() {
    let id = unique_id("config");
    let toml = format!(
        "[game]\nid = \"{id}\"\ntitle = \"Cat Tales\"\nversion = \"2.1.0\"\n\n\
         [display]\nwidth = 1280\nheight = 720\nscale = \"fit\"\n\n[my_game]\nlives = 3\nnames = [\"a\", \"b\"]\n"
    );
    let dir = folder(&[("kulisa.toml", toml.as_bytes()), ("main.luau", b"kulisa.save.setup()")]);
    let Some(r) = renderer() else { return };
    let g = Game::new(Assets::dir(&dir), r).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(g.config().title, "Cat Tales");
    let (title, lives, name, w, mode): (String, i64, String, f32, String) = g
        .eval(
            "t",
            "local c = kulisa.config
             return c.game.title, c.my_game.lives, c.my_game.names[2], kulisa.screen.width, kulisa.screen.mode",
        )
        .unwrap();
    assert_eq!(
        (title.as_str(), lives, name.as_str(), w, mode.as_str()),
        ("Cat Tales", 3, "b", 1280.0, "fit")
    );
    assert!(g.eval::<()>("t", "kulisa.config.game.title = 'x'").is_err(), "the config is read-only");

    // `save.setup()` uses the config's id.
    g.eval::<()>("t", "kulisa.save.write('one')").unwrap();
    let saves = kulisa_platform::data_dir(&id).unwrap();
    assert!(saves.join("saves").exists(), "{}", saves.display());
    let _ = std::fs::remove_dir_all(saves);

    // A broken config is an error at startup, and the game still runs with the defaults.
    let dir = folder(&[("kulisa.toml", b"[game]\ntitle = Cats"), ("main.luau", b"started = true")]);
    let Some(r) = renderer() else { return };
    let err = Game::new(Assets::dir(&dir), r).err().unwrap();
    assert!(err.starts_with("kulisa.toml:2:9:"), "{err}");
    let g = dev_game(&dir).unwrap();
    assert!(g.eval::<bool>("t", "return started").unwrap());
    assert!(g.shown_errors()[0].contains("strings need quotes"));
}

#[test]
fn error_view_shows_what_handlers_do_not_take() {
    // A failing main.luau keeps the game (with the error on screen) instead of no game.
    let dir = folder(&[("main.luau", b"local x = nil\nx.y = 1")]);
    let Some(r) = renderer() else { return };
    let mut g = Game::start(Assets::dir(&dir), r, Device::default(), false);
    assert_eq!(g.shown_errors().len(), 1);
    assert!(g.shown_errors()[0].contains("main.luau:2"), "{:?}", g.shown_errors());
    g.frame(0.016);
    g.render();
    let px = g.renderer().read_pixels().unwrap();
    assert!(px.pixels[..3].iter().all(|&c| c < 60), "the backdrop is dark: {:?}", &px.pixels[..4]);
    g.key("escape", true);
    assert!(g.shown_errors().is_empty());

    // Errors the game handles stay off the view, unless this is a development run.
    let main = b"kulisa.on('error', function(e) handled = e end)
        kulisa.on('key', function(k) if k == 'x' then error('boom') end end)";
    let dir = folder(&[("main.luau", main)]);
    let Some(r) = renderer() else { return };
    let mut g = Game::start(Assets::dir(&dir), r, Device::default(), false);
    g.key("x", true);
    assert!(g.eval::<String>("t", "return handled").unwrap().contains("boom"));
    assert!(g.shown_errors().is_empty());

    let mut g = dev_game(&dir).unwrap();
    g.key("x", true);
    assert!(g.shown_errors()[0].contains("boom"));
    // A click hides the view and goes no further.
    g.eval::<()>("t", "kulisa.on('press', function() pressed = true end)").unwrap();
    g.pressed("left");
    g.released("left");
    assert!(g.shown_errors().is_empty());
    assert!(!g.eval::<bool>("t", "return pressed == true").unwrap());
    g.pressed("left");
    assert!(g.eval::<bool>("t", "return pressed").unwrap());

    // Shift+R asks for a reload in development runs.
    g.key("shift", true);
    g.key("r", true);
    assert!(g.take_reload_request());
    assert!(!g.take_reload_request());
}

const STORY_MAIN: &[u8] = br#"
said = {}
kulisa.on("say", function(l) table.insert(said, l.text) end)
kulisa.on("restore", function() table.insert(said, "restore") end)
kulisa.story.load()
kulisa.story.start()
"#;

#[test]
fn reloads_stories_in_place() {
    let dir = folder(&[
        ("main.luau", STORY_MAIN),
        ("story.kulisa", b"label start:\n    \"One.\"\n    \"Two.\"\n    \"Three.\"\n"),
    ]);
    let Some(mut g) = dev_game(&dir) else { return };
    g.eval::<()>("t", "kulisa.story.next()").unwrap();
    // A line above the current one changes and a new one appears; the story stays on "Two.".
    write(&dir, "story.kulisa", b"label start:\n    \"Uno.\"\n    \"Zero.\"\n    \"Two.\"\n    \"Three!\"\n");
    assert!(!g.reload(&["story.kulisa".into()]));
    g.eval::<()>("t", "kulisa.story.next()").unwrap();
    let said: Vec<String> = g.eval("t", "return said").unwrap();
    assert_eq!(said, ["One.", "Two.", "restore", "Two.", "Three!"]);

    // A broken story keeps the old one and shows the error.
    write(&dir, "story.kulisa", b"label start:\n    jump\n");
    assert!(!g.reload(&["story.kulisa".into()]));
    assert!(g.shown_errors()[0].contains("story.kulisa:2"), "{:?}", g.shown_errors());
}

#[test]
fn reloads_images_and_shaders_in_place() {
    let dir = folder(&[
        ("main.luau", b"img = kulisa.load_image('a.png'); sh = kulisa.load_shader('s.wgsl')"),
        ("a.png", &png(4, 4, [255, 0, 0, 255])),
        ("s.wgsl", b"fn fragment(in: VertexOut) -> vec4<f32> { return in.color; }"),
    ]);
    let Some(mut g) = dev_game(&dir) else { return };
    write(&dir, "a.png", &png(8, 2, [0, 255, 0, 255]));
    write(&dir, "s.wgsl", b"fn fragment(in: VertexOut) -> vec4<f32> { return vec4(1.0); }");
    assert!(!g.reload(&["a.png".into(), "s.wgsl".into()]));
    assert_eq!(g.eval::<(u32, u32)>("t", "return img.width, img.height").unwrap(), (8, 2));
    assert!(g.shown_errors().is_empty());

    write(&dir, "s.wgsl", b"fn fragment(in: VertexOut) -> vec4<f32> { return nope; }");
    assert!(!g.reload(&["s.wgsl".into()]));
    assert!(g.shown_errors()[0].contains("s.wgsl"), "{:?}", g.shown_errors());
    // Files the game never loaded change nothing; Luau needs a restart.
    assert!(!g.reload(&["other.png".into()]));
    assert!(g.reload(&["main.luau".into()]));
}

#[test]
fn restarts_keep_the_story_line() {
    let id = unique_id("restart");
    let main = format!(
        "{}\nkulisa.save.setup{{ id = '{id}' }}\nkulisa.on('reload', function(slot) reloaded = slot; if slot then kulisa.save.load(slot) end end)",
        std::str::from_utf8(STORY_MAIN).unwrap()
    );
    let dir = folder(&[
        ("main.luau", main.as_bytes()),
        ("story.kulisa", b"label start:\n    $ n = 1\n    \"One.\"\n    $ n = 2\n    \"Two.\"\n"),
    ]);
    let Some(mut g) = dev_game(&dir) else { return };
    g.eval::<()>("t", "kulisa.story.next()").unwrap();
    assert!(g.save_for_restart());
    drop(g);

    let mut g = dev_game(&dir).unwrap();
    g.restarted(true);
    let (slot, n, slots): (String, i64, usize) =
        g.eval("t", "return reloaded, kulisa.story.vars().n, #kulisa.save.list()").unwrap();
    assert_eq!((slot.as_str(), n, slots), ("_reload", 2, 0), "the reload slot is hidden from the list");
    let said: Vec<String> = g.eval("t", "return said").unwrap();
    assert_eq!(said, ["One.", "restore", "Two."]);
    let _ = std::fs::remove_dir_all(kulisa_platform::data_dir(&id).unwrap());
}

/// With `KULISA_SHOTS=<dir>`, saves the error view at 1280x720 as `<dir>/error_view.png`.
#[test]
fn error_view_shot() {
    let Some(shots) = std::env::var_os("KULISA_SHOTS") else { return };
    let main = b"kulisa.screen.clear = '#3a6ea5'\nlocal t = {}\nprint(t.a.b)";
    let dir = folder(&[("main.luau", main)]);
    let r = pollster::block_on(Renderer::headless(1280, 720)).unwrap();
    let mut g = Game::start(Assets::dir(&dir), r, Device::default(), true);
    g.show_error(
        "story/day1.kulisa:12:5: unknown character `bob` (define it with `define bob = Character(...)`)",
    );
    g.frame(0.016);
    g.render();
    let img = g.renderer().read_pixels().unwrap();
    std::fs::create_dir_all(&shots).unwrap();
    std::fs::write(Path::new(&shots).join("error_view.png"), img.encode_png().unwrap()).unwrap();
}
