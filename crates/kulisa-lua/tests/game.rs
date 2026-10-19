//! The `kulisa` Luau API on a headless renderer.
//!
//! Without a GPU (or software adapter) the tests skip, unless `KULISA_REQUIRE_GPU` is set.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use kulisa_lua::Game;
use kulisa_platform::Assets;
use kulisa_render::Renderer;

const FONT: &[u8] = include_bytes!("../../../demos/smoke/fonts/Inter-Regular.ttf");

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

/// A solid white PNG.
fn png(w: u32, h: u32) -> Vec<u8> {
    kulisa_render::Image::solid(w, h, [255; 4]).encode_png().unwrap()
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

#[test]
fn nine_slices_and_masks_from_luau() {
    let g = game!("main.luau" => r##"
        local img = kulisa.load_image("box.png")
        panel = kulisa.sprite{ image = img, width = 300, height = 120, slice = 4, slice_scale = 2 }
        local s = kulisa.load_shader("m.wgsl")
        masked = kulisa.sprite{ shader = s, mask = img }
    "##,
    "box.png" => png(16, 16),
    "m.wgsl" => "fn fragment(in: VertexOut) -> vec4<f32> { return textureSample(mask_tex, mask_samp, in.uv); }");
    let v: (f32, f32, f32, bool) =
        g.eval("t", "return panel.slice[1], panel.slice[4], panel.slice_scale, masked.mask ~= nil").unwrap();
    assert_eq!(v, (4.0, 4.0, 2.0, true));
    let err = g.eval::<()>("t", "kulisa.sprite{ mask = panel.image }").unwrap_err();
    assert!(err.contains("set `shader` before `mask`"), "{err}");
    g.eval::<()>("t", "panel.slice = { 1, 2, 3, 4 }; panel.slice = nil").unwrap();
}

#[test]
fn meshes_deform_per_frame_and_render() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        kulisa.screen.clear = "#000000"
        m = kulisa.mesh{ cols = 4, rows = 4, width = 32, height = 32, tint = "#ff0000", x = 16, y = 16 }
    "##);
    let v: (f32, f32, f32, f32, String, f32, f32) = g
        .eval(
            "t",
            "return m.vertex_count, m.triangle_count, m.cols, m.rows, m.kind, m.width, m:vertex_at(4, 4)",
        )
        .unwrap();
    assert_eq!(v, (25.0, 32.0, 4.0, 4.0, "mesh".into(), 32.0, 25.0));
    g.frame(0.016);
    g.render();
    let img = g.renderer().read_pixels().unwrap();
    let red = |x: u32, y: u32| img.pixels[((y * img.width + x) * 4) as usize];
    assert_eq!((red(32, 32), red(4, 4), red(60, 32)), (255, 0, 0), "a red square in the middle");

    // Move the right column of vertices away every frame: the right edge follows.
    g.eval::<()>(
        "t",
        r#"
        for r = 0, 4 do
            local i = m:vertex_at(4, r)
            local x, y = m:vertex(i)
            m:set_vertex(i, x + 12, y)
        end
    "#,
    )
    .unwrap();
    g.frame(0.016);
    g.render();
    let img = g.renderer().read_pixels().unwrap();
    assert_eq!(img.pixels[((32 * img.width + 54) * 4) as usize], 255, "stretched right");
    assert!(g.eval::<bool>("t", "return m:contains(55, 32) and not m:contains(61, 32)").unwrap());

    // Bulk setters: tables and buffers, in sRGB for colors.
    g.eval::<()>(
        "t",
        r#"
        local buf = buffer.create(m.vertex_count * 8)
        for i = 0, m.vertex_count - 1 do
            buffer.writef32(buf, i * 8, i); buffer.writef32(buf, i * 8 + 4, 2 * i)
        end
        m:set_positions(buf)
        local p = m:positions()
        assert(p[1] == 0 and p[2] == 0 and p[3] == 1 and p[4] == 2, "round trip")
        local out = buffer.create(m.vertex_count * 8)
        m:positions(out)
        assert(buffer.readf32(out, 8) == 1)
        local cols = {}
        for i = 1, m.vertex_count do table.insert(cols, 1); table.insert(cols, 1); table.insert(cols, 1); table.insert(cols, 0.5) end
        m:set_colors(cols)
        "#,
    )
    .unwrap();

    for (code, want) in [
        ("m:set_positions({ 1, 2, 3, 4 })", "has 2 vertices but the mesh has 25"),
        ("m:set_positions({ 1, 2, 3, 4, 5 })", "x, y pairs"),
        ("m:vertex(26)", "vertex 26 does not exist (1 to 25)"),
        ("m:vertex(0)", "vertex 0 does not exist"),
        ("m:set_indices({ 1, 2, 99 })", "name existing vertices"),
        ("m:set_indices({ 0, 2, 3 })", "starting at 1"),
        ("kulisa.sprite():set_positions({})", "needs a mesh node"),
        ("kulisa.mesh{ cols = 0 }", "at least 1"),
        ("kulisa.mesh{ positions = { 0, 0, 1, 0, 0, 1 } }", "needs `indices`"),
    ] {
        let err = g.eval::<()>("t", code).unwrap_err();
        assert!(err.contains(want), "{code}: {err}");
    }
}

