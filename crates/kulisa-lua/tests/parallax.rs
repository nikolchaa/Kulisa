//! Runs `demos/parallax` headless: the valley through a day with particles, the depth-map
//! diorama, every post-processing look, the display lab (scale modes and the aspect clamp
//! switched at runtime, the overlay) and the phone's portrait profile.

use kulisa_core::Vec2;
use kulisa_lua::{Device, FormFactor, Game};
use kulisa_platform::Assets;
use kulisa_render::Renderer;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../demos/parallax");

fn renderer(w: u32, h: u32) -> Option<Renderer> {
    match pollster::block_on(Renderer::headless(w, h)) {
        Ok(r) => Some(r),
        Err(e) if std::env::var_os("KULISA_REQUIRE_GPU").is_some() => panic!("no GPU adapter: {e}"),
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

fn frames(g: &mut Game, n: usize) {
    for _ in 0..n {
        g.frame(0.033);
        g.render();
    }
}

fn eval<R: mlua::FromLuaMulti>(g: &Game, src: &str) -> R {
    g.eval::<R>("t", src).unwrap_or_else(|e| panic!("{src}: {e}"))
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

fn key(g: &mut Game, k: &str) {
    g.key(k, true);
    g.key(k, false);
    frames(g, 2);
}

fn errors(g: &Game) -> Vec<String> {
    eval(g, "return errors")
}

#[test]
fn parallax_demo_runs() {
    let Some(r) = renderer(1280, 720) else { return };
    let mut g = Game::new(Assets::dir(DIR), r).unwrap_or_else(|e| panic!("{e}"));
    g.resize(1280, 720);
    eval::<()>(&g, "errors = {}; kulisa.on('error', function(m) table.insert(errors, m) end)");
    frames(&mut g, 20);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
    let alive: i64 = eval(&g, "return require('lib/valley').alive()");
    assert!(alive > 50, "particles are running: {alive}");

    // The pointer moves the camera; near layers move more than far ones.
    g.pointer_moved(Some(Vec2::new(1270.0, 360.0)));
    frames(&mut g, 30);
    let cam: f32 = eval(&g, "return kulisa.camera.x - kulisa.screen.width / 2");
    assert!(cam > 30.0, "camera follows the pointer: {cam}");
    for (t, name) in [(0.72, "dusk"), (0.5, "noon"), (0.02, "night")] {
        eval::<()>(&g, &format!("require('lib/valley').set_time({t})"));
        frames(&mut g, 4);
        shot(&mut g, &format!("parallax-valley-{name}"));
    }
    // Snow, then every post-processing look.
    eval::<()>(&g, "local v = require('lib/valley'); v.weather.snow = true; v.set_weather(v.weather)");
    for k in ["t", "c"] {
        key(&mut g, k);
    }
    frames(&mut g, 30);
    shot(&mut g, "parallax-fx");
    let effects: i64 = eval(&g, "return #kulisa.screen.effects + 0");
    assert_eq!(effects, 0, "the UI stays out of the post-processing");
    for k in ["t", "c", "b", "g", "n"] {
        key(&mut g, k);
    }
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));

    // The diorama follows the view.
    key(&mut g, "1");
    key(&mut g, "2");
    g.pointer_moved(Some(Vec2::new(10.0, 600.0)));
    frames(&mut g, 30);
    shot(&mut g, "parallax-diorama");
    let p: f32 = eval(&g, "return require('lib/diorama') and 1 or 0");
    assert_eq!(p, 1.0);

    // The display lab: overlay, then fit (letterboxed) and fill, switched at runtime.
    key(&mut g, "1");
    key(&mut g, "d");
    shot(&mut g, "parallax-lab-expand");
    key(&mut g, "m");
    assert_eq!(eval::<String>(&g, "return kulisa.screen.mode"), "fit");
    g.resize(1600, 600); // a wide window: fit shows bars on the sides
    frames(&mut g, 4);
    shot(&mut g, "parallax-lab-fit-wide");
    let (vx, vw): (f32, f32) = eval(&g, "local x, _, w = kulisa.screen:visible(); return x, w");
    assert_eq!((vx, vw), (0.0, 1920.0), "fit shows exactly the design");
    key(&mut g, "m");
    assert_eq!(eval::<String>(&g, "return kulisa.screen.mode"), "fill");
    key(&mut g, "m");
    // Expand on 32:9 with the clamp: bars beyond 21:9; without: the scene bleeds to the edges.
    g.resize(1920, 540);
    frames(&mut g, 4);
    let clamped: f32 = eval(&g, "local _, _, w = kulisa.screen:visible(); return w");
    key(&mut g, "a");
    frames(&mut g, 4);
    shot(&mut g, "parallax-lab-32x9-unclamped");
    let open: f32 = eval(&g, "local _, _, w = kulisa.screen:visible(); return w");
    assert!(open > clamped + 100.0, "{clamped} -> {open}");
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
    drop(g);

    // A phone: the portrait design, the settings sheet.
    let Some(r) = renderer(540, 1170) else { return };
    let phone =
        Device { form: FormFactor::Phone, touch: true, can_lock: true, sensors: true, ..Default::default() };
    let mut g = Game::with_device(Assets::dir(DIR), r, phone).unwrap_or_else(|e| panic!("{e}"));
    g.resize(540, 1170);
    eval::<()>(&g, "errors = {}; kulisa.on('error', function(m) table.insert(errors, m) end)");
    frames(&mut g, 20);
    assert_eq!(eval::<f32>(&g, "return kulisa.screen.height"), 1920.0);
    shot(&mut g, "parallax-phone");
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}
