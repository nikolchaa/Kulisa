//! The `kulisa` Luau API on a headless renderer.
//!
//! Without a GPU (or software adapter) the tests skip, unless `KULISA_REQUIRE_GPU` is set.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use kulisa_lua::Game;
use kulisa_platform::Assets;
use kulisa_render::Renderer;

const FONT: &[u8] = include_bytes!("../../kulisa-runtime/assets/fonts/Inter-Regular.ttf");

/// Writes `files` into a fresh game folder.
fn folder(files: &[(&str, &[u8])]) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kulisa-game-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, bytes) in files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    dir
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

fn start(files: &[(&str, &[u8])]) -> Option<Result<Game, String>> {
    let r = renderer()?;
    Some(Game::new(Assets::dir(folder(files)), r))
}

macro_rules! game {
    ($($path:literal => $src:expr),* $(,)?) => {
        match start(&[$(($path, $src.as_ref())),*]) {
            Some(g) => g.unwrap_or_else(|e| panic!("{e}")),
            None => return,
        }
    };
}

#[test]
fn builds_scenes_from_luau() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        kulisa.screen.clear = "#000000"
        local root = kulisa.group{ x = 10 }
        box = kulisa.sprite{ parent = root, width = 20, height = 10, tint = "#ff0000", center = true }
        box.y = 5
    "##);
    let v: (f32, f32, f32, f32, String, String) =
        g.eval("t", "return box.x, box.y, box.origin_x, box.width, box.kind, box.tint").unwrap();
    assert_eq!(v, (0.0, 5.0, 10.0, 20.0, "sprite".into(), "#ff0000ff".into()));
    assert!(
        g.eval::<bool>("t", "return box.parent.x == 10 and box:contains(10, 5) and not box:contains(30, 5)")
            .unwrap()
    );

    let err = g.eval::<()>("t", "box.colour = 1").unwrap_err();
    assert!(err.contains("nodes have no property `colour`"), "{err}");
    let err = g.eval::<()>("t", "box.blend = 'burn'").unwrap_err();
    assert!(err.contains("one of: alpha, add, multiply, screen"), "{err}");

    // Centered on the design origin of a 64x64 screen: (10, 5) is near the top left.
    g.eval::<()>("t", "box.x = 32 - 10; box.y = 32").unwrap();
    g.frame(0.016);
    g.render();
    let img = g.renderer().read_pixels().unwrap();
    let px = |x: u32, y: u32| img.pixels[((y * img.width + x) * 4) as usize];
    assert_eq!(px(32, 32), 255, "red sprite in the middle");
    assert_eq!(px(2, 2), 0, "black elsewhere");

    g.eval::<()>("t", "box:remove()").unwrap();
    let err = g.eval::<()>("t", "return box.x").unwrap_err();
    assert!(err.contains("this node was removed"), "{err}");
}

