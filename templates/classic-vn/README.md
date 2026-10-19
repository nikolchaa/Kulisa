# classic-vn

A Ren'Py-style visual novel interface, built only with the public Kulisa API
([docs/API.md](../../docs/API.md)). Copy the folder to start a game, then make it yours.

```
cargo run -p kulisa-player -- templates/classic-vn
cargo xtask web --game templates/classic-vn --serve 8080
cargo xtask apk --game templates/classic-vn
```

## What is in it

- **Text box and name box** (`ui/say.luau`), typewriter speed from Preferences, a
  click-to-continue arrow. Laid out in the design area, so lines keep a readable length on
  ultrawide screens.
- **Choices** (`ui/choice.luau`); choices picked in an earlier playthrough are dimmed.
- **Quick menu** (`ui/quick.luau`): Back, History, Skip, Auto, Save, Q.Save, Q.Load, Prefs.
- **Game menu** (`ui/game_menu.luau`): save and load with thumbnails (pages 1-9, auto and
  quick saves), preferences, history. **Main menu** (`ui/main_menu.luau`).
- **Dialogs and overlays** (`ui/overlays.luau`): confirm, notifications, script errors, and a
  "turn your device" hint on the web when the orientation cannot be locked.
- **Transitions** for `with` (`game/config.luau`, `shaders/`): fade, dissolve, wipes and an
  image dissolve (`swirl`, from `masks/swirl.png`).

## Ren'Py behaviour

- **Skip**: Tab or the quick menu toggles it; Ctrl skips while held. It skips only text seen
  before (in any session) unless "Skip unseen text" is on, and stops at choices unless "After
  choices" is on. Transitions are instant while skipping.
- **Auto**: A or the quick menu; waits a little longer for longer lines.
- **Back** (rollback): wheel up, Page Up, the quick menu, or on touch screens a tap on the
  rollback side chosen in Preferences. Wheel down and Page Down roll forward. Rolling back to
  a menu lets you choose again. `config.rollback = false` turns it off; a story can stop
  rollback past a point with `$ kulisa.story.block_rollback()`.
- **Window auto**: the text box hides for `scene` and comes back after the transition, and
  lines wait for transitions, as with Ren'Py's `with`.
- **Keyboard and gamepad only**: arrow keys or the d-pad/left stick move through menus and
  choices, Enter or A picks (screens opt in with `nav = true`; modal screens always do). In the
  story: A advances, B or Start opens the game menu, X toggles auto, Y opens the history, LB
  rolls back, RB toggles skip (hold RT to skip), Back/Select hides the interface. In the game
  menu LB and RB change section and B closes it; Page Up/Down and the right stick scroll the
  history.
- Hide the interface with H or a middle click; open the game menu with Escape, a right
  click, a long press or Android's back button. Swipe down for the history.

## Images

`scene bg room` shows `images/bg room.png`, `show mila happy at left` shows
`images/mila happy.png` (the first word is the tag; showing `mila neutral` replaces it).
Missing images show a labelled placeholder. Positions are in `config.positions`.

The art here is original placeholder art made for the template; replace it.
Inter is under the SIL Open Font License (`fonts/Inter-OFL.txt`).
