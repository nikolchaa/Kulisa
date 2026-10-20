//! Plays `demos/pc-unwrap` headless: peel the tape, open the box, take the foam caps out, lift the
//! PC out, pull the bag off over the top, peel both glass films and the I/O sticker, fit a
//! protector and squeegee its bubbles out. The demo must keep working with the engine (and not
//! raise Luau errors on the way). With `KULISA_SHOTS=<dir>` it saves screenshots along the way.

use kulisa_core::Vec2;
use kulisa_lua::Game;
use kulisa_platform::Assets;
use kulisa_render::Renderer;

fn game() -> Option<Game> {
    let data = std::env::temp_dir().join(format!("kulisa-pc-unwrap-test-{}", std::process::id()));
    // SAFETY: this binary has one test, so nothing else reads the environment meanwhile.
    unsafe { std::env::set_var("KULISA_DATA_DIR", &data) };
    let r = match pollster::block_on(Renderer::headless(1280, 800)) {
        Ok(r) => r,
        Err(e) if std::env::var_os("KULISA_REQUIRE_GPU").is_some() => panic!("no GPU adapter: {e}"),
        Err(e) => {
            eprintln!("skipping: {e}");
            return None;
        }
    };
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../demos/pc-unwrap");
    let mut g = Game::new(Assets::dir(dir), r).unwrap_or_else(|e| panic!("{e}"));
    g.resize(1280, 800);
    Some(g)
}

/// Runs frames the way the player does (render targets only draw when rendered).
fn frames(g: &mut Game, n: usize) {
    for _ in 0..n {
        g.frame(0.016);
        g.render();
    }
}

fn num(g: &Game, src: &str) -> f32 {
    g.eval::<f32>("t", &format!("return {src}")).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn truth(g: &Game, src: &str) -> bool {
    g.eval::<bool>("t", &format!("return {src}")).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn shot(g: &mut Game, name: &str) {
    let Some(dir) = std::env::var_os("KULISA_SHOTS") else { return };
    g.render();
    if let Some(img) = g.renderer().read_pixels() {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), img.encode_png().unwrap())
            .unwrap();
    }
}

/// Window position of a scene point (inside the demo's view, which is zoomed and centred).
fn at(g: &Game, sx: f32, sy: f32) -> Vec2 {
    let v = g.eval::<(f32, f32)>("t", &format!("return require('lib/state').to_design({sx}, {sy})")).unwrap();
    Vec2::new(v.0, v.1)
}

