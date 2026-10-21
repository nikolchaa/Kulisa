//! Plays `templates/classic-vn` headless: the template must keep working with the engine.
//! With `KULISA_SHOTS=<dir>` it saves screenshots along the way.

use kulisa_core::Vec2;
use kulisa_lua::Game;
use kulisa_platform::Assets;
use kulisa_render::Renderer;

fn game() -> Option<Game> {
    // Keep the test's saves and seen lines away from a developer's own playthrough. This
    // binary has one test, so setting the variable races with nothing.
    let data = std::env::temp_dir().join(format!("kulisa-template-test-{}", std::process::id()));
    // SAFETY: no other thread reads the environment at this point.
    unsafe { std::env::set_var("KULISA_DATA_DIR", &data) };
    let r = match pollster::block_on(Renderer::headless(1280, 720)) {
        Ok(r) => r,
        Err(e) if std::env::var_os("KULISA_REQUIRE_GPU").is_some() => panic!("no GPU adapter: {e}"),
        Err(e) => {
            eprintln!("skipping: {e}");
            return None;
        }
    };
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../templates/classic-vn");
    let mut g = Game::new(Assets::dir(dir), r).unwrap_or_else(|e| panic!("{e}"));
    g.resize(1280, 720);
    Some(g)
}

/// Clicks the middle of the widget `id` on `screen` (window pixels = design / 1.5 here).
fn click(g: &mut Game, screen: &str, id: &str) {
    let (x, y, w, h): (f32, f32, f32, f32) = g
        .eval("t", &format!("return kulisa.ui.bounds('{screen}', '{id}')"))
        .unwrap_or_else(|e| panic!("{e}"));
    let p = Vec2::new(x + w / 2.0, y + h / 2.0) / 1.5;
    g.pointer_moved(Some(p));
    g.pressed("left");
    g.released("left");
}

