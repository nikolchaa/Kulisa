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
fn helpers_called_from_story_lines_reach_the_variables() {
    let g = game!("main.luau" => r##"
        function give(item)
            local v = kulisa.story.vars()
            v.bag[item] = (v.bag[item] or 0) + 1
        end
        kulisa.story.load()
        kulisa.story.start()
    "##, "story/s.kulisa" => "default bag = {}
label start:
    $ give(\"key\")
    \"Got [bag.key].\"
");
    assert_eq!(g.eval::<i64>("t", "return kulisa.story.vars().bag.key").unwrap(), 1);
}

#[test]
fn effect_chains_take_scale_inputs_and_masks() {
    let g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        local s = kulisa.load_shader("fx.wgsl")
        kulisa.screen.effects = {
            { shader = s, scale = 0.5 },
            { shader = s, scale = 0.25, input = "source" },
            { shader = s, input = 1, mask = "source" },
        }
        kulisa.sprite{ width = 64, height = 64, tint = "#ff0000" }
    "##, "fx.wgsl" => "fn fragment(in: VertexOut) -> vec4<f32> { return textureSample(tex, samp, in.uv) + textureSample(mask_tex, mask_samp, in.uv) * 0.0; }");
    let (scale, input, mask): (f32, i64, String) =
        g.eval("t", "local e = kulisa.screen.effects; return e[2].scale, e[3].input, e[3].mask").unwrap();
    assert_eq!((scale, input, mask.as_str()), (0.25, 1, "source"));
    assert_eq!(g.eval::<String>("t", "return kulisa.screen.effects[2].input").unwrap(), "source");
    let err = g
        .eval::<()>("t", "kulisa.screen.effects = { { shader = kulisa.load_shader('fx.wgsl'), input = 1 } }")
        .unwrap_err();
    assert!(err.contains("must name an earlier effect"), "{err}");
    let err = g
        .eval::<()>("t", "kulisa.screen.effects = { { shader = kulisa.load_shader('fx.wgsl'), scale = 2 } }")
        .unwrap_err();
    assert!(err.contains("`scale`"), "{err}");
    let mut g = g;
    g.frame(0.016);
    g.render();
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

#[test]
fn mesh_data_reaches_shaders() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        kulisa.screen.clear = "#000000"
        local show = kulisa.load_shader("data.wgsl")
        -- A grid whose left column carries (0.25, 0.5, 1, 1) and right column (1, 0.5, 0.25, 1),
        -- drawn as the data itself (premultiplied linear: the display re-encodes it).
        grid = kulisa.mesh{ cols = 1, rows = 1, width = 32, height = 64, x = 0, shader = show,
            data = { 0.25, 0.5, 1, 1, 1, 0.5, 0.25, 1, 0.25, 0.5, 1, 1, 1, 0.5, 0.25, 1 } }
        free = kulisa.mesh{ positions = { 0, 0, 32, 0, 0, 64 }, indices = { 1, 2, 3 }, x = 32, shader = show,
            data = { 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1 } }
    "##,
    "data.wgsl" => "fn fragment(in: VertexOut) -> vec4<f32> { return vec4<f32>(in.data.rgb, 1.0); }");
    g.frame(0.016);
    g.render();
    let img = g.renderer().read_pixels().unwrap();
    let px = |x: u32, y: u32| {
        let i = ((y * img.width + x) * 4) as usize;
        (img.pixels[i], img.pixels[i + 1], img.pixels[i + 2])
    };
    // Interpolated across the grid: red rises and blue falls from the left column to the right.
    let (left, mid, right) = (px(1, 32), px(16, 32), px(30, 32));
    assert!(left.0 < left.2 && right.0 > right.2, "data differs across the mesh: {left:?} {right:?}");
    assert!(left.0 < mid.0 && mid.0 < right.0, "red rises smoothly: {left:?} {mid:?} {right:?}");
    assert!(left.1.abs_diff(right.1) <= 2, "the middle channel is constant");
    // A free-form mesh (a triangle) takes data too; its covered corner is pure green.
    assert_eq!(px(40, 5), (0, 255, 0));
    // Replace and read back: half precision keeps three decimals around 1.
    g.eval::<()>(
        "t",
        r#"
        local d = buffer.create(16 * 4)
        for i = 0, 15 do buffer.writef32(d, i * 4, 0.5) end
        grid:set_data(d)
        local out = grid:data()
        assert(#out == 16 and out[1] == 0.5, "data round trip")
        "#,
    )
    .unwrap();
    for (code, want) in [
        ("grid:set_data({ 1, 2, 3, 4 })", "has 1 vertices but the mesh has 4"),
        ("grid:set_data({ 1, 2, 3 })", "groups of four"),
        ("kulisa.mesh{ cols = 2, rows = 2, data = { 0, 0, 0, 0 } }", "has 1 vertices but the mesh has 9"),
    ] {
        let err = g.eval::<()>("t", code).unwrap_err();
        assert!(err.contains(want), "{code}: {err}");
    }
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

fn v2(x: f32, y: f32) -> kulisa_core::Vec2 {
    kulisa_core::Vec2::new(x, y)
}

/// Presses pointer `id` at `from`, drags through `to` (several steps) and releases.
fn drag(g: &mut Game, id: u32, from: (f32, f32), to: (f32, f32)) {
    let button = if id == 0 { "left" } else { "touch" };
    g.pointer_moved_id(id, Some(v2(from.0, from.1)));
    g.pressed_id(id, button);
    for i in 1..=4 {
        let t = i as f32 / 4.0;
        g.pointer_moved_id(id, Some(v2(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)));
    }
    g.released_id(id, button);
}

fn settle(g: &mut Game) {
    for _ in 0..30 {
        g.frame(0.016);
    }
}

#[test]
fn drag_and_drop_with_zones() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        log = {}
        local function note(what) return function(i) table.insert(log, what .. ":" .. (i.zone and "zone" or "-")) end end
        coin = kulisa.sprite{ width = 8, height = 8, x = 4, y = 4 }
        gem = kulisa.sprite{ width = 8, height = 8, x = 20, y = 4 }
        slot = kulisa.sprite{ width = 16, height = 16, x = 40, y = 40, tint = "#333333" }
        cd = kulisa.drag(coin, { tag = "coin", on_start = note("start"), on_drop = note("drop"), on_reject = note("reject"), on_end = note("end"), on_click = note("click") })
        gd = kulisa.drag(gem, { tag = "gem", on_reject = note("gem_reject"), on_drop = note("gem_drop") })
        zone = kulisa.drop_zone(slot, { accept = "coin", capacity = 1, on_enter = note("enter"), on_leave = note("leave"), on_drop = note("zone_drop") })
    "##);
    let pos = |g: &Game, n: &str| -> (f32, f32) { g.eval("t", &format!("return {n}.x, {n}.y")).unwrap() };

    // Dropped on its zone: it snaps so its center sits on the zone's center (48, 48).
    drag(&mut g, 0, (8.0, 8.0), (44.0, 44.0));
    settle(&mut g);
    assert_eq!(pos(&g, "coin"), (44.0, 44.0));
    let log: Vec<String> = g.eval("t", "return log").unwrap();
    assert_eq!(
        log,
        ["start:-", "enter:zone", "leave:zone", "drop:zone", "zone_drop:zone", "end:zone"].map(String::from)
    );
    assert_eq!(g.eval::<f32>("t", "return #zone.occupants").unwrap(), 1.0);

    // The wrong tag is refused and returns home; the full zone refuses too.
    g.eval::<()>("t", "log = {}").unwrap();
    drag(&mut g, 0, (24.0, 8.0), (48.0, 48.0));
    settle(&mut g);
    assert_eq!(pos(&g, "gem"), (20.0, 4.0), "back where it started");
    assert_eq!(
        g.eval::<Vec<String>>("t", "return log").unwrap(),
        ["enter:zone", "leave:zone", "gem_reject:zone"],
        "zones report hovering whether or not they would accept"
    );

    // Picking the coin up again frees the slot; an unaccepted drop elsewhere stays put.
    drag(&mut g, 0, (48.0, 48.0), (10.0, 30.0));
    settle(&mut g);
    assert_eq!(g.eval::<f32>("t", "return #zone.occupants").unwrap(), 0.0);
    assert_eq!(pos(&g, "coin"), (6.0, 26.0), "dropped in the open: stays where released (offset kept)");

    // A press and release without moving is a click, not a drop.
    g.eval::<()>("t", "log = {}").unwrap();
    g.pointer_moved_id(0, Some(v2(8.0, 30.0)));
    g.pressed_id(0, "left");
    g.released_id(0, "left");
    assert_eq!(g.eval::<Vec<String>>("t", "return log").unwrap(), ["start:-", "click:-", "end:-"]);

    // Presses on draggables do not reach `press`, presses elsewhere do.
    g.eval::<()>("t", "presses = 0; kulisa.on('press', function() presses += 1 end)").unwrap();
    g.pointer_moved_id(0, Some(v2(8.0, 30.0)));
    g.pressed_id(0, "left");
    g.released_id(0, "left");
    g.pointer_moved_id(0, Some(v2(60.0, 10.0)));
    g.pressed_id(0, "left");
    g.released_id(0, "left");
    assert_eq!(g.eval::<f32>("t", "return presses").unwrap(), 1.0);

    // Disabled and removed draggables let go of their nodes.
    g.eval::<()>("t", "cd.enabled = false").unwrap();
    drag(&mut g, 0, (8.0, 30.0), (30.0, 30.0));
    assert_eq!(pos(&g, "coin"), (6.0, 26.0));
    g.eval::<()>("t", "cd.enabled = true; gd:remove()").unwrap();
    drag(&mut g, 0, (24.0, 8.0), (30.0, 12.0));
    assert_eq!(pos(&g, "gem"), (20.0, 4.0));
}

#[test]
fn drop_zone_accept_functions_bounds_and_axis() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        item = kulisa.sprite{ width = 8, height = 8, x = 4, y = 4 }
        box = kulisa.sprite{ width = 16, height = 16, x = 40, y = 4 }
        asked = 0
        kulisa.drop_zone(box, { accept = function(i) asked += 1 return i.node == item and asked > 1 end, snap = "none" })
        kulisa.drag(item, { bounds = { 0, 0, 30, 30 }, home = "never", axis = "y" })
    "##);
    let pos = |g: &Game| -> (f32, f32) { g.eval("t", "return item.x, item.y").unwrap() };
    // Vertical only and clamped: x stays, y stops at 30 although the pointer went to 50.
    drag(&mut g, 0, (8.0, 8.0), (46.0, 50.0));
    assert_eq!(pos(&g), (4.0, 30.0));
    // Over the zone with the pointer: the function is asked (and says no the first time).
    g.eval::<()>("t", "item.y = 4").unwrap();
    g.pointer_moved_id(0, Some(v2(8.0, 8.0)));
    g.pressed_id(0, "left");
    g.pointer_moved_id(0, Some(v2(46.0, 10.0)));
    g.pointer_moved_id(0, Some(v2(48.0, 10.0)));
    g.released_id(0, "left");
    assert_eq!(g.eval::<f32>("t", "return asked").unwrap(), 1.0);
    g.pointer_moved_id(0, Some(v2(8.0, 8.0)));
    g.pressed_id(0, "left");
    g.pointer_moved_id(0, Some(v2(46.0, 10.0)));
    g.pointer_moved_id(0, Some(v2(48.0, 10.0)));
    g.released_id(0, "left");
    assert_eq!(g.eval::<f32>("t", "return asked").unwrap(), 2.0);
    let err = g.eval::<()>("t", "kulisa.drag(item, { axis = 'diagonal' })").unwrap_err();
    assert!(err.contains("`axis` must be one of: both, x, y"), "{err}");
    let err = g
        .eval::<()>("t", "kulisa.drop_zone(item, { rect = {0,0,1,1}, polygon = {0,0,1,0,0,1} })")
        .unwrap_err();
    assert!(err.contains("at most one of"), "{err}");
}

#[test]
fn hotspots_hover_click_and_see_through_pixels() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        log = {}
        local function note(what) return function() table.insert(log, what) end end
        art = kulisa.sprite{ image = "half.png", width = 64, height = 32 }
        spot = kulisa.hotspot(art, { alpha = true, on_enter = note("enter"), on_leave = note("leave"), on_click = note("click"), on_press = note("press") })
        region = kulisa.group{ x = 0, y = 0 }
        gate = kulisa.hotspot(region, { rect = { 40, 8, 12, 12 }, priority = 5, on_click = note("gate") })
        presses = 0
        tri_log = {}
        tri = kulisa.hotspot(region, { polygon = { 0, 40, 16, 40, 8, 56 }, on_click = function() table.insert(tri_log, 'tri') end })
        kulisa.on("press", function() presses += 1 end)
    "##,
    "half.png" => half_png(16, 8));
    // Hover follows the mouse and respects transparent pixels (the right half).
    g.pointer_moved_id(0, Some(v2(10.0, 10.0)));
    assert!(g.eval::<bool>("t", "return spot.hovered").unwrap());
    g.pointer_moved_id(0, Some(v2(50.0, 25.0)));
    assert!(!g.eval::<bool>("t", "return spot.hovered").unwrap());
    // Click on the opaque half: a hotspot click that `press` never sees.
    g.pointer_moved_id(0, Some(v2(10.0, 10.0)));
    g.pressed_id(0, "left");
    g.released_id(0, "left");
    // Click on the see-through half: nothing is there, so the game gets `press`.
    g.pointer_moved_id(0, Some(v2(50.0, 25.0)));
    g.pressed_id(0, "left");
    g.released_id(0, "left");
    // The shaped hotspot sits at (40..52, 8..20) in the group's space; higher priority wins
    // even where `art` is also hit (nothing opaque there, but priority is checked anyway).
    g.pointer_moved_id(0, Some(v2(46.0, 14.0)));
    g.pressed_id(0, "left");
    g.released_id(0, "left");
    let log: Vec<String> = g.eval("t", "return log").unwrap();
    assert_eq!(log, ["enter", "leave", "enter", "press", "click", "leave", "gate"].map(String::from));
    assert_eq!(g.eval::<f32>("t", "return presses").unwrap(), 1.0, "only the see-through press");

    // A polygon hotspot (a triangle in the group's space): inside clicks, outside presses.
    for (x, y) in [(8.0, 44.0), (1.0, 55.0)] {
        g.pointer_moved_id(0, Some(v2(x, y)));
        g.pressed_id(0, "left");
        g.released_id(0, "left");
    }
    assert_eq!(g.eval::<Vec<String>>("t", "return tri_log").unwrap(), ["tri"], "inside once, outside once");
}