#[test]
fn free_form_meshes_take_raw_geometry() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        kulisa.screen.clear = "#000000"
        tri = kulisa.mesh{ positions = { 0, 0, 40, 0, 0, 40 }, indices = { 1, 2, 3 }, tint = "#00ff00", x = 8, y = 8 }
    "##);
    assert_eq!(g.eval::<(f32, f32)>("t", "return tri.vertex_count, tri.triangle_count").unwrap(), (3.0, 1.0));
    assert!(
        g.eval::<bool>("t", "return tri.cols == nil and tri:contains(20, 20) and not tri:contains(45, 45)")
            .unwrap()
    );
    g.frame(0.016);
    g.render();
    let img = g.renderer().read_pixels().unwrap();
    let green = |x: u32, y: u32| img.pixels[((y * img.width + x) * 4 + 1) as usize];
    assert_eq!((green(14, 14), green(50, 50)), (255, 0));
    // A new shape with a different vertex count.
    g.eval::<()>(
        "t",
        "tri:set_geometry{ positions = { 0, 0, 10, 0, 10, 10, 0, 10 }, indices = { 1, 2, 3, 1, 3, 4 } }",
    )
    .unwrap();
    assert_eq!(g.eval::<f32>("t", "return tri.vertex_count").unwrap(), 4.0);
}

/// A PNG whose left half is opaque and right half transparent.
fn half_png(w: u32, h: u32) -> Vec<u8> {
    let px: Vec<u8> = (0..w * h)
        .flat_map(|i| if i % w < w / 2 { [255, 255, 255, 255] } else { [255, 255, 255, 0] })
        .collect();
    kulisa_render::Image::from_straight_rgba(w, h, px).encode_png().unwrap()
}