fn frames(g: &mut Game, n: usize) {
    for _ in 0..n {
        g.frame(0.05);
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

fn eval<R: mlua_free::FromLua>(g: &Game, src: &str) -> R {
    R::get(g, src)
}

/// Results from Luau without naming mlua types in this test.
mod mlua_free {
    use kulisa_lua::Game;
    pub trait FromLua: Sized {
        fn get(g: &Game, src: &str) -> Self;
    }
    impl FromLua for bool {
        fn get(g: &Game, src: &str) -> Self {
            g.eval("t", &format!("return {src}")).unwrap_or_else(|e| panic!("{src}: {e}"))
        }
    }
    impl FromLua for String {
        fn get(g: &Game, src: &str) -> Self {
            g.eval("t", &format!("return tostring({src})")).unwrap_or_else(|e| panic!("{src}: {e}"))
        }
    }
}

fn line(g: &Game) -> String {
    eval(g, "require('game/state').line and require('game/state').line.text")
}

/// Advances like a player: finishes the line, then moves on (waiting out transitions).
fn advance(g: &mut Game) {
    frames(g, 30);
    g.key("space", true);
    g.key("space", false);
    frames(g, 1);
    g.key("space", true);
    g.key("space", false);
    frames(g, 30);
}

#[test]
fn classic_vn_plays() {
    let Some(mut g) = game() else { return };
    let id: String = eval(&g, "kulisa.config.game.id");
    g.eval::<()>("t", "errors = {}; kulisa.on('error', function(m) table.insert(errors, m) end)").unwrap();
    frames(&mut g, 2);
    assert!(eval::<bool>(&g, "kulisa.ui.shown('main_menu')"));
    shot(&mut g, "classic-vn-1-menu");
    click(&mut g, "main_menu", "start");
    frames(&mut g, 40);
    assert!(eval::<bool>(&g, "kulisa.ui.shown('say') and not kulisa.ui.shown('main_menu')"));
    assert_eq!(line(&g), "Afternoon sun had found the kitchen counter, and someone small had found the sun.");
    // The text box waited for the fade, then typed the line.
    assert!(eval::<bool>(&g, "kulisa.ui.find('say', 'what') ~= nil"));
    advance(&mut g);
    assert_eq!(line(&g), "Mrrp? You're new here. Are you here about the counter too?");
    advance(&mut g);
    advance(&mut g);
    advance(&mut g);
    assert!(eval::<bool>(&g, "kulisa.ui.shown('choice')"), "at the menu: {}", line(&g));
    shot(&mut g, "classic-vn-2-choice");
    assert!(eval::<bool>(&g, "kulisa.save.exists('auto')"), "autosave at menus");

    // Keys pick choices; rollback returns to the menu and the choice can change.
    g.key("2", true);
    g.key("2", false);
    frames(&mut g, 30);
    assert_eq!(line(&g), "Later, then. I'll be in my sunbeam.");
    shot(&mut g, "classic-vn-3-later");
    g.key("pageup", true);
    g.key("pageup", false);
    frames(&mut g, 5);
    assert!(eval::<bool>(&g, "kulisa.ui.shown('choice')"));
    g.key("1", true);
    g.key("1", false);
    frames(&mut g, 30);
    assert_eq!(line(&g), "...Fine. One crunchy each. Don't tell the humans.");

    // Quick save, play on, quick load.
    g.eval::<()>("t", "require('game/actions').quick_save()").unwrap();
    advance(&mut g);
    assert_eq!(line(&g), "By evening the whole gang had gathered in the playroom.");
    frames(&mut g, 40);
    shot(&mut g, "classic-vn-4-playroom");
    if std::env::var_os("KULISA_SHOTS").is_some() {
        // Half way through a swirl back to the kitchen (only for the screenshots).
        g.eval::<()>(
            "t",
            "local s = require('game/stage'); s.transition('swirl'); s.scene({ 'bg', 'kitchen' })",
        )
        .unwrap();
        frames(&mut g, 12);
        shot(&mut g, "classic-vn-5-swirl");
        frames(&mut g, 20);
        g.eval::<()>("t", "require('game/stage').scene({ 'bg', 'playroom' })").unwrap();
    }
    g.eval::<()>("t", "require('game/actions').quick_load()").unwrap();
    frames(&mut g, 5);
    assert_eq!(line(&g), "...Fine. One crunchy each. Don't tell the humans.");
    assert!(eval::<bool>(&g, "kulisa.save.list()[1].thumbnail ~= nil"));

    // Skip runs through seen lines and stops at the first unseen one.
    g.key("tab", true);
    g.key("tab", false);
    frames(&mut g, 40);
    assert_eq!(eval::<String>(&g, "require('game/state').mode:peek()"), "normal");
    assert_eq!(line(&g), "Thanks for sharing. Treats taste better with company.");

    // The game menu, then back to the main menu.
    g.key("escape", true);
    g.key("escape", false);
    frames(&mut g, 2);
    assert!(eval::<bool>(&g, "kulisa.ui.shown('game_menu')"));
    g.eval::<()>("t", "require('game/actions').main_menu()").unwrap();
    frames(&mut g, 2);
    assert!(eval::<bool>(&g, "kulisa.ui.shown('main_menu') and not kulisa.ui.shown('say')"));
    // Keyboard and gamepad alone. The player has used the keys above, so the main menu opened
    // with Start focused; Enter begins, then the pad plays on.
    frames(&mut g, 2);
    assert_eq!(eval::<String>(&g, "select(2, kulisa.ui.focused())"), "start");
    g.key("enter", true);
    g.key("enter", false);
    frames(&mut g, 40);
    assert_eq!(line(&g), "Afternoon sun had found the kitchen counter, and someone small had found the sun.");
    let press = |g: &mut Game, button: &str| {
        g.gamepad_button(0, button, true);
        g.gamepad_button(0, button, false);
    };
    // A finishes the line, then moves on; at the menu it picks instead, so stop when it shows.
    for _ in 0..12 {
        frames(&mut g, 30);
        if eval::<bool>(&g, "kulisa.ui.shown('choice')") {
            break;
        }
        press(&mut g, "a");
    }
    frames(&mut g, 30);
    assert!(eval::<bool>(&g, "kulisa.ui.shown('choice')"), "at the menu: {}", line(&g));
    // The first choice is focused already (the player has used the arrows); down, then A.
    assert_eq!(eval::<String>(&g, "(select(1, kulisa.ui.focused()))"), "choice");
    press(&mut g, "down");
    press(&mut g, "a");
    frames(&mut g, 30);
    assert_eq!(line(&g), "Later, then. I'll be in my sunbeam.");
    // Y opens the history, RB changes section, B closes; LB rolls back to the menu.
    press(&mut g, "y");
    frames(&mut g, 2);
    assert_eq!(eval::<String>(&g, "require('game/state').menu:peek()"), "history");
    press(&mut g, "rb");
    frames(&mut g, 2);
    assert_eq!(eval::<String>(&g, "require('game/state').menu:peek()"), "save");
    press(&mut g, "b");
    frames(&mut g, 2);
    assert!(!eval::<bool>(&g, "kulisa.ui.shown('game_menu')"));
    press(&mut g, "lb");
    frames(&mut g, 5);
    assert!(eval::<bool>(&g, "kulisa.ui.shown('choice')"));
    // Moving the mouse hands control back to the pointer.
    g.pointer_moved(Some(Vec2::new(5.0, 5.0)));
    g.pointer_moved(Some(Vec2::new(300.0, 300.0)));
    assert_eq!(eval::<String>(&g, "tostring(kulisa.ui.focused())"), "nil");
    g.eval::<()>("t", "require('game/actions').main_menu()").unwrap();
    frames(&mut g, 2);
    let errors: Vec<String> = g.eval("t", "return errors").unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    drop(g);
    let _ = std::fs::remove_dir_all(kulisa_platform::data_dir(&id).unwrap());
}