/// A scene point from a Luau expression returning two numbers.
fn point(g: &Game, src: &str) -> (f32, f32) {
    g.eval::<(f32, f32)>("t", &format!("return {src}")).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn drag(g: &mut Game, path: &[(f32, f32)]) {
    g.pointer_moved(Some(at(g, path[0].0, path[0].1)));
    frames(g, 3);
    g.pressed("left");
    frames(g, 2);
    for w in path.windows(2) {
        for i in 1..=8 {
            let t = i as f32 / 8.0;
            let (x, y) = (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
            g.pointer_moved(Some(at(g, x, y)));
            frames(g, 2);
        }
    }
    g.released("left");
    frames(g, 4);
}

/// Pulls a film from its tab by `dx`, `dy` scene units in a few steps.
fn peel(g: &mut Game, film: &str, dx: f32, dy: f32) {
    let s = format!("require('lib/state').films.{film}");
    // Cx, Cy is where the tab starts: the top corner of a corner film, the middle of the edge
    // otherwise; aim a little inside it.
    let down = if truth(g, &format!("{s}.o.corner == true")) { 14.0 } else { 0.0 };
    let (fx, fy) = (num(g, &format!("{s}.Cx")) + 12.0, num(g, &format!("{s}.Cy")) + down);
    let path: Vec<(f32, f32)> =
        (0..=6).map(|k| (fx + dx * k as f32 / 6.0, fy + dy * k as f32 / 6.0)).collect();
    drag(g, &path);
}

#[test]
fn pc_unwrap_plays() {
    let Some(mut g) = game() else { return };
    g.eval::<()>("t", "errors = {}; kulisa.on('error', function(m) table.insert(errors, m) end)").unwrap();
    frames(&mut g, 10);
    assert_eq!(num(&g, "#errors"), 0.0, "startup errors");
    assert!(truth(&g, "require('lib/state').unbox.phase == 'tape'"));
    shot(&mut g, "pc-1-box");

    // Peel the tape off along the seam (it runs from the back, left on screen, to the front).
    let (tx, ty) = point(&g, "require('lib/state').unbox:tape_tab()");
    drag(
        &mut g,
        &[
            (tx, ty),
            (tx + 150.0, ty + 30.0),
            (tx + 330.0, ty + 60.0),
            (tx + 520.0, ty + 95.0),
            (tx + 760.0, ty + 140.0),
        ],
    );
    frames(&mut g, 90);
    assert!(truth(&g, "require('lib/state').done('tape')"), "tape peeled");
    // Pull the flaps open.
    let top = point(&g, "require('lib/geom').project(0, 57.6, 0)");
    drag(&mut g, &[top, (top.0, top.1 + 120.0), (top.0, top.1 + 260.0), (top.0, top.1 + 400.0)]);
    frames(&mut g, 30);
    shot(&mut g, "pc-2-opening");
    frames(&mut g, 120);
    assert!(truth(&g, "require('lib/state').done('flaps')"), "flaps open");
    shot(&mut g, "pc-3-open");
    // The top foam caps out, one at a time (up first, then away).
    for (cap, dir) in [("rear", -1.0), ("front", 1.0)] {
        let c = point(
            &g,
            &format!(
                "(function() local s = require('lib/state').unbox.foams.{cap}.sprite; return s.x + s.width / 2, s.y + s.height / 2 end)()"
            ),
        );
        drag(&mut g, &[c, (c.0, c.1 - 90.0), (c.0 + dir * 150.0, c.1 - 200.0)]);
        frames(&mut g, 60);
    }
    assert!(truth(&g, "require('lib/state').done('foam')"), "foam out");
    // Lift the PC up and out.
    let mid = point(&g, "require('lib/geom').project(0, 30, -11.5)");
    let (_, my) = point(&g, "require('lib/state').pc.x, require('lib/state').pc.y");
    let mid = (mid.0, mid.1 + my);
    drag(&mut g, &[mid, (mid.0, mid.1 - 200.0), (mid.0, mid.1 - 420.0), (mid.0, mid.1 - 640.0)]);
    frames(&mut g, 20);
    shot(&mut g, "pc-4-lifted");
    frames(&mut g, 160);
    assert!(truth(&g, "require('lib/state').done('lift')"), "pc lifted out");
    assert!(truth(&g, "not require('lib/state').unbox:active()"), "box gone");
    shot(&mut g, "pc-5-bagged");

    // The bag comes off over the top only: a sideways drag does not move it off.
    let bag = "require('lib/state').bag";
    let b = point(&g, "require('lib/geom').project(0, 30, -12)");
    drag(&mut g, &[b, (b.0 - 200.0, b.1), (b.0 - 400.0, b.1)]);
    frames(&mut g, 60);
    assert!(num(&g, &format!("{bag}:progress()")) < 0.05, "sideways does not pull it off");
    // Held high, it slides up as a whole (and slides back down when let go part way).
    let hi = point(&g, "require('lib/geom').project(0, 45, -12)");
    drag(&mut g, &[hi, (hi.0, hi.1 - 60.0), (hi.0, hi.1 - 120.0)]);
    assert!(num(&g, &format!("{bag}.s")) > 5.0, "a high grab slides the bag");
    assert!(num(&g, &format!("{bag}.c")) < 0.5, "a high grab does not bunch it");
    frames(&mut g, 90);
    assert!(num(&g, &format!("{bag}.s")) < 1.0, "let go, it slides back down");
    // Held low, the hem climbs and the plastic above the hand bunches up; the bunch stays.
    let lo = point(&g, "require('lib/geom').project(0, 8, -12)");
    drag(&mut g, &[lo, (lo.0, lo.1 - 120.0), (lo.0, lo.1 - 250.0)]);
    frames(&mut g, 30);
    let c = num(&g, &format!("{bag}.c"));
    assert!(c > 15.0, "a low grab bunches the bag: {c}");
    assert!(num(&g, &format!("{bag}.s")) < 1.0, "a low grab does not slide it");
    shot(&mut g, "pc-6-bag-bunched");
    // Pulled on from below the bunch: bunched tight, the whole bunch comes off over the top.
    let lo = point(&g, &format!("require('lib/geom').project(0, {}, -12)", c - 2.0));
    drag(&mut g, &[lo, (lo.0, lo.1 - 250.0), (lo.0, lo.1 - 500.0), (lo.0, lo.1 - 800.0)]);
    frames(&mut g, 120);
    assert!(truth(&g, "require('lib/state').done('bag')"), "bag off");
    shot(&mut g, "pc-7-films");

    // The side film: from its tab across the glass.
    peel(&mut g, "side", 1000.0, 420.0);
    frames(&mut g, 120);
    assert!(truth(&g, "require('lib/state').done('side')"), "side film peeled off");
    // The front film.
    peel(&mut g, "front", 700.0, 200.0);
    frames(&mut g, 120);
    assert!(truth(&g, "require('lib/state').done('front')"), "front film peeled off");
    // The sticker: once off it falls.
    peel(&mut g, "sticker", 120.0, -60.0);
    frames(&mut g, 6);
    let y0 = num(
        &g,
        "require('lib/state').films.sticker.free and require('lib/state').films.sticker.free.py or 0",
    );
    frames(&mut g, 20);
    let y1 = num(
        &g,
        "require('lib/state').films.sticker.free and require('lib/state').films.sticker.free.py or 0",
    );
    assert!(y1 > y0, "a let-go sticker falls: {y0} -> {y1}");
    frames(&mut g, 100);
    assert!(truth(&g, "require('lib/state').done('sticker')"), "sticker removed");
    shot(&mut g, "pc-8-bare");

    // Fit a fresh protector and squeegee it.
    g.eval::<()>("t", "require('lib/state').protector:start()").unwrap();
    frames(&mut g, 80);
    let bubbles = num(&g, "#require('lib/state').protector.bubbles");
    assert!(bubbles >= 8.0, "bubbles trapped: {bubbles}");
    shot(&mut g, "pc-9-protector");
    for k in 0..6 {
        let (x0, y) =
            point(&g, &format!("require('lib/geom').project(21, {}, -11.5)", 44.0 - 7.5 * k as f32));
        let (x1, _) = point(&g, "require('lib/geom').project(-21, 0, -11.5)");
        drag(&mut g, &[(x0, y), ((x0 + x1) / 2.0, y + 20.0), (x1, y + 40.0)]);
    }
    frames(&mut g, 60);
    let left = num(&g, "#require('lib/state').protector.bubbles");
    assert!(left < bubbles, "the squeegee pushed bubbles out: {left} of {bubbles} left");
    assert_eq!(
        num(&g, "#errors"),
        0.0,
        "{}",
        g.eval::<String>("t", "return table.concat(errors, '\\n')").unwrap()
    );
    g.render();
}