#[test]
fn tweens_and_tasks() {
    let mut g = game!("main.luau" => r#"
        dot = kulisa.sprite{ opacity = 0 }
        log = {}
        kulisa.spawn(function()
            local t = dot:tween({ x = 100, opacity = 1 }, { time = 1, ease = "out_cubic" })
            kulisa.await(t)
            table.insert(log, "tweened")
            kulisa.wait(0.5)
            table.insert(log, "waited")
        end)
    "#);
    for _ in 0..10 {
        g.frame(0.05);
    }
    let (x, done): (f32, bool) = g.eval("t", "return dot.x, #log == 0").unwrap();
    assert!(x > 50.0 && x < 100.0 && done, "half way through an out_cubic tween: {x}");
    for _ in 0..12 {
        g.frame(0.05);
    }
    assert_eq!(g.eval::<Vec<String>>("t", "return log").unwrap(), ["tweened"]);
    for _ in 0..12 {
        g.frame(0.05);
    }
    assert_eq!(g.eval::<Vec<String>>("t", "return log").unwrap(), ["tweened", "waited"]);
    assert_eq!(g.eval::<(f32, f32)>("t", "return dot.x, dot.opacity").unwrap(), (100.0, 1.0));

    // A new tween on the same property takes over; cancelled tweens report done.
    let (a, b): (bool, bool) = g
        .eval("t", "a = dot:tween({ x = 0 }, 1); b = dot:tween({ x = 50 }, 1); return a.done, b.done")
        .unwrap();
    assert!(a && !b);
    let err = g.eval::<()>("t", "dot:tween({ x = 1 }, { time = 1, ease = 'wobbly' })").unwrap_err();
    assert!(err.contains("unknown ease `wobbly`"), "{err}");
}

#[test]
fn text_nodes_lay_out_and_type() {
    let mut g = game!(
        "main.luau" => r#"
            kulisa.load_font("Inter.ttf")
            label = kulisa.text{ text = "Hello {b}world{/b}", font = "Inter", size = 20, cps = 10 }
        "#,
        "Inter.ttf" => FONT,
    );
    g.frame(0.1);
    let (w, h, complete): (f32, f32, bool) =
        g.eval("t", "return label.width, label.height, label.complete").unwrap();
    assert!(w > 50.0 && h > 15.0, "{w}x{h}");
    assert!(!complete);
    g.eval::<()>("t", "label:skip()").unwrap();
    assert!(g.eval::<bool>("t", "return label.complete").unwrap());
    g.eval::<()>("t", "label.text = 'Hi'").unwrap();
    assert!(g.eval::<f32>("t", "return label.width").unwrap() < w);
    let err = g.eval::<()>("t", "label.text = '{b}open'").unwrap_err();
    assert!(err.contains("text markup"), "{err}");
}

const STORY: &str = r#"
default trust = 0

label start:
    play music "theme.ogg"
    scene bg room
    "First."
    $ trust += 1
    menu:
        "Stay":
            $ trust += 10
        "Leave":
            $ trust += 0
    "Trust [trust]."
"#;

const STORY_MAIN: &str = r#"
    events = {}
    inventory = { coins = 1 }
    kulisa.state.register("inventory", {
        save = function() return { coins = inventory.coins } end,
        load = function(t) inventory.coins = t and t.coins or 0 end,
    })
    kulisa.on("say", function(line) table.insert(events, "say:" .. line.text) end)
    kulisa.on("menu", function(items) table.insert(events, "menu:" .. #items .. ":" .. items[1].text) end)
    kulisa.on("stage", function(e) table.insert(events, e.kind .. ":" .. table.concat(e.image, " ")) end)
    kulisa.on("restore", function(stage) table.insert(events, "restore:" .. table.concat(stage.scene, " ")) end)
    kulisa.story.load()
    kulisa.story.start()
"#;

fn events(g: &Game) -> Vec<String> {
    g.eval("t", "local e = events; events = {}; return e").unwrap()
}

#[test]
fn drives_stories() {
    let g = game!("main.luau" => STORY_MAIN, "story/script.kulisa" => STORY);
    assert_eq!(events(&g), ["scene:bg room", "say:First."]);
    assert!(g.eval::<bool>("t", "return kulisa.story.next()").unwrap());
    assert_eq!(events(&g), ["menu:2:Stay"]);
    assert!(!g.eval::<bool>("t", "return kulisa.story.next()").unwrap(), "menus need a choice");
    g.eval::<()>("t", "inventory.coins = 7; kulisa.story.choose(1)").unwrap();
    assert_eq!(events(&g), ["say:Trust 11."]);
    assert_eq!(g.eval::<i64>("t", "return kulisa.story.vars().trust").unwrap(), 11);

    assert!(g.eval::<bool>("t", "return kulisa.story.rollback()").unwrap());
    assert_eq!(events(&g), ["restore:bg room", "menu:2:Stay"]);
    assert_eq!(g.eval::<i64>("t", "return inventory.coins").unwrap(), 1, "registered state rolled back");
    assert!(g.eval::<bool>("t", "return kulisa.story.roll_forward()").unwrap());
    assert_eq!(events(&g), ["restore:bg room", "say:Trust 11."]);
    assert!(g.eval::<bool>("t", "kulisa.story.next(); return kulisa.story.ended()").unwrap());
}

#[test]
fn saves_and_loads() {
    let id = format!("kulisa_test_{}", std::process::id());
    let g = game!("main.luau" => STORY_MAIN, "story/script.kulisa" => STORY);
    let setup = format!("kulisa.save.setup{{ id = '{id}', version = 2 }}");
    g.eval::<()>("t", &setup).unwrap();
    g.eval::<()>("t", "kulisa.story.next(); kulisa.story.choose(2)").unwrap();
    g.eval::<()>("t", "kulisa.save.write('one', { data = { chapter = 3 } })").unwrap();
    g.eval::<()>("t", "kulisa.story.rollback(); kulisa.story.choose(1)").unwrap();
    events(&g);

    let (title, chapter): (String, i64) = g
        .eval("t", "local d = kulisa.save.load('one'); return kulisa.save.list()[1].title, d.chapter")
        .unwrap();
    assert_eq!((title.as_str(), chapter), ("Trust 1.", 3));
    assert_eq!(events(&g), ["restore:bg room", "say:Trust 1."]);

    // Version 3 of the game renames `trust` to `bond`.
    g.eval::<()>("t", &format!("kulisa.save.setup{{ id = '{id}', version = 3 }}")).unwrap();
    g.eval::<()>(
        "t",
        "kulisa.save.migrate(2, function(body)
            local vars = body.story.vars.vars
            vars.bond, vars.trust = vars.trust, nil
        end)",
    )
    .unwrap();
    g.eval::<()>("t", "kulisa.save.load('one')").unwrap();
    assert_eq!(g.eval::<i64>("t", "return kulisa.story.vars().bond").unwrap(), 1);

    g.eval::<()>("t", "kulisa.save.delete('one')").unwrap();
    assert!(!g.eval::<bool>("t", "return kulisa.save.exists('one')").unwrap());
    let err = g.eval::<()>("t", "kulisa.save.load('one')").unwrap_err();
    assert!(err.contains("no save in slot `one`"), "{err}");
    let _ = std::fs::remove_dir_all(kulisa_platform::data_dir(&id).unwrap());
}

#[test]
fn reports_script_errors() {
    let Some(r) = renderer() else { return };
    let err = Game::new(Assets::dir(folder(&[("main.luau", b"\n\nkulisa.sprite{ width = 'wide' }")])), r)
        .err()
        .unwrap();
    assert!(err.contains("main.luau:3") && err.contains("`width` must be a number"), "{err}");

    let mut g = game!("main.luau" => r#"
        errors = {}
        kulisa.on("error", function(e) table.insert(errors, e) end)
        kulisa.on("update", function() error("oops") end)
        kulisa.spawn(function() kulisa.wait(0); local x = nil; x.y = 1 end)
        local ok = pcall(function() kulisa.on("updte", print) end)
        assert(not ok)
        local ok2, e = pcall(function() kulisa.version = "2" end)
        assert(not ok2, "the kulisa table is read-only")
    "#);
    g.frame(0.016);
    let errors: Vec<String> = g.eval("t", "return errors").unwrap();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("oops")), "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("main.luau:5")), "{errors:?}");
}

#[test]
fn requires_modules() {
    let g = game!(
        "main.luau" => "util = require('lib/util'); again = require('lib/util')",
        "lib/util.luau" => "loads = (loads or 0) + 1; return { twice = function(x) return x * 2 end }",
        "lib/loop.luau" => "require('lib/loop')",
    );
    assert_eq!(g.eval::<(f64, i64)>("t", "return util.twice(21), loads").unwrap(), (42.0, 1));
    let err = g.eval::<()>("t", "require('lib/loop')").unwrap_err();
    assert!(err.contains("requires itself"), "{err}");
    let err = g.eval::<()>("t", "require('missing')").unwrap_err();
    assert!(err.contains("no asset `missing.luau`"), "{err}");
}
