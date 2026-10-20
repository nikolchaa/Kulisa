# desktop-os

A story told through a fake computer: chats, a browser with a small fake web, notes and
notifications, on a desktop with draggable, resizable windows. Built only with the public Kulisa
API ([docs/API.md](../../docs/API.md)). Copy the folder to start a game, then make it yours.

```
cargo run -p kulisa-player -- templates/desktop-os
cargo xtask web --game templates/desktop-os --serve 8080
cargo xtask apk --game templates/desktop-os
```

## What is in it

- **Window manager** (`os/wm.luau`). Every window is a UI screen laid out in its own rectangle
  (`kulisa.ui.place`), raised with `kulisa.ui.set_z`. Title bars drag (`on_drag`) and maximize on a
  double-click; edges and corners resize with resize cursors; a press anywhere in a window focuses
  it (`on_press`); window bodies `block` the desktop below and `clip` their content. Minimize flies
  the window to its taskbar button. On phones windows open maximized.
- **Shell** (`os/shell.luau`): the wallpaper (a shader, `shaders/wallpaper.wgsl`), desktop icons
  (double-click or tap to open, drag to rearrange), a clock, the taskbar with running indicators
  and a notification bell, the start menu, notifications, story captions, an objective hint,
  dialogs, and the sign-in screen that doubles as the main menu.
- **Apps** (`apps/`): Messages, Browser (address bar with typing, back / forward, bookmarks,
  search), Notes and Settings (wallpapers, volume, display, save slots with thumbnails). An app is
  a module with `build(win)` returning widgets, and optionally `init`, `open(win, arg)`, `close`.
  Register new ones in `os/config.luau` and `main.luau`.
- **The fake web** (`web/pages.luau`): pages are tables of blocks (headings, text, images, links,
  forum posts, a search box) or functions that change with the story. Unknown addresses get a 404.

## Writing the story

`story/*.kulisa` is ordinary Kulisa story language; `os/story.luau` routes it:

```
define juno = Character("Juno", { chat = "juno", avatar = "avatars/juno.png" })
define me = Character("You", { me = true })

label start:
    "Narration shows as a caption the player clicks through."
    juno "chat lines arrive after a short 'typing…'"
    menu:
        "A menu after chat lines becomes reply buttons":
            me "and `me` lines are what the player sends"
    $ desk.wait_visit("lighthousecam.net", "Open lighthousecam.net")
    pause
```

- A contact's message waits until the player has seen it: if the chat is not open on that thread,
  a notification shows and the story holds.
- `desk` (os/story.luau) is the computer's API for `$` lines: `desk.open(app, arg)`,
  `desk.notify(title, text, app?)`, `desk.wait_visit(url, hint?)` and `desk.wait_open(app, hint?)`
  (then `pause`), `desk.hint(text)`, `desk.note(title, text)`, `desk.photo(thread, image)`,
  `desk.flag(name, value?)` (pages read `data.flags`), `desk.bookmark(url)`, `desk.clock(h, m)`,
  `desk.wallpaper(name)`.
- Saves keep the story plus the chats, notes, browsing history, open windows and clock
  (`kulisa.state.register` in main.luau).

## Controls

Mouse or touch for everything. Space or Enter continues a caption; Escape closes the start menu or
a dialog.

## Art

Original to this template: window chrome and panels (nine-slices), white glyphs that widgets tint,
app icons, avatars, the photos on the fake web pages and the UI sounds. Inter is under the SIL Open Font License
(`fonts/Inter-OFL.txt`).