#[test]
fn multi_touch_reports_ids_pinch_and_rotate() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        got = {}
        kulisa.on("press", function(x, y, button, id) table.insert(got, "press" .. id .. button) end)
        kulisa.on("release", function(x, y, button, id) table.insert(got, "release" .. id) end)
        kulisa.on("pinch", function(f, x, y, total) pinch = { f, x, y, total } end)
        kulisa.on("rotate", function(d, x, y, total) rot = { d, total } end)
    "##);
    g.pointer_moved_id(1, Some(v2(19.0, 32.0)));
    g.pressed_id(1, "touch");
    g.pointer_moved_id(2, Some(v2(45.0, 32.0)));
    g.pressed_id(2, "touch");
    assert_eq!(g.eval::<f32>("t", "return #kulisa.input.pointers()").unwrap(), 2.0);
    // Spread the fingers apart to twice the distance, then turn them a quarter circle.
    g.pointer_moved_id(1, Some(v2(6.0, 32.0)));
    g.pointer_moved_id(2, Some(v2(58.0, 32.0)));
    let (factor, cx, total): (f32, f32, f32) = g.eval("t", "return pinch[1], pinch[2], pinch[4]").unwrap();
    assert!(factor > 1.0 && (cx - 32.0).abs() < 1e-3 && (total - 2.0).abs() < 1e-3, "{factor} {cx} {total}");
    g.pointer_moved_id(1, Some(v2(32.0, 6.0)));
    g.pointer_moved_id(2, Some(v2(32.0, 58.0)));
    let turned: f32 = g.eval("t", "return rot[2]").unwrap();
    assert!((turned.abs() - std::f32::consts::FRAC_PI_2).abs() < 0.05, "{turned}");
    g.released_id(2, "touch");
    g.released_id(1, "touch");
    assert_eq!(
        g.eval::<Vec<String>>("t", "return got").unwrap(),
        ["press1touch", "press2touch", "release2", "release1"].map(String::from)
    );
    assert_eq!(
        g.eval::<f32>("t", "return #kulisa.input.pointers()").unwrap(),
        0.0,
        "touches are forgotten once up"
    );
    g.pointer_moved(Some(v2(5.0, 5.0)));
    assert_eq!(g.eval::<f32>("t", "return #kulisa.input.pointers()").unwrap(), 1.0, "the mouse");
}