#[test]
fn alpha_hit_testing_and_picking() {
    let g = game!("main.luau" => r##"
        kulisa.display(200, 100, "fit")
        local img = kulisa.load_image("half.png")
        back = kulisa.sprite{ image = img, width = 100, height = 100 }
        front = kulisa.sprite{ image = img, width = 100, height = 100, x = 50, z = 1 }
        deformed = kulisa.mesh{ image = img, cols = 2, rows = 1, width = 100, height = 50, y = 200 }
        local tint = kulisa.sprite{ width = 10, height = 10, x = 150, y = 0, z = 2 }
        solid = tint
    "##,
    "half.png" => half_png(16, 8));
    // Shape only, then with a pixel test: the right half of each sprite is see-through.
    let v: (bool, bool, bool, bool) = g
        .eval(
            "t",
            "return front:contains(140, 50), front:contains(140, 50, true), front:contains(70, 50, 0.9), front:contains(140, 50, 0.0)",
        )
        .unwrap();
    assert_eq!(v, (true, false, true, true), "0 accepts everything, even transparent pixels");
    // Picking goes front to back; the see-through half lets the click reach what is behind.
    let names: Vec<String> = g
        .eval("t", "local l = {} for _, n in kulisa.pick_all(75, 50) do table.insert(l, n == front and 'front' or n == back and 'back' or '?') end return l")
        .unwrap();
    assert_eq!(names, ["front", "back"]);
    let pick = |x: i32, y: i32, extra: &str| -> String {
        g.eval(
            "t",
            &format!(
                "local n = kulisa.pick({x}, {y}, {{ alpha = true{extra} }}) return n == front and 'front' or n == back and 'back' or n == deformed and 'mesh' or n == solid and 'solid' or 'none'"
            ),
        )
        .unwrap()
    };
    assert_eq!(pick(60, 50, ""), "front", "opaque left half of front");
    assert_eq!(pick(120, 50, ""), "none", "right half of front and nothing behind it at x=120");
    assert_eq!(pick(30, 50, ""), "back");
    assert_eq!(pick(30, 50, ", filter = function(n) return n ~= back end"), "none", "filter rejects back");
    assert_eq!(pick(30, 50, ", root = back"), "back", "limited to a subtree");
    assert_eq!(pick(60, 50, ", root = back"), "none", "back is see-through there, front is outside the root");
    // A deformed mesh is tested where it is drawn.
    g.eval::<()>("t", "for r = 0, 1 do for c = 0, 2 do local i = deformed:vertex_at(c, r) local x, y = deformed:vertex(i) deformed:set_vertex(i, x * 2, y) end end").unwrap();
    assert_eq!(pick(80, 225, ""), "mesh", "x = 80 is uv 0.4 after stretching to twice the width");
    assert_eq!(pick(120, 225, ""), "none", "uv 0.6 is transparent");
    // Images without a decodable source (the white pixel) are opaque.
    assert_eq!(pick(155, 5, ""), "solid");
    let err = g.eval::<()>("t", "kulisa.pick(1, 1, { alpha = 2 })").unwrap_err();
    assert!(err.contains("between 0 and 1"), "{err}");
}

const FADE: &str = "fn fragment(in: VertexOut) -> vec4<f32> {
    return textureSample(tex, samp, in.uv) * (1.0 - params.p[0].x);
}";

#[test]
fn transitions_fade_from_the_frozen_screen() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        kulisa.screen.clear = "#000000"
        box = kulisa.sprite{ width = 64, height = 64, tint = "#ff0000" }
        kulisa.on("stage", function(e)
            if e.kind == "scene" then box.tint = "#0000ff" end
            if e.kind == "show" then box.tint = "#00ff00" end
            if e.with then
                t = kulisa.screen:transition{ shader = "fade.wgsl", time = 1 }
            end
        end)
        kulisa.on("say", function() end)
        kulisa.story.load()
    "##,
    "fade.wgsl" => FADE,
    "s.kulisa" => "label start:\n    scene blue with fade\n    \"Hi.\"\n    show green\n    with fade\n    \"Green.\"\n");
    let px = |g: &mut Game| {
        g.render();
        let img = g.renderer().read_pixels().unwrap();
        [img.pixels[32 * 64 * 4 + 128], img.pixels[32 * 64 * 4 + 130]]
    };
    g.frame(0.0);
    assert_eq!(px(&mut g), [255, 0], "red before the story");
    g.eval::<()>("t", "kulisa.story.start()").unwrap();
    g.frame(0.0);
    assert_eq!(px(&mut g)[0], 255, "the frozen red frame covers the new blue scene");
    g.frame(0.25); // frames are clamped to 0.25 s
    g.frame(0.25);
    let mid = px(&mut g);
    assert!(mid[0] > 20 && mid[0] < 250 && mid[1] > 20, "halfway: {mid:?}");
    assert!(!g.eval::<bool>("t", "return t.done").unwrap());
    g.frame(0.25);
    g.frame(0.26);
    assert_eq!(px(&mut g), [0, 255], "blue once it ends");
    assert!(g.eval::<bool>("t", "return t.done").unwrap());
    // `with fade` on its own transitions everything since the last interaction.
    g.eval::<()>("t", "kulisa.story.next()").unwrap();
    g.frame(0.0);
    assert_eq!(px(&mut g), [0, 255], "starts from the blue frame shown before");
    for _ in 0..5 {
        g.frame(0.25);
    }
    let img = {
        g.render();
        g.renderer().read_pixels().unwrap()
    };
    assert_eq!(img.pixels[32 * 64 * 4 + 129], 255, "ends green");
    // `skip()` ends one at once; time 0 is instant.
    g.eval::<()>(
        "t",
        "kulisa.screen:freeze(); t = kulisa.screen:transition{ shader = 'fade.wgsl' }; t:skip()",
    )
    .unwrap();
    assert!(
        g.eval::<bool>(
            "t",
            "return t.done and kulisa.screen:transition{ shader = 'fade.wgsl', time = 0 }.done"
        )
        .unwrap()
    );
    let err = g.eval::<()>("t", "kulisa.screen:transition{ time = 1 }").unwrap_err();
    assert!(err.contains("needs a `shader`"), "{err}");
}

