//! Videos in a running game: the picture reaches the screen, stories wait for cutscenes.
//! The clip is red for a second and blue for a second.
//!
//! Without a GPU (or software adapter) the tests skip, unless `KULISA_REQUIRE_GPU` is set.

#![cfg(any(windows, target_os = "macos", target_os = "linux"))]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use kulisa_lua::Game;
use kulisa_platform::Assets;
use kulisa_render::Renderer;

fn data(name: &str) -> Vec<u8> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kulisa-video/tests/data");
    std::fs::read(dir.join(name)).unwrap()
}

fn folder(files: &[(&str, &[u8])]) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kulisa-video-test-{}-{}",
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

fn game(main: &str, story: &str) -> Option<Game> {
    let renderer = match pollster::block_on(Renderer::headless(64, 36)) {
        Ok(r) => r,
        Err(e) if std::env::var_os("KULISA_REQUIRE_GPU").is_some() => panic!("no GPU adapter: {e}"),
        Err(e) => {
            eprintln!("skipping: {e}");
            return None;
        }
    };
    let dir = folder(&[
        ("main.luau", main.as_bytes()),
        ("story/a.kulisa", story.as_bytes()),
        ("movies/clip.mp4", &data("clip.mp4")),
        ("movies/clip.ivf", &data("clip.ivf")),
        ("movies/clip.ogg", &data("clip.ogg")),
    ]);
    Some(Game::new(Assets::dir(dir), renderer).unwrap())
}

/// Runs frames in real time until `done` says so.
fn run_until(g: &mut Game, what: &str, mut done: impl FnMut(&mut Game) -> bool) {
    let start = Instant::now();
    while !done(g) {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
        g.frame(0.01);
    }
}

fn centre(g: &mut Game) -> [u8; 4] {
    g.render();
    let image = g.renderer().read_pixels().unwrap();
    let at = (image.height as usize / 2 * image.width as usize + image.width as usize / 2) * 4;
    image.pixels[at..at + 4].try_into().unwrap()
}

fn is_red(px: [u8; 4]) -> bool {
    px[0] > 200 && px[2] < 60
}

fn is_blue(px: [u8; 4]) -> bool {
    px[2] > 200 && px[0] < 60
}

#[test]
fn a_sprite_shows_the_video_as_it_plays() {
    let Some(mut g) = game(
        r#"
        kulisa.display(64, 36, "fit")
        v = kulisa.video.play("movies/clip.mp4", { channel = false })
        s = kulisa.sprite({ image = v.image, width = 64, height = 36 })
        "#,
        "label start:\n    \"x\"\n",
    ) else {
        return;
    };
    run_until(&mut g, "the video to open", |g| g.eval::<bool>("t", "return v.ready").unwrap());
    assert_eq!(g.eval::<(u32, u32)>("t", "return v.width, v.height").unwrap(), (160, 90));
    assert!((g.eval::<f64>("t", "return v.duration").unwrap() - 2.0).abs() < 0.1);

    run_until(&mut g, "a red picture", |g| is_red(centre(g)));
    assert_eq!(g.eval::<u32>("t", "return v.image.width").unwrap(), 160, "the image took the video's size");
    run_until(&mut g, "a blue picture", |g| is_blue(centre(g)));
    run_until(&mut g, "the end", |g| g.eval::<bool>("t", "return v.done").unwrap());
    assert!(g.eval::<bool>("t", "return v.error == nil").unwrap());

    // Back to the start from the end.
    g.eval::<()>("t", "v:seek(0)").unwrap();
    assert!(!g.eval::<bool>("t", "return v.done").unwrap());
    run_until(&mut g, "red again", |g| is_red(centre(g)));
    g.eval::<()>("t", "v:pause()").unwrap();
    let before = g.eval::<f64>("t", "return v.time").unwrap();
    for _ in 0..10 {
        g.frame(0.05);
    }
    assert_eq!(g.eval::<f64>("t", "return v.time").unwrap(), before, "pausing stops the clock");
    g.eval::<()>("t", "s:remove(); v:stop()").unwrap();
    assert!(g.eval::<bool>("t", "return v.done and v.image == nil").unwrap());
}

