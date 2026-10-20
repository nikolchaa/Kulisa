//! Plays `templates/text-rpg` headless: the title screen, the story with choices, dice and items,
//! using an item from the pack, save and load, and the phone (portrait) layout with its sheets.

use kulisa_core::Vec2;
use kulisa_lua::{Device, FormFactor, Game};
use kulisa_platform::Assets;
use kulisa_render::Renderer;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../templates/text-rpg");

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
        g.frame(0.05);
    }
}

fn truth(g: &Game, src: &str) -> bool {
    g.eval::<bool>("t", &format!("return {src}")).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn num(g: &Game, src: &str) -> f32 {
    g.eval::<f32>("t", &format!("return {src}")).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn string(g: &Game, src: &str) -> String {
    g.eval::<String>("t", &format!("return tostring({src})")).unwrap_or_else(|e| panic!("{src}: {e}"))
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

/// Clicks the middle of widget `id` on `screen`; `k` is design units per window pixel.
fn click(g: &mut Game, screen: &str, id: &str, k: f32) {
    let (x, y, w, h): (f32, f32, f32, f32) = g
        .eval("t", &format!("return kulisa.ui.bounds('{screen}', '{id}')"))
        .unwrap_or_else(|e| panic!("{screen}/{id}: {e}"));
    g.pointer_moved(Some(Vec2::new(x + w / 2.0, y + h / 2.0) / k));
    g.pressed("left");
    g.released("left");
    frames(g, 2);
}

fn key(g: &mut Game, k: &str) {
    g.key(k, true);
    g.key(k, false);
}

/// Presses Space until the story shows choices (or ends); returns whether choices are up.
fn read_on(g: &mut Game) -> bool {
    for _ in 0..80 {
        frames(g, 3);
        if truth(g, "require('game/play').choices:peek() ~= nil") {
            return true;
        }
        if truth(g, "require('game/play').ended:peek()") {
            return false;
        }
        key(g, "space");
    }
    panic!(
        "the story did not reach a choice; log: {}; waiting {} newest complete {} focused {}",
        string(g, "require('game/log').last().text"),
        string(g, "require('game/play').waiting"),
        string(g, "kulisa.ui.find('main', 'newest') and kulisa.ui.find('main', 'newest').complete"),
        string(g, "kulisa.ui.focused()")
    );
}

fn chapter(g: &Game) -> String {
    string(g, "kulisa.story.vars().chapter")
}

#[test]
fn text_rpg_plays() {
    let data = std::env::temp_dir().join(format!("kulisa-text-rpg-test-{}", std::process::id()));
    // SAFETY: this binary sets the variable once, before any game runs.
    unsafe { std::env::set_var("KULISA_DATA_DIR", &data) };
    let Some(r) = renderer(1280, 720) else { return };
    let mut g = Game::new(Assets::dir(DIR), r).unwrap_or_else(|e| panic!("{e}"));
    g.resize(1280, 720);
    g.eval::<()>("t", "errors = {}; kulisa.on('error', function(m) table.insert(errors, m) end)").unwrap();
    frames(&mut g, 4);
    shot(&mut g, "rpg-title");
    // Load from the title shows the play screen behind it again (its illustration is a game node
    // that hiding the screen must keep), then back to the title.
    click(&mut g, "menu", "title_load", 1.5);
    frames(&mut g, 2);
    assert!(
        truth(&g, "#errors == 0"),
        "{}",
        g.eval::<String>("t", "return table.concat(errors, '\\n')").unwrap()
    );
    click(&mut g, "menu", "close", 1.5);
    frames(&mut g, 2);
    click(&mut g, "menu", "new", 1.5);
    assert!(!truth(&g, "kulisa.ui.shown('menu')"));
    frames(&mut g, 20);
    shot(&mut g, "rpg-start");

    // The first choice: promise to help (number key), which gives a herb.
    assert!(read_on(&mut g));
    shot(&mut g, "rpg-choice");
    key(&mut g, "1");
    assert!(truth(&g, "kulisa.story.vars().promised"));
    // The mill: the wit check, picked by clicking the button.
    assert!(read_on(&mut g));
    assert!(truth(&g, "require('game/rules').has('herb')"));
    assert_eq!(chapter(&g), "The mill");
    click(&mut g, "main", "choice2", 1.5);
    let rolled = "(function() for _, e in require('game/log').entries do if e.kind == 'roll' then return true end end return false end)()";
    assert!(truth(&g, rolled), "the check was rolled and logged");
    // The cellar: open the chest.
    assert!(read_on(&mut g));
    assert_eq!(chapter(&g), "The cellar");
    key(&mut g, "1");
    for _ in 0..10 {
        if truth(&g, "require('game/rules').has('dagger')") {
            break;
        }
        frames(&mut g, 3);
        key(&mut g, "space");
    }
    assert!(truth(
        &g,
        "require('game/rules').has('dagger') and require('game/rules').has('coins') and require('game/rules').has('key')"
    ));
    frames(&mut g, 10);
    shot(&mut g, "rpg-cellar");

    // Use the coin purse from the pack: +5 gold.
    let gold = num(&g, "kulisa.story.vars().gold");
    click(&mut g, "main", "item:coins", 1.5);
    click(&mut g, "main", "use", 1.5);
    assert_eq!(num(&g, "kulisa.story.vars().gold"), gold + 5.0);
    assert!(!truth(&g, "require('game/rules').has('coins')"));

    // Save here (through the menu), play on, load back.
    click(&mut g, "main", "save", 1.5);
    click(&mut g, "menu", "slot1", 1.5);
    click(&mut g, "menu", "close", 1.5);
    let entries = num(&g, "#require('game/log').entries");
    assert!(read_on(&mut g));
    assert_eq!(chapter(&g), "The pines");
    shot(&mut g, "rpg-pines");
    key(&mut g, "2"); // the dagger
    let ended = !read_on(&mut g);
    assert!(ended, "the story ends after the pines");
    frames(&mut g, 10);
    shot(&mut g, "rpg-end");
    assert!(truth(&g, "kulisa.ui.find('main', 'again') ~= nil"));

    g.eval::<()>("t", "require('ui/menus').show('load')").unwrap();
    frames(&mut g, 2);
    click(&mut g, "menu", "slot1", 1.5);
    frames(&mut g, 4);
    assert_eq!(chapter(&g), "The cellar");
    // A save holds the moment the current line appeared: the purse opened after it is closed again.
    assert_eq!(num(&g, "#require('game/log').entries"), entries - 1.0);
    assert!(truth(&g, "require('game/rules').has('coins')"));
    assert!(!truth(&g, "require('game/play').ended:peek()"));
    let errors: Vec<String> = g.eval("t", "return errors").unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    drop(g);

    // A phone in portrait: the portrait layout, the hero and pack sheets.
    let Some(r) = renderer(720, 1280) else { return };
    let phone = Device { form: FormFactor::Phone, touch: true, can_lock: true, ..Default::default() };
    let mut g = Game::with_device(Assets::dir(DIR), r, phone).unwrap_or_else(|e| panic!("{e}"));
    g.resize(720, 1280);
    g.eval::<()>("t", "errors = {}; kulisa.on('error', function(m) table.insert(errors, m) end)").unwrap();
    frames(&mut g, 4);
    assert_eq!(num(&g, "kulisa.screen.width"), 1080.0);
    shot(&mut g, "rpg-phone-title");
    click(&mut g, "menu", "new", 1.5);
    assert!(read_on(&mut g));
    shot(&mut g, "rpg-phone-play");
    // Choices are tapped.
    click(&mut g, "main", "choice1", 1.5);
    assert!(read_on(&mut g));
    assert!(truth(&g, "require('game/rules').has('herb')"));
    click(&mut g, "main", "pack_tab", 1.5);
    assert!(truth(&g, "kulisa.ui.find('menu', 'item:herb') ~= nil"));
    shot(&mut g, "rpg-phone-pack");
    click(&mut g, "menu", "close", 1.5);
    click(&mut g, "main", "hero_tab", 1.5);
    shot(&mut g, "rpg-phone-hero");
    let errors: Vec<String> = g.eval("t", "return errors").unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    drop(g);
    let _ = std::fs::remove_dir_all(&data);
}