#[test]
fn timers_run_in_game_time() {
    let mut g = game!("main.luau" => r##"
        log = {}
        once = kulisa.after(0.5, function() table.insert(log, "once") end)
        tick = kulisa.every(0.2, function() table.insert(log, "tick") end)
        gone = kulisa.after(0.3, function() table.insert(log, "never") end)
        gone.cancel()
        kulisa.spawn(function() kulisa.await(once) table.insert(log, "awaited") end)
    "##);
    for _ in 0..30 {
        g.frame(0.05);
    }
    let log: Vec<String> = g.eval("t", "return log").unwrap();
    let ticks = log.iter().filter(|l| *l == "tick").count();
    assert!((6..=8).contains(&ticks), "{ticks} ticks in 1.5 s at 0.2 s: {log:?}");
    assert_eq!(log.iter().filter(|l| *l == "once").count(), 1);
    assert!(log.contains(&"awaited".to_string()) && !log.contains(&"never".to_string()), "{log:?}");
    g.eval::<()>("t", "tick.cancel()").unwrap();
    let before = log.len();
    for _ in 0..10 {
        g.frame(0.05);
    }
    assert_eq!(g.eval::<Vec<String>>("t", "return log").unwrap().len(), before);
    let err = g.eval::<()>("t", "kulisa.after(-1, function() end)").unwrap_err();
    assert!(err.contains("the delay must be a number of seconds"), "{err}");
}

const NAV_MENU: &str = r##"
    kulisa.display(64, 64, "fit")
    log = {}
    local ui = kulisa.ui
    local function note(what) return function() table.insert(log, what) end end
    ui.screen("menu", function()
        return ui.column{ gap = 4,
            ui.button{ id = "play", width = 40, height = 10, on_click = note("play"), background = "#222222", hover_background = "#ff0000" },
            ui.button{ id = "load", width = 40, height = 10, on_click = note("load"), background = "#222222", hover_background = "#ff0000" },
            ui.slider{ id = "vol", width = 40, height = 8, value = 0.5, step = 0.25, on_change = function(v) table.insert(log, "vol" .. v) end },
        }
    end, { nav = true, area = "screen" })
    ui.show("menu")
    kulisa.on("key", function(k) table.insert(log, "key:" .. k) end)
    kulisa.on("pad_press", function(b, pad) table.insert(log, "pad:" .. b) end)
"##;

#[test]
fn keyboard_and_gamepad_navigate_ui_screens() {
    let mut g = game!("main.luau" => NAV_MENU);
    g.frame(0.016);
    let focus =
        |g: &Game| -> Option<String> { g.eval("t", "return (select(2, kulisa.ui.focused()))").unwrap() };
    let log = |g: &Game| -> Vec<String> { g.eval("t", "return log").unwrap() };
    assert_eq!(focus(&g), None);

    // Arrows move focus and never reach `key`; letters still do.
    g.key("down", true);
    g.key("down", false);
    assert_eq!(focus(&g).as_deref(), Some("play"));
    g.key("down", true);
    g.key("down", false);
    assert_eq!(focus(&g).as_deref(), Some("load"));
    g.key("x", true);
    g.key("x", false);
    assert_eq!(log(&g), ["key:x"]);

    // Enter clicks the focused button, and it does not reach `key`.
    g.key("enter", true);
    g.key("enter", false);
    assert_eq!(log(&g), ["key:x", "load"]);

    // Down to the slider; left and right adjust it by its step instead of moving.
    g.key("down", true);
    g.key("down", false);
    assert_eq!(focus(&g).as_deref(), Some("vol"));
    g.key("right", true);
    g.key("right", false);
    g.key("left", true);
    g.key("left", false);
    g.key("left", true);
    g.key("left", false);
    assert_eq!(&log(&g)[2..], ["vol0.75", "vol0.5", "vol0.25"]);

    // The focused button is drawn hovered: red.
    g.key("up", true);
    g.key("up", false);
    g.frame(0.016);
    g.render();
    let img = g.renderer().read_pixels().unwrap();
    // Column of 10px buttons with a 4px gap, centered at the top left of the screen area.
    let at = |x: u32, y: u32| &img.pixels[((y * img.width + x) * 4) as usize..][..3];
    assert_eq!(at(2, 16), [255, 0, 0], "the focused (second) button looks hovered");
    assert_ne!(at(2, 2), [255, 0, 0]);

    // Moving the mouse takes the focus away; the next arrow starts over at the first widget.
    g.pointer_moved(Some(v2(60.0, 60.0)));
    assert_eq!(focus(&g), None);
    g.key("right", true);
    g.key("right", false);
    assert_eq!(focus(&g).as_deref(), Some("play"));
}

#[test]
fn gamepads_press_axes_repeat_and_navigate() {
    let mut g = game!("main.luau" => NAV_MENU);
    g.frame(0.016);
    let focus =
        |g: &Game| -> Option<String> { g.eval("t", "return (select(2, kulisa.ui.focused()))").unwrap() };
    let log = |g: &Game| -> Vec<String> { g.eval("t", "return log").unwrap() };

    g.gamepad_connected(7, Some("Test Pad"));
    assert_eq!(
        g.eval::<(f32, String)>("t", "local p = kulisa.input.pads()[1] return p.id, p.name").unwrap(),
        (7.0, "Test Pad".into())
    );
    // D-pad down focuses, A clicks; B is the game's.
    g.gamepad_button(7, "down", true);
    g.gamepad_button(7, "down", false);
    assert_eq!(focus(&g).as_deref(), Some("play"));
    g.gamepad_button(7, "a", true);
    g.gamepad_button(7, "a", false);
    g.gamepad_button(7, "b", true);
    assert!(g.eval::<bool>("t", "return kulisa.input.pad_down('b') and kulisa.input.pad_down('b', 7) and not kulisa.input.pad_down('a')").unwrap());
    g.gamepad_button(7, "b", false);
    assert_eq!(log(&g), ["play", "pad:b"]);

    // Holding the left stick down repeats after a delay.
    g.gamepad_axis(7, "ly", 0.9);
    assert_eq!(focus(&g).as_deref(), Some("load"));
    for _ in 0..60 {
        g.frame(0.016); // about one second: well past the delay
    }
    assert_eq!(focus(&g).as_deref(), Some("vol"), "repeats stop at the last widget");
    g.gamepad_axis(7, "ly", 0.0);
    assert_eq!(g.eval::<f32>("t", "return kulisa.input.pad_axis('ly')").unwrap(), 0.0);

    // Without a navigable screen, directions and A reach the game.
    g.eval::<()>("t", "kulisa.ui.hide('menu')").unwrap();
    g.frame(0.016);
    g.eval::<()>("t", "log = {}").unwrap();
    g.gamepad_button(7, "down", true);
    g.gamepad_button(7, "a", true);
    assert_eq!(log(&g), ["pad:down", "pad:a"]);

    // A screen that opens after the player used the d-pad starts focused.
    g.eval::<()>("t", "kulisa.ui.show('menu')").unwrap();
    g.frame(0.016);
    assert_eq!(focus(&g).as_deref(), Some("play"));

    let err = g.eval::<()>("t", "kulisa.input.pad_down('select')").unwrap_err();
    assert!(err.contains("unknown gamepad button `select`"), "{err}");
    g.gamepad_connected(7, None);
    assert_eq!(g.eval::<f32>("t", "return #kulisa.input.pads()").unwrap(), 0.0);
}

#[test]
fn text_fields_take_text_and_editing_keys() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        kulisa.load_font("Inter.ttf")
        local ui = kulisa.ui
        name = ui.signal("")
        submitted = {}
        keys = {}
        ui.screen("form", function()
            return ui.column{ gap = 2,
                ui.input{ id = "name", value = name, placeholder = "Your name", width = 60, height = 10, size = 6,
                    max_length = 8, on_submit = function(t) table.insert(submitted, t) end },
                ui.button{ id = "ok", width = 20, height = 10, on_click = function() end },
            }
        end, { nav = true, area = "screen" })
        ui.show("form")
        kulisa.on("key", function(k) table.insert(keys, k) end)
        texts = {}
        kulisa.on("text", function(s) table.insert(texts, s) end)
    "##,
    "Inter.ttf" => FONT);
    g.frame(0.016);
    let name = |g: &Game| -> String { g.eval("t", "return name:peek()").unwrap() };
    assert!(!g.wants_text());
    g.text_input("ignored");
    assert_eq!(g.eval::<f32>("t", "return #texts").unwrap(), 0.0, "text goes nowhere without a field");

    // Click the field: it starts taking text, and asks for the soft keyboard once.
    let (x, y, w, h): (f32, f32, f32, f32) = g.eval("t", "return kulisa.ui.bounds('form', 'name')").unwrap();
    g.pointer_moved(Some(v2(x + w / 2.0, y + h / 2.0)));
    g.pressed("left");
    g.released("left");
    assert!(g.wants_text());
    assert_eq!(g.take_ime_request(), Some(true));
    assert_eq!(g.take_ime_request(), None);

    // Characters arrive as text (never as keys); editing keys still arrive as keys.
    g.text_input("Zoë");
    g.key("z", true);
    g.key("z", false);
    g.key("backspace", true);
    g.key("backspace", false);
    assert_eq!(name(&g), "Zo");
    g.key("left", true);
    g.key("left", false);
    g.text_input("ab");
    assert_eq!(name(&g), "Zabo", "inserted at the caret after one step left");
    g.key("home", true);
    g.key("home", false);
    g.text_input(">");
    g.key("end", true);
    g.key("end", false);
    g.text_input("123456789");
    assert_eq!(name(&g), ">Zabo123", "capped at max_length = 8 characters");

    // Enter submits; the field stays active until the frame ends, so the game's own Enter
    // handler can still tell a field was being edited.
    g.key("enter", true);
    g.key("enter", false);
    assert!(g.wants_text());
    assert_eq!(g.eval::<Vec<String>>("t", "return submitted").unwrap(), [">Zabo123"]);
    g.frame(0.016);
    assert!(!g.wants_text());
    assert_eq!(g.take_ime_request(), Some(false));
    let keys: Vec<String> = g.eval("t", "return keys").unwrap();
    assert_eq!(
        keys,
        ["backspace", "left", "home", "end", "enter"].map(String::from),
        "no letters, no arrows eaten by nav"
    );

    // Not editing: arrows and Enter drive the form again (focus), and text is ignored.
    g.text_input("nope");
    assert_eq!(name(&g), ">Zabo123");
    g.key("down", true);
    g.key("down", false);
    assert_eq!(g.eval::<String>("t", "return (select(2, kulisa.ui.focused()))").unwrap(), "name");
    // Enter on a focused field starts editing it, Escape stops without submitting.
    g.key("enter", true);
    g.key("enter", false);
    assert!(g.wants_text());
    g.key("escape", true);
    g.key("escape", false);
    g.frame(0.016);
    assert!(!g.wants_text());
    assert_eq!(g.eval::<f32>("t", "return #submitted").unwrap(), 1.0);

    // Held keys repeat only while a field is active.
    g.key_repeat("backspace");
    assert_eq!(g.eval::<f32>("t", "return #keys").unwrap(), 6.0, "no repeats outside a field");
    let err = g.eval::<()>("t", "kulisa.ui.input{ value = name }").unwrap_err();
    assert!(err.contains("needs an `id`"), "{err}");
}

