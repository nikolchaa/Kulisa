# text-rpg

An illustrated text adventure: a picture for each scene, a scrolling log, numbered choices, a hero
panel with health and stats, an inventory you can use items from, d20 checks, and save slots.
Stories are `.kulisa` files; the rules are Luau. Built only with the public Kulisa API
([docs/API.md](../../docs/API.md)). Copy the folder to start a game, then make it yours.

```
cargo run -p kulisa-player -- templates/text-rpg
cargo xtask web --game templates/text-rpg --serve 8080
cargo xtask apk --game templates/text-rpg
```

## Layouts

- **Landscape** (desktops, tablets): the illustration and the log on the left, the hero and the
  pack on the right.
- **Phones** play in **portrait** (a `phone` display profile with a 1080×1920 design): the
  illustration on top, the log below, and the hero, pack and menu in sheets. Try it on desktop with
  `KULISA_PROFILE=phone` in a tall window, or `?profile=phone` on the web.

## Writing stories

```
define mara = Character("Old Mara", { color = "#e8b48a" })
default hp = 10
default bag = { lantern = 1 }

label start:
    scene crossroads                      # shows images/crossroads.png
    "Narration goes into the log."
    mara "So do lines, with the speaker's name."
    menu:
        "Force the door. (Might)":
            if roll("might", 11):         # d20 + Might against 11, written to the log
                "It gives."
            else:
                $ hurt(2)
        "Use the key." if has("key"):
            $ take("key")
    $ give("herb")
```

- Rules (`game/rules.luau`): `give(item, n?)`, `take(item, n?)`, `has(item, n?)`, `heal(n)`,
  `hurt(n)`, `pay(n)` (negative earns), `roll(stat, target)`. They change story variables, so
  saves keep health, gold, stats and the pack.
- Items (`game/items.luau`): name, icon, description, an optional `use(rules)` for the pack's Use
  button, and stat bonuses while carried (`might = 1`).
- Stats shown on the hero panel are listed in `game/config.luau`.
- Saves hold the moment the current line appeared (as in Ren'Py), plus the log.

## Controls

Click or tap the log, Space or Enter to continue; 1-9 pick a choice (or arrows / d-pad, then Enter
or A). F5 quick save, F9 quick load, Escape opens the menu.

## Art

Original to this template: the scene illustrations, item icons, the portrait, the die and the ornamented panels. Inter is under the SIL
Open Font License (`fonts/Inter-OFL.txt`); a serif face would suit the genre, drop one into
`fonts/` and change `config.font`.
