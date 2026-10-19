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
- `kulisa.display{ width, height, scale, aspect, orientation, phone = {...}, tablet = {...},
  desktop = {...} }`: the same with display profiles. `aspect = { "4:3", "21:9" }` limits
  the window's aspect ratio (bars beyond it; the scale mode applies inside), so 32:9 monitors
  do not show areas the art never painted. `orientation`: `"auto"` (default: follows the
  design's shape), `"landscape"`, `"portrait"` or `"any"`. Android and iOS lock to it; on the
  web the game should ask the player to rotate when `screen.mismatch` is set; desktop windows
  open in that shape. `phone`, `tablet` and `desktop` override any of these for that kind of
  device (a touch screen under 600 logical pixels on its short side is a phone). Call it
  before building the scene; the profile is chosen once, at startup.
- `kulisa.screen`: `width`, `height`, `mode`, `scale` (window pixels per design unit),
  `clear` (background color), `effects` (screen post-processing, see Shaders),
  `screen:visible()` → `x, y, width, height` of the design area the window shows (in `"fit"`
  mode, the design area itself), `screen:safe()` → the same minus notches and system bars,
  `profile` (`"desktop"`, `"tablet"`, `"phone"`), `orientation` (`"landscape"` or
  `"portrait"`, the window's shape now), `mismatch` (the device should be rotated), `touch`.
  The `display` event (`{ profile, orientation, mismatch }`) fires when these change.
- `kulisa.window`: `fullscreen` (read and write; desktop and web), `can_quit`, `window:quit()`
  (desktop only; mobile apps and pages are closed by the player).
- Testing: `KULISA_PROFILE=phone` and `KULISA_SAFE_AREA=left,top,right,bottom` (pixels) force
  a profile and insets on desktop; on the web, `?profile=phone&safe=0,40,0,30`.
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
`node:contains(x, y, alpha?)` (design point inside the node, after all transforms, camera and
parallax; see Hit testing), `node:tween(props, opts)`, and for text `node:skip()`,
`node:restart()`.

### Hit testing

- `node:contains(x, y, alpha?)`: whether the design-space point is on the node's drawn area:
  a sprite's rectangle, a mesh's triangles (so deformed meshes are hit where they are drawn),
  text glyph boxes. Groups and layers have no area. With `alpha` (a number from 0 to 1, or
  `true` for 0.5) the pixel under the point must be at least that opaque: pixel-perfect clicks
  on irregular sprites. Alpha comes from the image file the sprite or mesh uses, decoded on the
  first test; images that are not files (captures, render targets, text glyphs) count as
  opaque.
- `kulisa.pick_all(x, y, { root?, alpha? })` → the nodes under the point, topmost first (the
  reverse of draw order). Hidden and fully transparent nodes (and their children), points
  outside a node's `clip`, and render-target contents are skipped. `root` limits the search to
  that node's subtree.
- `kulisa.pick(x, y, { root?, alpha?, filter? })` → the topmost node, or nil. `filter(node)`
  can reject nodes (return false) to look through them.

### Meshes

`kulisa.mesh(opts)` makes a node drawn as a triangle mesh whose vertices you edit every
frame: cloth, peeling film, bendy sprites. It batches with everything else and takes `image`,
`tint`, `shader`, `blend` and the other node properties.

```lua
-- A grid of 8x8 cells covering the image (vertices run row by row, numbered from 1).
local flag = kulisa.mesh{ image = "flag.png", cols = 8, rows = 8, x = 400, y = 200 }
kulisa.on("update", function()
    for r = 0, 8 do
        for c = 0, 8 do
            local i = flag:vertex_at(c, r)
            local x, y, u, v = flag:vertex(i)
            flag:set_vertex(i, x, y + math.sin(kulisa.now() * 4 + c) * 6 * (c / 8))
        end
    end
end)

-- Or raw geometry: `positions` (x, y pairs), optional `uvs` (u, v pairs, 0..1 over the image)
-- and `colors` (r, g, b, a groups in 0..1, sRGB), and `indices` (vertex numbers, from 1).
local tri = kulisa.mesh{ positions = { 0, 0, 100, 0, 0, 100 }, indices = { 1, 2, 3 } }
```

- Options: `image` (default a white pixel), grid: `cols`, `rows` (cells), `width`, `height`
  (default the image size, or 100); raw: `positions`, `uvs`, `colors`, `indices`.
- Read-only: `vertex_count`, `triangle_count`, `cols` and `rows` (grids; nil otherwise).
  `width`/`height` are the far edge of the vertices. `image` can be changed.
- Bulk setters take a flat table or a Luau `buffer` of 32-bit floats (indices: `u32`) and need
  one entry per vertex: `mesh:set_positions(data)`, `set_uvs`, `set_colors`, `set_indices`.
  `mesh:positions(buf?)`, `uvs(buf?)`, `colors(buf?)` return a flat table, or fill `buf`.
  Reusing one buffer per frame avoids allocating.
- Per vertex (numbered from 1): `mesh:vertex(i)` → `x, y, u, v`; `mesh:set_vertex(i, x, y, u?,
  v?)` (UVs change only when both are given); `mesh:set_color(i, color)`;
  `mesh:vertex_at(col, row)` → the number of a grid vertex (columns `0..cols`).
- `mesh:set_geometry{ positions, uvs, colors, indices }` replaces everything, including the
  vertex count (the mesh stops being a grid).
- A mesh has at most 65,536 vertices. Small images share atlas pages, so UVs outside 0..1
  would read a neighbour's pixels: keep meshes inside 0..1, or use an image larger than 512
  pixels per side for tiling effects.
- `node:contains` and `kulisa.pick` test the triangles, so a deformed mesh is hit where it is drawn.

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
| `press`, `release` | `x, y, button, id` (`left`, `right`, `middle`, `touch`), design units; `id` is the pointer (0 is the mouse, each finger has its own from 1) |
| `move` | `x, y, id` |
| `key`, `key_up` | name: `space`, `enter`, `escape`, `left`, `right`, `up`, `down`, `pageup`, `pagedown`, `f1`…`f12`, `back`, or the character (`a`, `1`) |
| `tap`, `long_press` | `x, y`: a short press without moving (also a mouse click), or a press held for half a second |
| `swipe` | `direction` (`left`, `right`, `up`, `down`), `x, y` where it started |
| `pinch` | `factor, x, y, total`: two fingers moved; `factor` is the distance now over the distance at the last event, `total` over the distance when the second finger landed, `x, y` the midpoint |
| `rotate` | `delta, x, y, total`: radians the line between two fingers turned (positive is clockwise on screen) since the last event, and since the second finger landed |
| `wheel` | lines; positive is up |
| `resize` | window `width, height` in pixels |
| `error` | message |
| `display` | `{ profile, orientation, mismatch }`, see Display |
| `say`, `menu`, `pause`, `end`, `stage`, `restore` | see Story |

`kulisa.input.pointer()` → `x, y` or nil (the primary pointer); `kulisa.input.pointers()` →
`{ { id, x, y, touch, down }, ... }`, the mouse and every finger; `kulisa.input.down(key)` →
whether a key is held.

The primary pointer is the first finger down, or the mouse when none is. It drives hover, the
UI and the `tap`/`long_press`/`swipe` gestures; other fingers only produce `press`, `move`,
`release` (with their `id`) and join `pinch`/`rotate`. Two fingers that landed on the game
(not on a button, draggable or hotspot) make a pinch; a second finger cancels a tap in progress.

Shown screens see the left button and touches first (topmost first), then draggables and
hotspots (see Interaction). What a button, slider or scroll takes, what a draggable or a
blocking hotspot takes, and anything over a modal screen, does not reach `press`, `release`,
gestures or `wheel`. Other buttons and keys always reach the handlers.

## Timers

- `kulisa.after(secs, fn)` runs `fn` once after `secs` seconds of game time; `kulisa.every(secs,
  fn)` runs it every `secs` (at most once per frame). Both return a handle with `cancel()` and
  `done` (true once cancelled, or after an `after` fired), so tasks can `kulisa.await(handle)`.
  Errors in callbacks are reported like handler errors.

## Interaction

Dragging, dropping and clickable areas in the scene (the UI has its own buttons). Each takes the
node it is about, and options. Areas are the node's drawn area (a sprite, mesh or text, or any
descendant's, so a group of parts works); `rect = { x, y, width, height }` or `polygon = { x1,
y1, x2, y2, ... }` give a shape in the node's own space instead (an empty group can be a hit
area); `alpha` (a number 0 to 1 or `true`) makes see-through pixels not count, as in `contains`.
Overlapping things are resolved by what is drawn on top. All of them have `enabled` (read and
write) and `remove()`; they go away on their own when their node is removed.

```lua
local coin = kulisa.sprite{ image = "coin.png", x = 100, y = 100 }
local slot = kulisa.sprite{ image = "slot.png", x = 600, y = 300 }

kulisa.drag(coin, { tag = "coin", alpha = 0.1, on_drop = function(i) print("landed in", i.zone) end })
kulisa.drop_zone(slot, { accept = "coin", capacity = 1, on_reject = function() shake(slot) end })
```

Callbacks get one table: `node` (the dragged node, or the hotspot's), `zone` (the zone's node,
when there is one), `x`, `y` (design units), `pointer` (id), `from_x`, `from_y` (where the node
started), `moved` (it traveled beyond a few units) and `accepted`.

- `kulisa.drag(node, opts)` → handle (`dragging`, `cancel()`). The node follows the pointer.
  Options: `tag` (a string zones can ask for), `bounds = { x, y, width, height }` (limits the
  node's position in its parent), `axis = "x" | "y" | "both"`, `lift` (default true: draw on top
  while dragged), `home = "rejected"` (default: return after a zone refused it) `| "always"
  | "never"`, `snap_time` (seconds, default 0.15), and callbacks `on_start`, `on_move`,
  `on_drop` (released and staying: accepted by a zone, or let go in the open), `on_reject`
  (refused by a zone, or returned because `home = "always"`), `on_click` (released without
  moving; no `on_drop`) and `on_end` (always last; `accepted`, `zone`). A press on a draggable
  does not reach `press`. Several fingers can drag different nodes at once.
- `kulisa.drop_zone(node, opts)` → handle (`occupants`, `clear()`). Options: `accept` (a tag,
  a list of tags, or `function(info) -> boolean` called with `info.node` and `info.zone`; default
  anything), `capacity` (how many it holds at once), `snap = "center"` (default) `| "none" |
  { x, y }` (an offset from the center): an accepted node slides so its center is on the zone's
  center, `by = "pointer"` (default) or `"center"`: whether the pointer or the node's center
  must be over the zone, and callbacks `on_enter`/`on_leave` (while a dragged node is over it,
  accepted or not), `on_drop` (accepted), `on_reject`.
- `kulisa.hotspot(node, opts)` → handle (`hovered`): a clickable area. Callbacks `on_click`
  (pressed and released on it), `on_press`, `on_release`, `on_enter`, `on_leave` (the mouse; the
  UI and drags hide it). Options: `priority` (higher wins over what is drawn on top; default 0)
  and `block` (default true: its presses do not reach `press`).

## UI

Screens are functions that return a tree of widgets. They rebuild when a signal they read
changes, when `show` passes new props, after each story step (so they can show story state),
and on `kulisa.ui.refresh`. Widgets are matched to the previous build by `key` (or position),
so their nodes, hover state, scroll position and running typewriter survive rebuilds.

```lua
local ui = kulisa.ui
local who = ui.signal("Mila")

ui.screen("say", function(props)
    return ui.box{ anchor = "bottom", width = "80%", height = 240, margin = 24,
        background = "gui/frame.png", slice = 24, padding = { 24, 32 },
        ui.text{ text = who, size = 30, color = "#e8a0bf" },
        ui.text{ id = "line", text = props.text, size = 34, cps = 40, y = 48 },
    }
end, { z = 100, area = "design" })

ui.show("say", { text = "Hello." })
who:set("Ren") -- the screen rebuilds next frame
```

- `ui.signal(value)` → a signal: `s()` or `s:get()` reads it (and subscribes the screen being
  built), `s:peek()` reads without subscribing, `s:set(v)` changes it. A signal can also be
  given directly as a widget property (`text = who`). Create signals once, not in screens.
- `ui.screen(name, fn, { z = 100, modal = false, area = "safe" })` defines a screen.
  `fn(props)` returns one widget. `z` orders screens (higher on top). `modal` screens block
  pointer input to everything below. `area` is what the screen's root is laid out in:
  `"safe"` (the visible screen minus notches and system bars), `"screen"` (the whole visible
  screen) or `"design"` (the design resolution's rectangle, for text boxes that should stay
  readable on ultrawide screens).
- `ui.show(name, props?)` shows a screen (or rebuilds it with new props) right away;
  `ui.hide(name)`, `ui.shown(name)`, `ui.refresh(name?)` (all shown screens without a name),
  `ui.flush()` (rebuild what is due now instead of next frame).
- `ui.find(screen, id)` → the node of a widget with that `id`: the text node of a text (with
  `complete`, `skip()`, ...), the sprite of an image, the group of a box. `ui.bounds(screen,
  id)` → `x, y, width, height`. `ui.layer(screen)` → the screen's layer (tween its
  `opacity` to fade a screen).

### Widgets

Each constructor takes one table: properties by name, children in the list part (nested
lists are flattened; `false`/`nil` entries are skipped, so `cond and ui.text(...)` works).

| Widget | Is |
|---|---|
| `ui.box{}` | A container; children are placed by their `anchor` (an overlay). `layout = "row"` or `"column"` makes it a stack. |
| `ui.row{}`, `ui.column{}` | Stacks: children side by side or top to bottom, with `gap`, `align` (cross axis: `start`, `center`, `end`, `stretch`) and `justify` (`start`, `center`, `end`, `between`). |
| `ui.grid{ columns = n }` | Rows of `n` equal cells with `gap`; `align` places children in their cells. |
| `ui.text{ text, size, font, color, align, line_height, cps, fade }` or `ui.text("Hi", { ... })` | Rich text (markup as in `kulisa.text`). Wraps at the width it gets; `align`: `left`, `center`, `right`, `justified`. |
| `ui.image{ image, tint, slice }` | An image (path or image); sized to the image unless given a size. |
| `ui.button{ on_click, selected, disabled, ... }` | A box that takes clicks (press and release on it). Any box with `on_click` is a button. |
| `ui.slider{ value, min = 0, max = 1, step, on_change, vertical, thumb_size = 32, bar, thumb }` | Drag to set a value; `on_change(v)` runs while dragging. `background` is the track, `bar` the filled part, `thumb` the handle (colors or images). |
| `ui.scroll{ direction = "vertical", start = "end" }` | A clipped viewport; its children stack and scroll with the wheel or by dragging. `start = "end"` keeps it scrolled to the end as content grows (backlogs). |
| `ui.node{ node = n }` | A node the game made, placed by the layout (custom displayables). It is hidden, not removed, when the widget goes away. |
| `ui.spacer{}` | Empty space that takes what is left in a row or column. |

Properties every widget takes:

| Property | Meaning |
|---|---|
| `key`, `id` | `key` matches the widget across rebuilds (lists that change order); `id` is for `ui.find`. |
| `width`, `height` | A number, `"50%"` of the parent's content box, `"fill"` (the rest of a row or column, shared by `grow`, default 1; elsewhere the whole parent) or `"auto"` (fit the content, the default). |
| `min_width`, `min_height`, `max_width`, `max_height` | Limits. |
| `padding`, `margin` | A number, `{ vertical, horizontal }` or `{ top, right, bottom, left }`. |
| `anchor` | Where the widget sits in an overlay box: `top_left` (default), `top`, `top_right`, `left`, `center`, `right`, `bottom_left`, `bottom`, `bottom_right`, or `{ x, y }` in 0..1. |
| `x`, `y` | Moves the widget after layout. |
| `background`, `tint`, `opacity`, `slice`, `slice_scale` | A color (`"#rrggbb"`, `{ r, g, b, a }`), an image (path or image) or a tinted image (`{ image = ..., tint = ... }`) behind the widget; `tint` multiplies the background; opacity of the widget and its children; nine-slice borders for an image background (sliders' bars use them too). |
| `color` | Text color. |
| `visible = false` | Leaves the widget out. |
| `hover_*`, `pressed_*`, `selected_*`, `disabled_*` | Any of `background`, `tint`, `opacity`, `color`, `image`, `bar`, `thumb`, `slice`, `slice_scale` for that state, e.g. `hover_background`, `selected_color`. Widgets inside a button follow the button's state. |

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
`localStorage` on the web, internal storage on Android). On desktop, `KULISA_DATA_DIR` moves
it (portable installs, tests).