#[test]
fn sensors_and_haptics_go_through_the_runtime() {
    let Some(r) = renderer() else { return };
    let dir = folder(&[("main.luau", b"kulisa.display(64, 64, 'fit')")]);
    let device = kulisa_lua::Device { sensors: true, haptics: true, ..Default::default() };
    let mut g = Game::with_device(Assets::dir(dir), r, device).unwrap_or_else(|e| panic!("{e}"));

    assert!(g.eval::<bool>("t", "return kulisa.sensors.available() and kulisa.haptics.available()").unwrap());
    // No readings until enabled and fed by the runtime.
    assert_eq!(g.eval::<Option<f32>>("t", "return (kulisa.sensors.gyro())").unwrap(), None);
    assert!(!g.sensors_wanted() && g.take_sensor_request().is_none());
    g.eval::<()>("t", "kulisa.sensors.enable(true); kulisa.sensors.enable(true)").unwrap();
    assert!(g.sensors_wanted());
    assert_eq!(g.take_sensor_request(), Some(true));
    assert_eq!(g.take_sensor_request(), None, "asked once");

    let g98 = 9.80665_f32;
    // Held upright, then leaning right at 45 degrees with the top edge pushed back at 30.
    g.set_motion(Some(kulisa_platform::Motion { rate: [0.1, -0.2, 0.3], gravity: [0.0, -g98, 0.0] }));
    let (tx, ty): (f32, f32) = g.eval("t", "return kulisa.sensors.tilt()").unwrap();
    assert!(tx.abs() < 1e-4 && ty.abs() < 1e-4, "{tx} {ty}");
    assert_eq!(g.eval::<(f32, f32, f32)>("t", "return kulisa.sensors.gyro()").unwrap(), (0.1, -0.2, 0.3));
    let lean = std::f32::consts::FRAC_PI_4;
    g.set_motion(Some(kulisa_platform::Motion {
        rate: [0.0; 3],
        gravity: [g98 * lean.sin(), 0.0, -g98 * 0.3],
    }));
    let (tx, ty): (f32, f32) = g.eval("t", "return kulisa.sensors.tilt()").unwrap();
    assert!((tx - 1.0).abs() < 1e-4, "45 degrees right edge down is 1: {tx}");
    assert!((ty - 0.3 / std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4, "{ty}");
    let gz: f32 = g.eval("t", "local _, _, z = kulisa.sensors.gravity() return z").unwrap();
    assert!((gz + g98 * 0.3).abs() < 1e-4);
    g.eval::<()>("t", "kulisa.sensors.enable(false)").unwrap();
    assert_eq!(g.take_sensor_request(), Some(false));
    assert_eq!(g.eval::<Option<f32>>("t", "return (kulisa.sensors.tilt())").unwrap(), None, "readings stop");

    // Haptics: clamped, defaulted and handed over once.
    g.eval::<()>("t", "kulisa.haptics.pulse(); kulisa.haptics.pulse(3, 9); kulisa.haptics.pulse(0.25, 0.1)")
        .unwrap();
    assert_eq!(g.take_haptics(), [(0.5, 0.05), (1.0, 2.0), (0.25, 0.1)]);
    assert!(g.take_haptics().is_empty());
    g.eval::<()>("t", "for i = 1, 100 do kulisa.haptics.pulse(1, 0.01) end").unwrap();
    assert_eq!(g.take_haptics().len(), 16, "a script cannot flood the queue");
    let err = g.eval::<()>("t", "kulisa.haptics.pulse('hard')").unwrap_err();
    assert!(err.contains("strength"), "{err}");
}

#[test]
fn the_view_lifts_a_field_above_the_on_screen_keyboard() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        kulisa.load_font("Inter.ttf")
        kulisa.screen.clear = "#000000"
        local ui = kulisa.ui
        name = ui.signal("")
        ui.screen("form", function()
            return ui.box{ ui.input{ id = "name", value = name, anchor = "bottom", width = 40, height = 10,
                background = "#ff0000", size = 6 } }
        end, { area = "screen" })
        ui.show("form")
    "##,
    "Inter.ttf" => FONT);
    g.frame(0.016);
    let red =
        |g: &Game, y: u32| g.renderer().read_pixels().unwrap().pixels[((y * 64 + 32) * 4) as usize] > 200;
    g.render();
    assert!(red(&g, 58) && !red(&g, 40), "the field sits at the bottom");

    // Start editing with a 24 px keyboard up: the field would be hidden, so the view lifts.
    let (x, y, w, h): (f32, f32, f32, f32) = g.eval("t", "return kulisa.ui.bounds('form', 'name')").unwrap();
    g.pointer_moved(Some(v2(x + w / 2.0, y + h / 2.0)));
    g.pressed("left");
    g.released("left");
    g.set_keyboard_inset(24.0);
    assert_eq!(g.eval::<f32>("t", "return kulisa.input.keyboard_height()").unwrap(), 24.0);
    for _ in 0..60 {
        g.frame(0.016);
    }
    g.render();
    // Keyboard top at 40 and a 24 px margin: the field (54..64) lifts by 48 to 6..16, clear of it.
    assert!(!red(&g, 58), "the old spot is empty now");
    assert!(red(&g, 10), "the field moved up above the keyboard");
    // Pointers hit what they see: clicking the lifted field keeps editing.
    g.pointer_moved(Some(v2(32.0, 10.0)));
    g.pressed("left");
    g.released("left");
    assert!(g.wants_text());

    // The keyboard goes away: the view settles back.
    g.set_keyboard_inset(0.0);
    for _ in 0..60 {
        g.frame(0.016);
    }
    g.render();
    assert!(red(&g, 58), "back in place");
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
fn ui_windows_move_raise_block_and_set_cursors() {
    let mut g = game!("main.luau" => r##"
        kulisa.display(64, 64, "fit")
        local ui = kulisa.ui
        log, game_presses = {}, 0
        kulisa.on("press", function() game_presses += 1 end)
        local z = 10
        local function window(name, x, y)
            ui.screen(name, function()
                return ui.box{ block = true, clip = true, background = "#303030",
                    on_press = function() z += 1; ui.set_z(name, z); table.insert(log, "raise " .. name) end,
                    ui.box{ id = "bar", width = "fill", height = 8, cursor = "move",
                        on_drag = function(e)
                            table.insert(log, e.phase)
                            if e.phase == "move" or e.phase == "end" then
                                ui.place(name, { x = x + e.dx, y = y + e.dy, width = 30, height = 30 })
                            end
                            if e.phase == "end" then x += e.dx; y += e.dy end
                        end },
                    ui.box{ id = "spill", x = 25, y = 25, width = 20, height = 20, background = "#ff0000",
                        on_click = function() table.insert(log, "spill") end },
                }
            end, { area = { x = x, y = y, width = 30, height = 30 }, z = 10 })
            ui.show(name)
        end
        window("a", 2, 2)
        window("b", 20, 20)
    "##);
    let at = |g: &mut Game, x: f32, y: f32| g.pointer_moved(Some(v2(x, y)));
    g.frame(0.016);
    // b was shown last, so it is on top where the two overlap.
    at(&mut g, 25.0, 25.0);
    g.pressed("left");
    g.released("left");
    assert_eq!(g.eval::<Vec<String>>("t", "return log").unwrap(), ["raise b"]);
    assert_eq!(g.eval::<i64>("t", "return game_presses").unwrap(), 0, "the window body blocks");
    // Clicking a's visible part raises it; the clipped child outside its box is not hit.
    at(&mut g, 5.0, 15.0);
    g.pressed("left");
    g.released("left");
    at(&mut g, 35.0, 35.0); // a's "spill" button would be here, clipped away; b's body is here
    g.pressed("left");
    g.released("left");
    assert_eq!(g.eval::<Vec<String>>("t", "return log").unwrap(), ["raise b", "raise a", "raise b"]);
    // The title bar sets the cursor and drags its window.
    at(&mut g, 5.0, 4.0);
    assert_eq!(g.take_requests().cursor, Some("move"));
    g.eval::<()>("t", "log = {}").unwrap();
    drag(&mut g, 0, (5.0, 4.0), (15.0, 10.0));
    g.frame(0.016);
    assert_eq!(
        g.eval::<Vec<String>>("t", "return log").unwrap(),
        ["raise a", "start", "move", "move", "end"]
    );
    let (x, y, w, _): (f32, f32, f32, f32) = g.eval("t", "return kulisa.ui.bounds('a', 'bar')").unwrap();
    assert_eq!((x, y, w), (12.0, 8.0, 30.0));
    // Leaving the bar gives the cursor back; scripts can override it.
    at(&mut g, 60.0, 5.0);
    assert_eq!(g.take_requests().cursor, Some("default"));
    g.eval::<()>("t", "kulisa.input.cursor('wait')").unwrap();
    assert_eq!(g.take_requests().cursor, Some("wait"));
    let err = g.eval::<()>("t", "kulisa.input.cursor('spinny')").unwrap_err();
    assert!(err.contains("unknown cursor"), "{err}");
    g.eval::<()>("t", "kulisa.window:resize(800, 600)").unwrap();
    assert_eq!(g.take_requests().window_size, Some((800.0, 600.0)));
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
