//! Plays `templates/desktop-os` headless: sign in, read and answer chats, browse the fake web
//! (typing an address, searching, following links), drag a window, save and load, to the end.

use kulisa_core::Vec2;
use kulisa_lua::Game;
use kulisa_platform::Assets;
use kulisa_render::Renderer;

fn game() -> Option<Game> {
    let data = std::env::temp_dir().join(format!("kulisa-desktop-os-test-{}", std::process::id()));
    // SAFETY: this binary has one test, so nothing else reads the environment meanwhile.
    unsafe { std::env::set_var("KULISA_DATA_DIR", &data) };
    let r = match pollster::block_on(Renderer::headless(1280, 720)) {
        Ok(r) => r,
        Err(e) if std::env::var_os("KULISA_REQUIRE_GPU").is_some() => panic!("no GPU adapter: {e}"),
        Err(e) => {
            eprintln!("skipping: {e}");
            return None;
        }
    };
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../templates/desktop-os");
    let mut g = Game::new(Assets::dir(dir), r).unwrap_or_else(|e| panic!("{e}"));
    g.resize(1280, 720);
    Some(g)
}

/// With `KULISA_SHOTS=<dir>`, saves what the window shows now as `<dir>/<name>.png`.
fn shot(g: &mut Game, name: &str) {
    let Some(dir) = std::env::var_os("KULISA_SHOTS") else { return };
    g.render();
    if let Some(img) = g.renderer().read_pixels() {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), img.encode_png().unwrap())
            .unwrap();
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

fn errors(g: &Game) -> Vec<String> {
    g.eval("t", "return errors").unwrap_or_default()
}

/// Runs frames until `cond` holds (fails after `max` frames).
fn until(g: &mut Game, cond: &str, max: usize) {
    for _ in 0..max {
        if truth(g, cond) {
            return;
        }
        g.frame(0.05);
    }
    let state: String = g
        .eval(
            "t",
            "local d = require('os/data'); local wm = require('os/wm'); local out = {}              for id, t in d.threads do table.insert(out, id .. ':' .. #t.messages .. '/' .. t.unread) end              return table.concat(out, ' ') .. ' focused=' .. tostring(wm.focused:peek()) .. ' screens=' ..              tostring(kulisa.ui.shown('win:chat1')) .. ' story_ended=' .. tostring(kulisa.story.ended())",
        )
        .unwrap_or_default();
    panic!("timed out waiting for `{cond}`; {state}; errors: {:?}", errors(g));
}

/// Window pixels of the middle of widget `id` on `screen` (the design is 1.5x the window).
fn center(g: &Game, screen: &str, id: &str) -> Vec2 {
    let (x, y, w, h): (f32, f32, f32, f32) = g
        .eval("t", &format!("return kulisa.ui.bounds('{screen}', '{id}')"))
        .unwrap_or_else(|e| panic!("{screen}/{id}: {e}"));
    Vec2::new(x + w / 2.0, y + h / 2.0) / 1.5
}

fn click(g: &mut Game, screen: &str, id: &str) {
    let p = center(g, screen, id);
    g.pointer_moved(Some(p));
    g.pressed("left");
    g.released("left");
    frames(g, 1);
}

fn key(g: &mut Game, k: &str) {
    g.key(k, true);
    g.key(k, false);
}

/// Types `text` into a field (clearing it first) and presses Enter.
fn type_into(g: &mut Game, screen: &str, id: &str, text: &str) {
    if screen.starts_with("win:browser") && !truth(g, "require('os/wm').is_focused('browser')") {
        click(g, "taskbar", "task:browser");
        frames(g, 2);
    }
    click(g, screen, id);
    key(g, "end");
    for _ in 0..60 {
        key(g, "backspace");
    }
    g.text_input(text);
    key(g, "enter");
    frames(g, 4);
}

fn focus_chat(g: &mut Game) {
    if !truth(g, "require('os/wm').is_focused('chat')") {
        click(g, "taskbar", "task:chat");
    }
}

/// Waits for reply chips in the chat and picks the first.
fn reply(g: &mut Game) {
    focus_chat(g);
    until(g, "require('os/story').replies:peek() ~= nil", 600);
    frames(g, 2);
    click(g, "win:chat1", "reply1");
    frames(g, 2);
}

