//! A whole Ren'Py project on disk: scripts, images named the Ren'Py way, audio found by name.

use std::path::{Path, PathBuf};

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kulisa-renpy-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files: &[(&str, &[u8])] = &[
        (
            "game/options.rpy",
            b"define config.name = _(\"Harbor\")\ndefine config.version = \"2.0\"\ndefine config.window_icon = \"gui/window_icon.png\"\n",
        ),
        ("game/gui/window_icon.png", b"png"),
        ("game/gui.rpy", b"init python:\n    gui.init(1280, 720)\nscreen say(who, what):\n    text what\n"),
        (
            "game/script.rpy",
            b"define h = Character(\"Hana\")\nimage bg pier = \"art/Pier Night.jpg\"\n\nlabel start:\n    scene bg pier\n    show hana smile\n    play music harbor\n    h \"Hello.\"\n    return\n",
        ),
        ("game/chapters/two.rpy", b"label two:\n    \"Later.\"\n    return\n"),
        ("game/images/sprites/Hana Smile.png", b"png"),
        ("game/images/bg sea.webp", b"webp"),
        ("game/images/notes.txt", b"not an image"),
        ("game/art/Pier Night.jpg", b"jpg"),
        ("game/audio/harbor.ogg", b"ogg"),
        ("game/tl/french/script.rpy", b"translate french start:\n    pass\n"),
    ];
    for (path, bytes) in files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    dir
}

fn dest(copies: &[(PathBuf, String)], to: &str) -> Option<PathBuf> {
    copies.iter().find(|(_, d)| d == to).map(|(s, _)| s.clone())
}

#[test]
fn imports_a_project() {
    let dir = project();
    let imported = kulisa_renpy::import(&dir).unwrap();
    let names: Vec<&str> = imported.stories.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        names,
        ["story/chapters/two.kulisa", "story/script.kulisa"],
        "translations and empty files are skipped"
    );
    let script = &imported.stories[1].1;
    assert!(script.contains("play music \"audio/harbor.ogg\""), "{script}");

    let game = Path::new(&dir).join("game");
    let copies = &imported.copies;
    assert_eq!(dest(copies, "images/hana smile.png"), Some(game.join("images/sprites/Hana Smile.png")));
    assert_eq!(dest(copies, "images/bg sea.webp"), Some(game.join("images/bg sea.webp")));
    assert_eq!(dest(copies, "images/bg pier.jpg"), Some(game.join("art/Pier Night.jpg")));
    assert_eq!(dest(copies, "audio/harbor.ogg"), Some(game.join("audio/harbor.ogg")));
    assert_eq!(dest(copies, "icon.png"), Some(game.join("gui/window_icon.png")));
    assert_eq!(copies.len(), 5, "{copies:?}");

    assert_eq!(imported.info.title.as_deref(), Some("Harbor"));
    assert_eq!(imported.info.version.as_deref(), Some("2.0"));
    assert_eq!(imported.info.size, Some((1280, 720)));
    assert!(imported.notes.iter().any(|n| n.file == "gui.rpy" && n.message.contains("screen `say`")));

    let sources: Vec<(&str, &str)> = imported.stories.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
    kulisa_story::compile(&sources).unwrap_or_else(|e| panic!("{e}"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refuses_folders_without_scripts() {
    let dir = std::env::temp_dir().join(format!("kulisa-renpy-empty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(kulisa_renpy::import(&dir).unwrap_err().contains("no .rpy scripts"));
    let _ = std::fs::remove_dir_all(&dir);
}
