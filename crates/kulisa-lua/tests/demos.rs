//! Starts `demos/smoke` and `demos/interact` headless and plays a few seconds of each: they must
//! keep starting with the engine, without Luau errors. With `KULISA_SHOTS=<dir>` it saves a
//! screenshot of each.

use kulisa_lua::Game;
use kulisa_platform::Assets;
use kulisa_render::Renderer;

fn game(name: &str) -> Option<Game> {
    let r = match pollster::block_on(Renderer::headless(1280, 720)) {
        Ok(r) => r,
        Err(e) if std::env::var_os("KULISA_REQUIRE_GPU").is_some() => panic!("no GPU adapter: {e}"),
        Err(e) => {
            eprintln!("skipping: {e}");
            return None;
        }
    };
    let dir = format!("{}/../../demos/{name}", env!("CARGO_MANIFEST_DIR"));
    let mut g = Game::new(Assets::dir(dir), r).unwrap_or_else(|e| panic!("{name}: {e}"));
    g.resize(1280, 720);
    g.eval::<()>("t", "errors = {}; kulisa.on('error', function(m) table.insert(errors, m) end)").unwrap();
    Some(g)
}

/// Runs frames the way the player does (render targets only draw when rendered).
fn play(g: &mut Game, secs: f32) {
    for _ in 0..(secs / 0.016) as usize {
        g.frame(0.016);
        g.render();
    }
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

fn no_errors(g: &Game, name: &str) {
    let errors: Vec<String> = g.eval("t", "return errors").unwrap();
    assert!(errors.is_empty(), "{name}: {errors:#?}");
}

#[test]
fn demos_start_and_play() {
    // Saves and persistent data stay out of a developer's own. One test in this binary, so
    // setting the variable races with nothing.
    let data = std::env::temp_dir().join(format!("kulisa-demos-test-{}", std::process::id()));
    // SAFETY: no other thread reads the environment at this point.
    unsafe { std::env::set_var("KULISA_DATA_DIR", &data) };

    for name in ["smoke", "interact"] {
        let Some(mut g) = game(name) else { return };
        play(&mut g, 2.5);
        no_errors(&g, name);
        shot(&mut g, name);
    }
}