#[test]
fn desktop_os_plays() {
    let Some(mut g) = game() else { return };
    g.eval::<()>("t", "errors = {}; kulisa.on('error', function(m) table.insert(errors, m) end)").unwrap();
    frames(&mut g, 4);
    assert!(truth(&g, "kulisa.ui.shown('signin') and not kulisa.ui.shown('taskbar')"));
    shot(&mut g, "desktop-signin");
    click(&mut g, "signin", "signin1");
    until(&mut g, "require('os/shell').caption_shown()", 100);
    assert!(truth(&g, "kulisa.ui.shown('desktop') and kulisa.ui.shown('taskbar')"));
    // Space finishes the caption's typewriter, then moves on.
    key(&mut g, "space");
    key(&mut g, "space");

    // Juno writes while nobody is looking: a notification, and the story waits.
    until(
        &mut g,
        "require('os/data').threads.juno ~= nil and require('os/data').threads.juno.unread > 0",
        200,
    );
    assert!(truth(&g, "#require('os/shell').history == 1"));
    frames(&mut g, 60);
    assert!(truth(&g, "#require('os/data').threads.juno.messages == 1"), "waits until read");
    // Open Messages from the taskbar; the rest arrives, then the question.
    click(&mut g, "taskbar", "task:chat");
    until(&mut g, "require('os/story').replies:peek() ~= nil", 600);
    frames(&mut g, 10);
    shot(&mut g, "desktop-chat");
    reply(&mut g);
    until(&mut g, "require('os/data').threads.juno.messages[4].from == 'me'", 100);

    // Open the browser with a double-click on its desktop icon and type the address.
    until(&mut g, "kulisa.ui.shown('hint')", 400);
    click(&mut g, "desktop", "icon:browser");
    click(&mut g, "desktop", "icon:browser");
    until(&mut g, "kulisa.ui.shown('win:browser2')", 20);
    type_into(&mut g, "win:browser2", "address", "lighthousecam.net");
    until(&mut g, "not kulisa.ui.shown('hint')", 40);
    assert!(truth(&g, "require('os/data').visited['lighthousecam.net']"));
    frames(&mut g, 10);
    shot(&mut g, "desktop-cam");

    // Drag the browser by its title bar.
    let before = center(&g, "win:browser2", "titlebar");
    g.pointer_moved(Some(before));
    g.pressed("left");
    for i in 1..=5 {
        g.pointer_moved(Some(before + Vec2::new(-12.0 * i as f32, 8.0 * i as f32)));
        frames(&mut g, 1);
    }
    g.released("left");
    frames(&mut g, 2);
    let after = center(&g, "win:browser2", "titlebar");
    assert!((after - before - Vec2::new(-60.0, 40.0)).length() < 2.0, "{before} -> {after}");

    reply(&mut g);
    // Search for the keeper, then follow the result.
    until(&mut g, "kulisa.ui.shown('hint')", 600);
    type_into(&mut g, "win:browser2", "address", "searchly.com");
    type_into(&mut g, "win:browser2", "search", "keeper missing");
    shot(&mut g, "desktop-search");
    click(&mut g, "win:browser2", "link:coastnews.net/hale");
    frames(&mut g, 10);
    shot(&mut g, "desktop-news");
    until(&mut g, "not kulisa.ui.shown('hint')", 40);

    // Juno's answer arrives while the browser has the focus: read it, then find the forum thread
    // through the forum's front page.
    until(&mut g, "require('os/data').threads.juno.unread > 0", 200);
    focus_chat(&mut g);
    until(&mut g, "kulisa.ui.shown('hint')", 600);
    type_into(&mut g, "win:browser2", "address", "tidepool.forum");
    click(&mut g, "win:browser2", "link:tidepool.forum/t/gull-point");
    frames(&mut g, 10);
    shot(&mut g, "desktop-forum");
    until(&mut g, "not kulisa.ui.shown('hint')", 40);

    // A stranger writes in a new thread.
    until(&mut g, "require('os/data').threads.gull ~= nil", 400);
    focus_chat(&mut g);
    click(&mut g, "win:chat1", "thread:gull");
    reply(&mut g);
    until(&mut g, "#require('os/data').notes == 1", 400);
    // Juno again, in her thread.
    until(&mut g, "require('os/data').threads.juno.unread > 0", 400);
    click(&mut g, "win:chat1", "thread:juno");
    until(&mut g, "kulisa.ui.shown('hint')", 600);
    assert!(truth(&g, "require('os/data').flags.cam_lit"));
    type_into(&mut g, "win:browser2", "address", "lighthousecam.net");
    until(&mut g, "require('os/shell').caption_shown()", 100);
    key(&mut g, "space");
    key(&mut g, "space");
    reply(&mut g);

    // A photo from an unknown number; save while it waits to be read.
    until(
        &mut g,
        "require('os/data').threads.unknown ~= nil and #require('os/data').threads.unknown.messages >= 2",
        600,
    );
    g.eval::<()>("t", "require('apps/settings').save_slot('slot1')").unwrap();
    let saved = num(&g, "#require('os/data').threads.juno.messages");
    click(&mut g, "win:chat1", "thread:unknown");
    until(&mut g, "require('os/data').threads.juno.unread > 0", 400);
    click(&mut g, "win:chat1", "thread:juno");
    until(&mut g, "require('os/shell').caption_shown()", 600);
    key(&mut g, "space");
    key(&mut g, "space");
    until(&mut g, "kulisa.story.ended()", 100);
    until(&mut g, "kulisa.ui.shown('dialog')", 100);
    frames(&mut g, 10);
    shot(&mut g, "desktop-end");
    g.eval::<()>("t", "require('os/shell').toggle_start()").unwrap();
    frames(&mut g, 10);
    shot(&mut g, "desktop-start");
    g.eval::<()>("t", "require('os/shell').toggle_start(); require('os/wm').open('settings', 'saves')")
        .unwrap();
    frames(&mut g, 10);
    shot(&mut g, "desktop-settings");
    g.eval::<()>("t", "require('os/wm').open('notes')").unwrap();
    frames(&mut g, 10);
    shot(&mut g, "desktop-notes");
    assert!(num(&g, "#require('os/data').threads.juno.messages") > saved);
    click(&mut g, "dialog", "dialog1");

    // Load the save: the chats and windows are as they were.
    g.eval::<()>("t", "require('apps/settings').load_slot('slot1')").unwrap();
    frames(&mut g, 2);
    click(&mut g, "dialog", "dialog2");
    frames(&mut g, 4);
    assert_eq!(num(&g, "#require('os/data').threads.juno.messages"), saved);
    assert!(truth(&g, "require('os/wm').find('chat') ~= nil and require('os/wm').find('browser') ~= nil"));
    assert!(!truth(&g, "kulisa.story.ended()"));
    frames(&mut g, 20);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
    let id: String = g.eval("t", "return kulisa.config.game.id").unwrap();
    drop(g);
    let _ = std::fs::remove_dir_all(kulisa_platform::data_dir(&id).unwrap());
}
