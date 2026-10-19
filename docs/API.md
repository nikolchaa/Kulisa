# Kulisa Luau API

A game is a folder (or a `.kpak` packed from one) with a `main.luau` at its root. The
engine runs `main.luau` once at startup; everything else happens in event handlers and
tasks. `demos/smoke` uses most of this page.

Coordinates are design units, y down (see `kulisa.display`). Colors are strings like
`"#ff8800"` or `"#ff880080"`, or `{ r, g, b, a }` in 0..1 (sRGB). Paths are relative to the
game root and use `/`.

Errors are strict: unknown properties, bad values and typos in event names raise errors that
name the problem. Uncaught errors in handlers and tasks are logged and passed to
`kulisa.on("error", fn)`; the game keeps running.

## Sandbox

Scripts run in Luau's sandbox: no `io`, `os.execute` or `loadfile`; library tables and the
`kulisa` table are read-only. Game globals are fine. `print` logs through the engine.

- `require("lib/util")` loads `lib/util.luau` from the game once and returns what it
  returns. Cycles are reported.
- `kulisa.version`, `kulisa.platform` (`"windows"`, `"linux"`, `"macos"`, `"web"`,
  `"android"`, `"ios"`).
- `kulisa.log(...)`, `kulisa.now()` (seconds since start).

## Display

- `kulisa.display(width, height, mode?)`: the design resolution and scale mode: `"expand"`
  (default; like fit, but wider or taller windows see more), `"fit"` (letterboxed) or
  `"fill"` (cropped). Default 1920×1080 expand.
- `kulisa.screen`: `width`, `height`, `mode`, `scale` (window pixels per design unit),
  `clear` (background color), `effects` (screen post-processing, see Shaders),
  `screen:visible()` → `x, y, width, height` of the design area the window shows (in `"fit"`
  mode, the design area itself).
- `screen:capture(width, height, image?)` → an image of the screen as it is now, drawn as if
  the window were `width`×`height` (save thumbnails). Passing an earlier capture of the same
  size reuses it.

### Transitions

A transition draws a frozen frame of the screen over the live scene and animates it away
with a shader.

- `screen:freeze()` captures the screen as it is now.
- `screen:transition{ shader, time = 0.5, ease = "linear", mask, params }` → a handle with
  `done` and `skip()`. `shader` is a shader or a path. Its `tex` is the frozen frame and
  `params.p[0].x` the progress from 0 to 1; `params` (up to 15 numbers) fill the rest, from
  `params.p[0].y` on. `mask` is an image the shader reads as `mask_tex` (image wipes). A new
  transition ends the running one. Without a freeze, it starts from the current screen.

Story `with` is frozen for you, as in Ren'Py: before a statement with its own `with`
(`scene bg with fade`), so only that statement's change transitions; and at the last
interaction (or transition) before a lone `with` line, so everything since then does. The
`stage` handler starts the transition when the event has `with` (see `templates/classic-vn`).

- `kulisa.camera`: `x`, `y` (the point shown at the design center), `zoom`, `rotation`,
  `shake_x`, `shake_y`; `camera:tween(props, opts)` with `x`, `y`, `zoom`, `rotation`;
  `camera:reset()`.

## Nodes

Constructors take one options table: `parent` (a node; default the scene root),
`center = true` (origin at the middle), and any property below.

```lua
local ui = kulisa.layer({ parallax = 0, z = 10 })
local box = kulisa.sprite({ parent = ui, x = 40, y = 40, width = 200, height = 80, tint = "#111116" })
local label = kulisa.text({ parent = box, x = 16, y = 20, text = "Hello {b}world{/b}", size = 28 })
```

| Constructor | Makes |
|---|---|
| `kulisa.group(opts)` | An empty node for grouping and clipping. |
| `kulisa.sprite(opts)` | An image (`image`: an image or path). Without one, a 100×100 white rectangle to tint. Size defaults to the image size. |
| `kulisa.text(opts)` | Rich text (`text`, `font`, `size`, `line_height`, `color`, `wrap` width, `align`: `left`/`center`/`right`/`justified`, `cps` typewriter speed (0 = instant, the default), `fade`). |
| `kulisa.layer(opts)` | Content that moves with the camera by `parallax` (1 = world, 0 = fixed to the screen), with optional `effects`. |
| `kulisa.target(opts)` | Renders its children into a `width`×`height` texture (`clear`, `effects`); show it with a sprite whose `image` is `target.texture`. |

Properties (read and write unless noted):

