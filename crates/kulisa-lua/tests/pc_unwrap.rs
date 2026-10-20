//! Plays `demos/pc-unwrap` headless: unbox, slide the bag off, peel the wrap foil across the front
//! glass and round the corner, fit a protector and squeegee its bubbles out. The demo must keep
//! working with the engine (and not raise Luau errors on the way).

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

fn frames(g: &mut Game, n: usize) {
    for _ in 0..n {
        g.frame(0.016);
    }
}

fn num(g: &Game, src: &str) -> f32 {
    g.eval::<f32>("t", &format!("return {src}")).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn truth(g: &Game, src: &str) -> bool {
    g.eval::<bool>("t", &format!("return {src}")).unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// Window position of a stage point (the demo scales and centers its stage in the window).
fn at(g: &Game, sx: f32, sy: f32) -> Vec2 {
    let s = num(g, "require('lib/state').stage.scale");
    let (x, y) = (num(g, "require('lib/state').stage.x"), num(g, "require('lib/state').stage.y"));
    Vec2::new(x + sx * s, y + sy * s)
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

#[test]
fn pc_unwrap_plays() {
    let Some(mut g) = game() else { return };
    g.eval::<()>("t", "errors = {}; kulisa.on('error', function(m) table.insert(errors, m) end)").unwrap();
    frames(&mut g, 10);
    assert_eq!(num(&g, "#errors"), 0.0, "startup errors");
    assert!(truth(&g, "require('lib/state').unbox.phase == 'tape'"));

    // Peel the tape off the box (a fast pull along the seam), then pull the flaps open.
    let (tx, ty) =
        (num(&g, "require('lib/state').unbox.tape.Cx") + 24.0, num(&g, "require('lib/state').unbox.tape.Cy"));
    let end = tx + 640.0;
    drag(&mut g, &[(tx, ty), (tx + 150.0, ty), (tx + 320.0, ty - 4.0), (tx + 480.0, ty), (end, ty)]);
    frames(&mut g, 90);
    assert!(truth(&g, "require('lib/state').done('tape')"), "tape peeled");
    drag(&mut g, &[(640.0, 400.0), (640.0, 500.0), (640.0, 640.0)]);
    frames(&mut g, 120);
    assert!(truth(&g, "require('lib/state').done('flaps')"), "flaps open");
    // Foam blocks out, one at a time (find them by their sprites' hit areas).
    let foam = |g: &Game, which: &str| {
        let s = format!("require('lib/state').unbox.l_foam_{which}");
        (num(g, &format!("{s}.x")), num(g, &format!("{s}.y")))
    };
    let _ = foam(&g, "l");
    drag(&mut g, &[(420.0, 380.0), (260.0, 300.0), (60.0, 200.0)]);
    frames(&mut g, 60);
    drag(&mut g, &[(900.0, 380.0), (1050.0, 300.0), (1200.0, 200.0)]);
    frames(&mut g, 60);
    assert!(truth(&g, "require('lib/state').done('foam')"), "foam out");
    // Lift the PC out.
    drag(&mut g, &[(640.0, 560.0), (640.0, 440.0), (640.0, 300.0)]);
    frames(&mut g, 120);
    assert!(truth(&g, "require('lib/state').done('lift')"), "pc lifted out");
    assert_eq!(num(&g, "require('lib/state').pc.scale"), 1.0);

    // Slide the bag off to the left.
    drag(&mut g, &[(700.0, 400.0), (500.0, 400.0), (300.0, 400.0), (100.0, 400.0), (-100.0, 400.0)]);
    frames(&mut g, 120);
    assert!(truth(&g, "require('lib/state').done('bag')"), "bag off");

    // The wrap foil: grab its tab (left edge, middle) and pull across the front glass and round the
    // corner.
    let (fx, fy) = (
        num(&g, "require('lib/state').films.glass.Cx") + 30.0,
        num(&g, "require('lib/state').films.glass.Cy"),
    );
    drag(
        &mut g,
        &[
            (fx, fy),
            (fx + 160.0, fy),
            (fx + 340.0, fy),
            (fx + 520.0, fy),
            (fx + 700.0, fy),
            (fx + 860.0, fy),
            (fx + 1000.0, fy),
        ],
    );
    frames(&mut g, 160);
    assert!(truth(&g, "require('lib/state').done('glass')"), "foil peeled off");

    // Fit a fresh protector and squeegee it.
    g.eval::<()>("t", "require('lib/state').protector:start()").unwrap();
    frames(&mut g, 80);
    let bubbles = num(&g, "#require('lib/state').protector.bubbles");
    assert!(bubbles >= 8.0, "bubbles trapped: {bubbles}");
    for k in 0..6 {
        let y = 190.0 + 70.0 * k as f32;
        drag(&mut g, &[(270.0, y), (500.0, y), (720.0, y), (880.0, y)]);
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