const STORY_GAME: &str = r#"
    kulisa.display(64, 36, "fit")
    kulisa.save.setup({ id = "video_test_" .. kulisa.now() })
    lines = {}
    played = {}
    kulisa.on("say", function(l) table.insert(lines, l.text) end)
    kulisa.on("movie", function(file)
        table.insert(played, file)
        cutscene = kulisa.video.cutscene(file, { channel = false, on_done = function() kulisa.story.next() end })
    end)
    kulisa.story.load()
    kulisa.story.start()
"#;

const STORY: &str = "label start:\n    \"before\"\n    movie \"movies/clip.mp4\"\n    \"after\"\n";

#[test]
fn a_story_waits_for_its_movie() {
    let Some(mut g) = game(STORY_GAME, STORY) else { return };
    assert_eq!(g.eval::<u32>("t", "return #lines").unwrap(), 1);
    assert!(g.eval::<bool>("t", "kulisa.story.next(); return played[1] == 'movies/clip.mp4'").unwrap());
    assert!(g.eval::<bool>("t", "return kulisa.video.in_cutscene()").unwrap());
    assert_eq!(g.eval::<u32>("t", "return #lines").unwrap(), 1, "the story waits");
    run_until(&mut g, "the picture", |g| is_red(centre(g)));
    run_until(&mut g, "the next line", |g| g.eval::<u32>("t", "return #lines").unwrap() == 2);
    assert!(!g.eval::<bool>("t", "return kulisa.video.in_cutscene()").unwrap());
    assert_eq!(g.eval::<String>("t", "return lines[2]").unwrap(), "after");
}

#[test]
fn a_cutscene_can_be_skipped_and_a_load_plays_it_again() {
    let Some(mut g) = game(STORY_GAME, STORY) else { return };
    g.eval::<()>("t", "kulisa.story.next()").unwrap();
    run_until(&mut g, "the video to open", |g| g.eval::<bool>("t", "return cutscene.video.ready").unwrap());
    g.eval::<()>("t", "kulisa.save.write('s')").unwrap();
    g.eval::<()>("t", "cutscene.skip()").unwrap();
    assert_eq!(g.eval::<u32>("t", "return #lines").unwrap(), 2, "skipping goes to the next line");
    assert!(g.eval::<bool>("t", "return cutscene.done").unwrap());

    g.eval::<()>("t", "kulisa.save.load('s')").unwrap();
    assert_eq!(g.eval::<u32>("t", "return #played").unwrap(), 2, "loading onto a movie plays it again");
    assert!(g.eval::<bool>("t", "return not cutscene.done").unwrap());
    g.eval::<()>("t", "kulisa.save.delete('s')").unwrap();
}

#[test]
fn an_unplayable_video_reports_and_a_cutscene_still_ends() {
    let Some(mut g) = game(
        r#"
        kulisa.display(64, 36, "fit")
        errors = {}
        kulisa.on("error", function(e) table.insert(errors, e) end)
        finished = false
        kulisa.video.cutscene("movies/none.mp4", { on_done = function() finished = true end })
        "#,
        "label start:\n    \"x\"\n",
    ) else {
        return;
    };
    run_until(&mut g, "the cutscene to end", |g| g.eval::<bool>("t", "return finished").unwrap());
    assert!(g.eval::<bool>("t", "return #errors == 1 and errors[1]:find('video') ~= nil").unwrap());
    assert!(g.eval::<bool>("t", "return not pcall(kulisa.video.play, '../up.mp4')").unwrap());
    assert!(g.eval::<bool>("t", "return not pcall(kulisa.video.play, 'a.mp4', { channel = 3 })").unwrap());
}

#[test]
fn packs_leave_videos_out() {
    let dir =
        folder(&[("main.luau", b"x"), ("movies/a.MP4", b"x"), ("movies/b.webm", b"x"), ("c.png", b"x")]);
    let pack = Assets::pack(kulisa_platform::build_pack(&dir).unwrap()).unwrap();
    assert_eq!(pack.list(""), ["c.png", "main.luau"]);
    let videos = kulisa_platform::game_videos(&dir).unwrap();
    assert_eq!(videos.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(), ["movies/a.MP4", "movies/b.webm"]);
}