| Property | Nodes | Meaning |
|---|---|---|
| `x`, `y`, `rotation`, `scale`, `scale_x`, `scale_y`, `origin_x`, `origin_y` | all | Transform relative to the parent; rotation in radians. |
| `z` | all | Draw order among siblings; negative children draw behind their parent. |
| `opacity`, `visible`, `tint`, `blend` | all | `blend`: `alpha`, `add`, `multiply`, `screen`. |
| `clip` | all | `{ x, y, width, height }` in local units, or nil. |
| `shader`, `params` | all | A shader for this node and up to 16 numbers for it. |
| `mask` | all, after `shader` | A second image for the shader (`mask_tex`), e.g. a wipe mask. |
| `slice`, `slice_scale` | sprites | Nine-slice borders in image pixels: one number for all four or `{ left, top, right, bottom }`. Corners keep their size (times `slice_scale`, default 1), edges and the middle stretch. |
| `parent` | all | Reparents (nil = the root). |
| `kind` | all, read-only | `group`, `sprite`, `text`, `layer`, `target`. |
| `width`, `height` | sprites (read on all) | Size in local units. |
| `image` | sprites | An image or path. Changing it keeps the size. |
| `parallax`, `effects` | layers (`effects` also targets) | See constructors. |
| `clear`, `texture` (read-only) | targets | See constructors. |
| `text`, `font`, `size`, `line_height`, `color`, `wrap`, `align`, `cps`, `fade` | text | Setting `text` restarts the typewriter. |
| `complete`, `duration` (read-only) | text | Whether the typewriter finished; its total time. |

Methods: `node:remove()`, `node:exists()`, `node:add(child)`, `node:children()`,
`node:contains(x, y)` (design point inside the node, after all transforms, camera and
parallax), `node:tween(props, opts)`, and for text `node:skip()`, `node:restart()`.

Text markup: `{b}`, `{i}`, `{color=#hex}`, `{size=N}`, `{font=Name}`, `{wave}`, `{shake}`,
`{ruby=reading}`, `{speed=factor}` (typewriter speed), `{w=secs}` (typewriter pause); `{{` is a literal brace.

## Tweens and tasks

`node:tween(props, opts)` animates `x`, `y`, `rotation`, `scale`, `scale_x`, `scale_y`,
`opacity` and `tint` to the given values. `opts` is a duration in seconds or
`{ time, ease, delay, repeat, yoyo }`; `repeat = -1` repeats forever. Eases: `linear`,
`in_quad`, `out_quad`, `in_out_quad`, `in_cubic`, `out_cubic`, `in_out_cubic`, `in_sine`,
`out_sine`, `in_out_sine`, `in_back`, `out_back`, `in_out_back`, `out_elastic`,
`out_bounce`. A new tween takes over the properties it animates from older ones. The
returned handle has `done` and `cancel()`.

Tasks are coroutines the engine resumes every frame:

```lua
kulisa.spawn(function()
    kulisa.await(logo:tween({ opacity = 1 }, { time = 0.5, ease = "out_cubic" }))
    kulisa.wait(2)
    logo:tween({ opacity = 0 }, 0.5)
end)
```

- `kulisa.spawn(fn, ...)` → `ok, err`. `main.luau` itself runs as a task.
- `kulisa.wait(secs)`; `kulisa.await(tween_or_fn)` waits for `handle.done` or until
  `fn()` returns true. Only inside tasks.

## Events and input

`kulisa.on(event, fn)` → id; `kulisa.off(id)`.

| Event | Arguments |
|---|---|
| `update` | `dt` |
| `press`, `release` | `x, y, button` (`left`, `right`, `middle`, `touch`), design units |
| `move` | `x, y` |
| `key`, `key_up` | name: `space`, `enter`, `escape`, `left`, `right`, `up`, `down`, `pageup`, `pagedown`, `f1`…`f12`, `back`, or the character (`a`, `1`) |
| `wheel` | lines; positive is up |
| `resize` | window `width, height` in pixels |
| `error` | message |
| `say`, `menu`, `pause`, `end`, `stage`, `restore` | see Story |

`kulisa.input.pointer()` → `x, y` or nil; `kulisa.input.down(key)` → whether a key is held.

## Assets

- `kulisa.load_image(path)` → image (`width`, `height`). PNG, JPEG, WebP. Cached.
- `kulisa.load_shader(path)` → shader. Cached.
- `kulisa.load_font(path)` → the family names it added (use them as `font`). The engine
  bundles no fonts.
- `kulisa.read_text(path)`, `kulisa.exists(path)`, `kulisa.list(dir)`.