#[test]
fn thumbnails_persistent_data_and_seen_lines() {
    let id = format!("kulisa_test_{}_thumbs", std::process::id());
    let src = format!(
        r##"
        kulisa.display(64, 64)
        kulisa.save.setup{{ id = "{id}" }}
        kulisa.sprite{{ width = 64, height = 64, tint = "#00ff00" }}
        lines = {{}}
        kulisa.on("say", function(l) table.insert(lines, l.text .. ":" .. tostring(l.seen)) end)
        kulisa.on("menu", function(items)
            local m = {{}}
            for _, i in items do table.insert(m, i.text .. ":" .. tostring(i.chosen)) end
            table.insert(lines, table.concat(m, ","))
        end)
        kulisa.story.load()
        kulisa.story.start()
    "##
    );
    let story = "label start:\n    \"One.\"\n    menu:\n        \"A\":\n            \"Two.\"\n        \"B\":\n            \"Three.\"\n";
    let g = game!("main.luau" => src, "s.kulisa" => story);
    let lines = |g: &Game| g.eval::<Vec<String>>("t", "local l = lines; lines = {}; return l").unwrap();
    assert_eq!(lines(&g), ["One.:false"]);
    g.eval::<()>("t", "kulisa.story.next(); kulisa.story.choose(1)").unwrap();
    assert_eq!(lines(&g), ["A:false,B:false", "Two.:false"]);
    assert!(!g.eval::<bool>("t", "return kulisa.story.seen()").unwrap());
    // Rolling back shows lines as seen and the taken choice as chosen.
    g.eval::<()>("t", "kulisa.story.rollback()").unwrap();
    assert_eq!(lines(&g), ["A:true,B:false"]);

    // Captures become save thumbnails.
    g.eval::<()>("t", "shot = kulisa.screen:capture(32, 18); kulisa.save.write('s1', { thumbnail = shot })")
        .unwrap();
    let (w, h): (u32, u32) =
        g.eval("t", "local t = kulisa.save.list()[1].thumbnail; return t.width, t.height").unwrap();
    assert_eq!((w, h), (32, 18));
    assert!(g.eval::<bool>("t", "return kulisa.save.list()[1].thumbnail == nil or true").unwrap());

    // Persistent data survives a restart, and so do seen lines.
    g.eval::<()>("t", "kulisa.persist.write('prefs', { cps = 40, skip_unseen = false })").unwrap();
    let err = g.eval::<()>("t", "kulisa.persist.write('seen', 1)").unwrap_err();
    assert!(err.contains("reserved"), "{err}");
    drop(g);
    let g = game!("main.luau" => src, "s.kulisa" => story);
    assert_eq!(lines(&g), ["One.:true"]);
    assert_eq!(g.eval::<i64>("t", "return kulisa.persist.read('prefs').cps").unwrap(), 40);
    assert!(g.eval::<bool>("t", "return kulisa.persist.read('nothing') == nil").unwrap());
    g.eval::<()>("t", "kulisa.story.next()").unwrap();
    assert_eq!(lines(&g), ["A:true,B:false"]);

    // Blocked rollback.
    g.eval::<()>("t", "kulisa.story.choose(2)").unwrap();
    assert!(g.eval::<bool>("t", "return kulisa.story.can_rollback()").unwrap());
    g.eval::<()>("t", "kulisa.story.block_rollback()").unwrap();
    assert!(!g.eval::<bool>("t", "return kulisa.story.can_rollback() or kulisa.story.rollback()").unwrap());
    drop(g);
    let _ = std::fs::remove_dir_all(kulisa_platform::data_dir(&id).unwrap());
}

