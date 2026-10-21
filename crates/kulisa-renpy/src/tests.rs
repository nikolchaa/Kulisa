use super::*;

const SCRIPT: &str = r##"# The script of the game.

define e = Character("Eileen", color="#c8ffc8", image="eileen")
define narrator_voice = Character(None, kind=nvl)
define audio.theme = "music/theme.ogg"
default points = 0
default met = False

image bg park = "backgrounds/park_day.jpg"
image eileen wave:
    "eileen happy.png"
    pause 0.5
    repeat

screen hud():
    text "[points]"

init python:
    def helper():
        return 1

label start:
    scene bg park
    with fade
    play music theme fadein 2.0
    show eileen happy at right with dissolve
    e "Hi! You must be new here." # a comment
    e concerned "{b}Careful{/b}, the path is {i}steep{/i}.{w} Really."
    extend " Very steep."
    "Sam" "I'll be fine, [player_name!t]."
    $ points += 1
    $ met = True
    $ renpy.pause(0.5)
    window hide
    menu:
        e "Where to?"
        "The lake" if points > 0:
            jump lake
        "The hill":
            $ points -= 1
            call hill from _call_hill
        "Stay" if 'x' in inventory:
            pass
    if points >= 1 and not met:
        e "Nice."
    elif points == 0:
        e "Hm."
    else:
        $ inventory.append("rock")
    while points < 3:
        $ points += 1
    hide eileen with moveoutright
    stop music fadeout 1.0
    return

label lake:
    scene bg lake with Dissolve(1.0)
    play sound "sfx/splash.ogg"
    voice "voice/e001.ogg"
    e "The water is cold."
    with vpunch
    pause 1.0
    jump start

label hill:
    python:
        points += 2
        visited_hill = True
    return
"##;

fn converter() -> Converter {
    let files = ["music/theme.ogg", "audio/sfx/splash.ogg", "voice/e001.ogg"].map(String::from);
    let mut c = Converter::new(files.into_iter().collect());
    c.prescan(SCRIPT);
    c
}

#[test]
fn converts_a_script() {
    let mut c = converter();
    let out = c.convert("script.rpy", SCRIPT);
    let expected = r##"# The script of the game.
define e = Character("Eileen", { color = "#c8ffc8", image = "eileen" })
define narrator_voice = Character("", { kind = nvl })
default points = 0
default met = false
# Ren'Py: init python:
#    def helper():
#        return 1

label start:
    scene bg park
    with fade
    play music "music/theme.ogg" fadein 2
    show eileen happy at right with dissolve
    e "Hi! You must be new here."
    show eileen concerned
    e "{b}Careful{/b}, the path is {i}steep{/i}.{w=0.5} Really."
    e " Very steep."
    speaker_sam "I'll be fine, [player_name]."
    $ points += 1
    $ met = true
    pause 0.5
    menu:
        e "Where to?"
        "The lake" if points > 0:
            jump lake
        "The hill":
            $ points -= 1
            call hill
        "Stay":
            pass
    if points >= 1 and not met:
        e "Nice."
    elif points == 0:
        e "Hm."
    else:
        # Ren'Py: $ inventory.append("rock")
        pass
    # Ren'Py: while points < 3:
    #    $ points += 1
    hide eileen with dissolve
    stop music fadeout 1
    return

label lake:
    scene bg lake with dissolve
    play sound "audio/sfx/splash.ogg"
    play voice "voice/e001.ogg"
    e "The water is cold."
    pause 1
    jump start

label hill:
    lua:
        points += 2
        visited_hill = true
    return
"##;
    assert_eq!(out, expected);
    assert_eq!(
        c.speakers_file().unwrap(),
        "# Speakers the Ren'Py script named with strings.\ndefine speaker_sam = Character(\"Sam\")\n"
    );
    assert_eq!(
        c.images,
        [("bg park".to_string(), "backgrounds/park_day.jpg".to_string(), "script.rpy".to_string(), 9)]
    );
    assert_eq!(
        c.audio.iter().collect::<Vec<_>>(),
        ["audio/sfx/splash.ogg", "music/theme.ogg", "voice/e001.ogg"]
    );

    // The result compiles as a Kulisa story.
    let speakers = c.speakers_file().unwrap();
    let program = kulisa_story::compile(&[("script.kulisa", &out), ("speakers.kulisa", &speakers)]);
    assert!(program.is_ok(), "{}", program.err().unwrap());

    let notes: Vec<String> = c.notes.iter().map(|n| format!("{}: {}", n.line, n.message)).collect();
    for expected in [
        "10: image `eileen wave` uses ATL animation",
        "15: screen `hud` was left out",
        "18: Python code is not converted",
        "28: `{w}` (wait for a click)",
        "29: `extend` became a new line",
        "34: `window hide` was dropped",
        "42: choice condition `'x' in inventory`: `in` expressions are not converted",
        "49: `$` line not converted",
        "50: `while` loops are not converted",
        "52: transition `moveoutright` became `dissolve`",
        "61: `with vpunch` (a screen shake) was dropped",
    ] {
        assert!(notes.iter().any(|n| n.starts_with(expected)), "missing note `{expected}` in {notes:#?}");
    }
}

#[test]
fn reads_game_info() {
    let mut c = Converter::default();
    c.prescan("define config.name = _(\"Lake Days\")\ndefine config.version = \"1.2\"\n");
    c.prescan("init python:\n    gui.init(1280, 720)\n");
    assert_eq!(c.info.title.as_deref(), Some("Lake Days"));
    assert_eq!(c.info.version.as_deref(), Some("1.2"));
    assert_eq!(c.info.size, Some((1280, 720)));
}

#[test]
fn logical_lines_join_brackets_and_strip_comments() {
    let src = "define e = Character(\n    \"Eileen\",  # name\n    color=\"#fff\")\n\ne \"a # not a comment\" # comment\n\"\"\"Long\ntext\"\"\"\n";
    let lines = logical_lines(src);
    assert_eq!(lines[0].text, "define e = Character( \"Eileen\", color=\"#fff\")");
    assert_eq!(lines[1].text, "e \"a # not a comment\"");
    assert_eq!((lines[2].no, lines[2].text.as_str()), (6, "\"\"\"Long\ntext\"\"\""));
}

#[test]
fn empty_blocks_get_pass_and_menus_need_choices() {
    let mut c = Converter::default();
    let out = c.convert("t.rpy", "label start:\n    window show\n    menu:\n        \"Only text\"\n");
    assert_eq!(out, "label start:\n    # Ren'Py: menu:\n    pass\n");
}