### Shaders

WGSL defining `fn fragment(in: VertexOut) -> vec4<f32>` (premultiplied linear color),
optionally `fn vertex(in: VertexIn) -> VertexOut`. The engine provides `tex`, `samp`,
`mask_tex`, `mask_samp` (the node's `mask`, white without one), `globals.time`,
`globals.resolution` and `params.p[0..4]` (the 16 numbers as four `vec4`s). See
`demos/smoke/shaders`. `effects` lists are `{ shader, ... }` or
`{ { shader = s, params = { ... }, mask = image }, ... }` and run in order.

## Audio

Channels: `music` and `voice` (one sound at a time, crossfading) and `sound` (overlapping).

- `kulisa.audio.play(channel, path, { loop, volume, fade, crossfade })`. WAV, OGG, MP3,
  FLAC.
- `kulisa.audio.stop(channel, fade?)`, `kulisa.audio.playing(channel)`.
- `kulisa.audio.volume(channel, v?, fade?)` reads or sets a channel's volume; channel
  `"master"` is the whole mix.

On the web, audio starts with the player's first click, tap or key; earlier requests for
looping music start then.

## Story

- `kulisa.story.load(files?)` compiles the given `.kulisa` files (default: every `.kulisa`
  file in the game). Errors name the file, line and column.
- `kulisa.story.start(label?)` runs from `label` (default `start`).
- `kulisa.story.next()` continues past the current line or pause; false while a menu waits.
- `kulisa.story.choose(index)` picks a menu item by its `index`.
- `kulisa.story.rollback()`, `kulisa.story.roll_forward()` → whether it moved;
  `kulisa.story.can_rollback()`, `kulisa.story.can_roll_forward()`, `kulisa.story.ended()`.
- `kulisa.story.block_rollback()`: the player cannot roll back past the current line.
- `kulisa.story.seen()`: whether the current line had been seen before, in any session (for
  "skip seen text"). Seen lines are kept with the persistent data (needs `kulisa.save.setup`).
- `kulisa.story.vars()` (the story variables table), `kulisa.story.define(name, value)`
  (a constant stories can use), `kulisa.story.stage()`.

The engine never draws the story. It calls handlers, and the game decides what they look
like:

| Event | Arguments |
|---|---|
| `say` | `{ text, who, seen }`; `who` is the `Character` table (`name`, `color`, ...) plus `id`; `seen` whether the player saw this line before |
| `menu` | `items` (`{ index, text, chosen }`, hidden choices left out; `chosen` if the player picked it before), `caption` (a line or nil) |
| `pause` | seconds, or nil to wait for `next()` |
| `stage` | `{ kind = "scene" \| "show" \| "hide" \| "with", image, at, tag, with }` |
| `end` | |
| `restore` | the stage `{ scene, shown = { { image, at } }, tracks }` after rollback or load; rebuild visuals from it. The current line follows as a `say` or `menu`. |

`play` and `stop` lines are handled by the engine (asset paths, `fadein`/`fadeout`).

## State and saves

- `kulisa.state.register(name, { save = fn, load = fn })`: script state that rolls back
  and saves with the story. `save()` returns plain data (nil, booleans, numbers, strings,
  tables); `load(data)` restores it (`data` is nil for state a save does not have).
- `kulisa.save.setup({ id, version })`: the game's storage id (letters, digits, `_`, `-`)
  and save data version.
- `kulisa.save.write(slot, { title?, data?, thumbnail? })`: saves the story, registered state
  and optional extra `data`. The title defaults to the current line. `thumbnail` is an image
  from `screen:capture`.
- `kulisa.save.load(slot)` → the save's `data`; restores the story (`restore` follows).
- `kulisa.save.list()` → `{ slot, title, timestamp, playtime, version, thumbnail }` per save
  (`thumbnail` is an image, or nil);
  `kulisa.save.exists(slot)`, `kulisa.save.delete(slot)`.
- `kulisa.save.migrate(from, fn)`: upgrades saves from version `from` to `from + 1`. `fn`
  gets the save body (`{ story, data }`) and changes it in place or returns a new one.
  Saves from a newer version are refused.

- `kulisa.persist.read(name)`, `kulisa.persist.write(name, data)`: plain data that is not
  part of any save and does not roll back (settings, unlocks, endings seen). Needs
  `kulisa.save.setup`.

Saves live in the platform's app data folder (`%APPDATA%\<id>\saves` on Windows,
`localStorage` on the web, internal storage on Android).