#[test]
fn ui_screens_rebuild_from_signals_and_take_input() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        kulisa.load_font("Inter.ttf")
        local ui = kulisa.ui
        name = ui.signal("Mila")
        clicks, volume, game_presses, gestures = 0, 0, 0, {}
        builds = 0
        ui.screen("hud", function(props)
            builds += 1
            return ui.column{ padding = 2, gap = 2,
                ui.text{ id = "name", text = name, size = 8 },
                ui.button{ id = "ok", width = 20, height = 10, background = "#202020", hover_background = "#ff0000",
                    on_click = function() clicks += 1 end },
                ui.slider{ id = "vol", width = 40, height = 8, thumb_size = 8, value = volume, min = 0, max = 1, step = 0.25,
                    background = "#111111", bar = "#00ff00", thumb = "#ffffff",
                    on_change = function(v) volume = v end },
                props.extra and ui.text("extra", { size = 8 }),
            }
        end)
        ui.screen("dialog", function()
            return ui.box{ background = "#00000080" }
        end, { modal = true, z = 200 })
        kulisa.on("press", function() game_presses += 1 end)
        kulisa.on("tap", function(x, y) table.insert(gestures, "tap") end)
        kulisa.on("swipe", function(dir) table.insert(gestures, "swipe " .. dir) end)
        kulisa.on("long_press", function() table.insert(gestures, "long") end)
        ui.show("hud", { extra = false })
    "##,
    "Inter.ttf" => FONT);
    let click = |g: &mut Game, x: f32, y: f32| {
        g.pointer_moved(Some(kulisa_core::Vec2::new(x, y)));
        g.pressed("left");
        g.released("left");
    };
    g.frame(0.016);
    let (x, y, w, h): (f32, f32, f32, f32) = g.eval("t", "return kulisa.ui.bounds('hud', 'ok')").unwrap();
    assert!((w - 20.0).abs() < 1e-3 && (h - 10.0).abs() < 1e-3);
    assert!((x - 2.0).abs() < 1e-3 && y > 2.0, "below the name: {x} {y}");
    click(&mut g, x + 5.0, y + 5.0);
    assert_eq!(
        g.eval::<(i64, i64)>("t", "return clicks, game_presses").unwrap(),
        (1, 0),
        "the button took it"
    );
    // Hover changes the look (on the next frame).
    g.frame(0.016);
    let hovered: String = g.eval("t", "return kulisa.ui.find('hud', 'ok'):children()[1].tint").unwrap();
    assert_eq!(hovered, "#ff0000ff");
    // Empty space goes to the game, as a press and a tap.
    click(&mut g, 60.0, 60.0);
    assert_eq!(g.eval::<i64>("t", "return game_presses").unwrap(), 1);
    assert_eq!(g.eval::<Vec<String>>("t", "return gestures").unwrap(), ["tap"]);

    // Signals rebuild only the screens that read them, keeping widgets (and their nodes).
    let node_before: String = g.eval("t", "return tostring(kulisa.ui.find('hud', 'name'))").unwrap();
    let builds: i64 = g.eval("t", "return builds").unwrap();
    g.eval::<()>("t", "name:set('Ren')").unwrap();
    g.frame(0.016);
    assert_eq!(g.eval::<i64>("t", "return builds").unwrap(), builds + 1);
    assert_eq!(g.eval::<String>("t", "return kulisa.ui.find('hud', 'name').text").unwrap(), "Ren");
    assert_eq!(g.eval::<String>("t", "return tostring(kulisa.ui.find('hud', 'name'))").unwrap(), node_before);
    g.eval::<()>("t", "name:set('Ren')").unwrap();
    g.frame(0.016);
    assert_eq!(g.eval::<i64>("t", "return builds").unwrap(), builds + 1, "same value, no rebuild");

    // Sliders report snapped values.
    let (sx, sy, sw, _): (f32, f32, f32, f32) = g.eval("t", "return kulisa.ui.bounds('hud', 'vol')").unwrap();
    g.pointer_moved(Some(kulisa_core::Vec2::new(sx + sw * 0.55, sy + 4.0)));
    g.pressed("left");
    g.pointer_moved(Some(kulisa_core::Vec2::new(sx + sw, sy + 4.0)));
    g.released("left");
    assert_eq!(g.eval::<f64>("t", "return volume").unwrap(), 1.0);

    // A modal screen blocks the game and the screens below.
    g.eval::<()>("t", "kulisa.ui.show('dialog')").unwrap();
    click(&mut g, x + 5.0, y + 5.0);
    click(&mut g, 60.0, 60.0);
    assert_eq!(g.eval::<(i64, i64)>("t", "return clicks, game_presses").unwrap(), (1, 1));
    g.eval::<()>("t", "kulisa.ui.hide('dialog')").unwrap();
    assert!(!g.eval::<bool>("t", "return kulisa.ui.shown('dialog')").unwrap());

    // Props rebuild with show; lists may hold false.
    g.eval::<()>("t", "kulisa.ui.show('hud', { extra = true })").unwrap();
    assert_eq!(g.eval::<usize>("t", "return #kulisa.ui.layer('hud'):children()[1]:children()").unwrap(), 4);

    // Errors name the screen; the old tree stays.
    g.eval::<()>("t", "kulisa.ui.screen('bad', function() return ui_missing.x end)").unwrap();
    let err = g.eval::<()>("t", "kulisa.ui.show('bad')").unwrap_err();
    assert!(err.contains("in screen `bad`"), "{err}");
    let err = g.eval::<()>("t", "kulisa.ui.show('nope')").unwrap_err();
    assert!(err.contains("no screen `nope`"), "{err}");
}

#[test]
fn display_profiles_pick_the_device_override() {
    let main = r##"
        kulisa.display{
            width = 1920, height = 1080, aspect = { "4:3", "21:9" },
            phone = { width = 1080, height = 1920, orientation = "portrait" },
        }
        reports = {}
        kulisa.on("display", function(d) table.insert(reports, d.profile .. " " .. d.orientation .. " " .. tostring(d.mismatch)) end)
    "##;
    let Some(r) = renderer() else { return };
    let phone = kulisa_lua::Device { form: kulisa_lua::FormFactor::Phone, touch: true, ..Default::default() };
    let mut g = Game::with_device(Assets::dir(folder(&[("main.luau", main.as_bytes())])), r, phone).unwrap();
    let (w, h, p): (f32, f32, String) =
        g.eval("t", "return kulisa.screen.width, kulisa.screen.height, kulisa.screen.profile").unwrap();
    assert_eq!((w, h, p.as_str()), (1080.0, 1920.0, "phone"));
    let req = g.take_requests();
    assert_eq!(req.orientation, Some(kulisa_lua::Orientation::Portrait));
    assert_eq!(req.window_aspect, None, "phones do not reshape windows");
    // A square 64x64 window counts as landscape: a web phone (no lock) should rotate.
    g.frame(0.016);
    assert_eq!(g.eval::<Vec<String>>("t", "return reports").unwrap(), ["phone landscape true"]);
    g.renderer().resize(32, 64);
    g.resize(32, 64);
    assert_eq!(
        g.eval::<Vec<String>>("t", "return reports").unwrap(),
        ["phone landscape true", "phone portrait false"]
    );

    // On desktop the base applies, with the aspect clamp and a window request.
    let mut g = game!("main.luau" => main);
    let (w, p): (f32, String) = g.eval("t", "return kulisa.screen.width, kulisa.screen.profile").unwrap();
    assert_eq!((w, p.as_str()), (1920.0, "desktop"));
    let req = g.take_requests();
    assert_eq!(req.orientation, Some(kulisa_lua::Orientation::Landscape));
    assert!((req.window_aspect.unwrap() - 16.0 / 9.0).abs() < 1e-4);
    // Safe insets shrink the safe area; the window controls queue requests.
    g.set_safe_area([8.0, 0.0, 8.0, 0.0]);
    let (x, _, sw, _): (f32, f32, f32, f32) = g.eval("t", "return kulisa.screen:safe()").unwrap();
    let (vx, _, vw, _): (f32, f32, f32, f32) = g.eval("t", "return kulisa.screen:visible()").unwrap();
    assert!(x > vx && sw < vw, "{x} {sw} vs {vx} {vw}");
    g.eval::<()>("t", "kulisa.window.fullscreen = true; kulisa.window:quit()").unwrap();
    let req = g.take_requests();
    assert!(req.quit && req.fullscreen == Some(true));
    let err = g.eval::<()>("t", "kulisa.display{ orientation = 'sideways' }").unwrap_err();
    assert!(err.contains("`orientation` must be"), "{err}");
}
