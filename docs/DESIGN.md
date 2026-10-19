# Kulisa design

Living document. Architecture, crate boundaries and the decisions behind them.
When a decision changes, update the relevant section and add a line to the
[decision log](#13-decision-log).

## Milestones

Kulisa is built in milestones M0 to M10; "M3" etc. elsewhere in this document
refer to this table. Each milestone ends with all six targets compiling in CI,
tests passing and this document updated.

| | Milestone | Scope | Status |
|---|---|---|---|
| M0 | Platform spike | Workspace, CI for all six targets, wgpu window with a textured quad, Luau "hello" on every target (incl. web and iOS) | Done |
| M1 | Renderer and scene graph | Batching, sprites, layers, camera, parallax, render targets, custom WGSL shaders, post-fx chains (§4) | Done |
| M2 | Text and audio | Markup, cosmic-text shaping, glyph atlas, typewriter, per-glyph effects, ruby; kira channels (§5, §6) | Done |
| M3 | Story language and VM | `.kulisa` parser and compiler, bytecode VM, clear errors, rollback, update-safe saves, platform storage, portable stage (§7, §10) | Done |
| M4 | Luau API | The `kulisa` API over the whole engine, sandbox, tasks, registered state, save slots and migrations; the player runs game folders/`.kpak`s; the smoke scene became `demos/smoke` (§8, [API.md](API.md)) | Done |
| M5 | UI system and classic-vn template | Reactive declarative screens (`kulisa-ui`), layout, widgets, nine-slice; `templates/classic-vn` (textbox, namebox, choices, history, save/load with thumbnails, settings, quick menu, touch gestures); transitions; display profiles, aspect clamp, anchors and orientation (§3, §9) | Next |
| M6 | Meshes, input, interaction | Deformable vertex-grid meshes from Luau, alpha hit testing, drag and drop with snap zones, multi-touch, gamepad, gyro, haptics, sliders, text input | Planned |
| M7 | PC unwrap demo | `demos/pc-unwrap`: peeling and applying plastic film (curling mesh, squeegee, bubbles, glossy plastic shading), public API only | Planned |
| M8 | More templates and the parallax demo | `templates/desktop-os`, `templates/text-rpg`; `demos/parallax` (layers, pointer/gyro camera, GPU particles, shader sky, depth-map 2.5D, phone-portrait / desktop-landscape / 21:9 display demo); example post-fx (bloom, blur, grading, CRT) | Planned |
| M9 | CLI, live reload, quality tiers | `kulisa-cli` (`new --template`, `run`, `build <target>`), live reload of stories, Luau, shaders and assets, on-screen error view, game config (`[display]` etc.), quality tiers | Planned |
| M10 | Ren'Py importer | Basic `.rpy` import: labels, dialogue, menus, jumps, scene/show | Planned |

Not yet placed: story-language animation (an ATL equivalent over tweens and
timelines), layered sprites, sprite sheets, voice auto-advance. Assign them when
the milestone that needs them starts.

## 1. Principles

1. **The engine has no look.** Core provides rendering, text, input, audio,
   scripting, saves/rollback and a scene graph. Nothing visual is hardcoded.
2. **Templates are just scripts.** The classic VN UI is a starter template in
   `.kulisa` + Luau. It can be edited or deleted.
3. **Public API rule.** Templates and demos use only public API. If a demo needs
   something, it becomes a general engine feature.
4. **Fast.** Batched GPU rendering, instant startup, small builds, 60 fps on
   mid-range phones, smooth at 4K.
5. **Lean.** Few dependencies, small focused crates, simple code.

Building blocks we depend on: `winit`, `wgpu`, `cosmic-text`, `kira`, `mlua`
(Luau), `image`, `serde`, `gilrs`. Everything above them is ours.

## 2. Crate map

```
                kulisa-player   (entry points: main, android_main, wasm start)
                      |
                kulisa-runtime  (window, frame loop, finds the game)
                      |
                kulisa-lua      (Luau sandbox, kulisa API, the scripted Game)
     _________/     /   |   \     \_________
    /              /    |    \              \
kulisa-story  kulisa-ui  kulisa-audio  kulisa-text --> kulisa-render
    |              |                        |               |
    +--------------+------> kulisa-core <---+---------------+
                                |
                          kulisa-platform  (files, packs, save paths, logging; no engine deps)

   kulisa-cli  (new | run | build <target>; host-only tool, not shipped in games)
```

| Crate | Owns | Depends on |
|---|---|---|
| `kulisa-platform` | Asset/file access, save paths, haptics, gyro, logging, safe areas | std, per-OS crates |
| `kulisa-core` | Scene graph, nodes, transforms, camera, viewport, frame extraction; later tweens/timelines, `Value` type, game state, rollback log | `glam`, `bytemuck` |
| `kulisa-render` | wgpu device, batcher, textures/atlases, meshes, shaders, render targets, post-fx | `wgpu`, `image`, `kulisa-core` |
| `kulisa-text` | Markup, cosmic-text shaping, glyph cache in the sprite atlas, typewriter, effects, ruby | `cosmic-text`, `kulisa-core`, `kulisa-render` |
| `kulisa-audio` | Channels (music/sound/voice/custom), crossfades, per-channel volume, placeholder synthesis | `kira` |
| `kulisa-story` | `.kulisa` lexer, parser, compiler, bytecode, VM with rollback and update-safe saves | `kulisa-core` |
| `kulisa-lua` | Luau VM, sandbox, the `kulisa.*` API (docs/API.md), the scripted `Game` (scene, assets, story, saves, audio) | `mlua`, engine crates |
| `kulisa-ui` | Reactive declarative screens, layout, widgets | `kulisa-core`, `kulisa-text` |
| `kulisa-runtime` | Window, frame loop, finding the game, forwarding input; later live reload | `kulisa-lua`, `winit` |
| `kulisa-player` | Per-target entry points, `cdylib` for Android/web | `kulisa-runtime` |
| `kulisa-cli` | `kulisa new/run/build`, packaging (APK, web bundle, .app) | host-only |

Rules:

- Lower crates never depend on higher ones. `kulisa-core` has no GPU, audio or
  scripting code, so story/VM/rollback logic is testable headless.
- `kulisa-story` does not know about Luau. Expressions and `$` lines compile to
  opaque chunk IDs that the runtime hands to `kulisa-lua`.
- `kulisa-lua` is the only crate that touches `mlua`. Every binding is a thin
  wrapper over a public Rust API in a lower crate.
- Crates are created when a milestone needs them, not before.

## 3. Runtime and frame loop

One `winit` event loop on the main thread. The runtime finds the game (a folder or
`.kpak`: the first argument, `KULISA_GAME`, or `game/`/`game.kpak` next to the
executable; the APK's `assets/game.kpak` on Android; `game.kpak` fetched next to the
page on the web), creates the renderer, and hands both to `kulisa_lua::Game`, which
runs the game's `main.luau`. The engine has no built-in scene: if a game fails to
start, the error is logged (an on-screen error view arrives with live reload, M9).

Per frame (implemented as of M4: input events, then `Game::frame`: tweens, Luau
tasks, `update` handlers, timed story pauses, text animation; then render):

1. **Input** — winit + gilrs events are normalized into `kulisa_core::input`
   (unified pointer for mouse/touch, keys, gamepad, gyro), hit tested against
   the scene/UI, then dispatched.
2. **Script** — resume Luau coroutines whose waits are due, run `update(dt)`
   callbacks, advance the story VM until it blocks (on `say`, `menu`, `pause`,
   a transition, or a script wait).
3. **Animate** — tweens and timelines step; dirty transforms propagate.
4. **Extract** — the scene graph is walked into a flat draw list.
5. **Render** — the batcher builds GPU buffers and records passes.
6. **Audio** — kira runs on its own thread; the runtime only sends commands.

Time is a fixed `dt` per rendered frame (clamped), plus an accumulated
`time` exposed to shaders and scripts. Asset decoding runs on worker threads on
native and async browser APIs on web.

### Coordinate system

Games declare a **design resolution** (e.g. 1920×1080) and a scale mode
(`kulisa_core::Viewport`):

- `Fit`: the whole design area is visible; nothing is drawn outside it (letterbox).
- `Fill`: the design area covers the window; edges are cropped.
- `Expand` (default): like `Fit`, but content outside the design area stays
  visible, so wider/taller screens (16:9 to 20:9, tablets, portrait) see more.

World space is in design units with **y down**. The camera rests on the design
center, so with an untouched camera a node at `(x, y)` appears at design `(x, y)`.
Anchoring UI to the *screen* edges and safe areas (notches, home indicator) is
the UI system's job (M5); in M1 a HUD layer is anchored to the design area.

### Display profiles, aspect and orientation (planned)

Goal: one design and one layout per game by default, with opt-in support for
games that want different layouts per device class.

- **Aspect clamp** (M5). `Viewport` gains `aspect = { min, max }` (e.g. 4:3 to
  21:9). The window is first clamped to that range (bars beyond it), then the
  scale mode applies inside. `Fit` is the special case min = max = design aspect.
  This keeps 21:9 and 32:9 monitors from revealing areas the art never painted.
- **Anchors** (M5). UI elements anchor either to the screen/safe area (quick
  menu, HUD) or to the design area (the textbox, so lines stay readable on
  ultrawide).
- **Orientation** (M5): `landscape | portrait | auto | any`. `auto` (default)
  follows the design aspect. Mobile locks to it (Android manifest
  `sensorLandscape`/`sensorPortrait`, iOS `UISupportedInterfaceOrientations`);
  desktop uses it for the default window shape; the web cannot lock, so the
  engine reports a mismatch and the template shows a "rotate your device"
  overlay. `any` lets the device rotate and reports changes to scripts.
- **Profiles by form factor** (M5): `desktop`, `tablet`, `phone`, each optionally
  overriding design resolution, scale mode, aspect clamp and orientation on top of
  a base. Form factor comes from the OS and screen size (smallest side under
  600 logical px with touch = phone); on the web it is decided at runtime.
  Because a static Android manifest holds one orientation, the manifest carries
  the base and the runtime corrects it at startup when a profile differs
  (`setRequestedOrientation` via JNI on Android, `set_valid_orientations` on iOS).
- **Portable saves** (M3). Saved scene state stores named positions (`at left`)
  and anchors, resolved against the active profile on load, never raw
  design-unit pixels. A save made on desktop (landscape) loads on a phone
  (portrait).
- **Config** (M9). A `[display]` section in the game config, read by
  `kulisa build` to write manifests and plists:

  ```toml
  [display]
  design = [1920, 1080]
  scale = "expand"
  orientation = "auto"
  aspect = { min = "4:3", max = "21:9" }

  [display.phone]          # optional
  design = [1080, 1920]
  orientation = "portrait"
  ```
- **Demo** (M8). `demos/parallax` shows the same game as portrait on a phone,
  landscape on desktop, and 16:9 vs 21:9 (bleed, clamp, anchors).

## 4. Render pipeline

Implemented in M1. `kulisa-core` turns the scene into a renderer-agnostic `Frame`
(no GPU code, unit-tested); `kulisa-render` only uploads and draws it.

- **Backends.** Vulkan on Windows, Linux and Android; Metal on macOS and iOS;
  WebGPU on web with WebGL2 fallback. DX12 (Windows) and GLES (Linux, Android)
  are fallbacks. A small driver denylist skips adapters that crash on valid
  usage (the Android emulator's gfxstream Vulkan). `WGPU_BACKEND` overrides.
- **Scene graph.** Nodes live in a generational arena (`NodeId`, no
  `Rc<RefCell>`). Kinds: `Group`, `Sprite`, `Layer` (parallax + effects) and
  `Target` (render-to-texture). Transform, opacity, tint, clip and layer parallax
  are inherited; blend and material are per node. Siblings draw by `z` (stable);
  children of a sprite with negative `z` draw behind it (shadows, glows).
- **Extraction** (`kulisa_core::extract`). One tree walk per frame produces
  passes in execution order. Vertices are transformed on the CPU into the pixel
  space of their pass target (4 vertices per sprite is cheaper than per-draw
  uniforms and lets everything batch). Offscreen sprites are culled.
  - Vertex: `pos: vec2<f32>`, `uv: vec2<f32>`, `color: unorm8x4`
    (linear, premultiplied), 20 bytes. Meshes (M6) reuse it with their own
    index ranges.
  - `Target` nodes become their own pass, pushed before the pass that contains
    them. A target sampled by another target must come earlier in the tree;
    otherwise it shows the previous frame.
  - Layers with effects render into a screen-sized transient texture, run their
    chain, and are composited back as one quad.
- **Batching**, two levels. Extraction merges consecutive items with identical
  texture, blend, material and clip. The renderer then merges consecutive items
  whose GPU state matches: images up to 512 px are packed into 2048² atlas pages
  (shelf packer, 1 px extruded borders), so sprites using *different* small
  images still share one bind group and draw call. Large images and render
  targets get their own textures. A tested scene of 60 sprites using 3 textures
  is 1 draw call.
- **Clipping** uses scissor rectangles. A rotated clip uses its bounding box.
- **Camera and parallax.** `Camera { position, zoom, rotation, shake }`. A
  layer's `parallax` factor scales how much of the camera's movement, zoom and
  rotation applies: 1 = world, 0 = fixed to the screen. Pointer/gyro follow are
  behaviours written against public API (see `demos/smoke`).
- **Shaders.** User WGSL defines `fn fragment(in: VertexOut) -> vec4<f32>`
  (premultiplied linear color) and optionally `fn vertex(in: VertexIn) ->
  VertexOut`. Kulisa appends a prelude (after the user code, so naga's
  `file:line` errors match the user's file) with `globals` (`proj`,
  `resolution`, `time`), `tex`/`samp`, and `params.p[0..4]` (four `vec4`s per
  material or effect, uploaded with dynamic offsets). Shaders are validated with
  naga before reaching wgpu; `reload_shader` keeps the old shader on error.
  Pipelines are cached by `(shader, blend, format)`.
- **Effects.** Screens, layers and targets carry ordered effect chains. Effects
  ping-pong between two scratch textures; the last writes into the pass target.
  Vignette, grain and wave ship as examples in the runtime, not built-ins.
- **Blend modes** (premultiplied): alpha, add, multiply, screen.
- **Color.** Textures and targets are `Rgba8UnormSrgb`; blending happens in
  linear space. Images are premultiplied in linear space at load. If the surface
  cannot be viewed as sRGB (some GLES drivers), the screen is drawn into a
  scratch texture and a final pass encodes sRGB, so user shaders never care.
- **Headless mode** renders offscreen with pixel readback. GPU tests use it and
  run in CI on software adapters; save thumbnails (M3) will too.
- **Known limits (M1):** linear filtering only (no per-texture nearest/repeat
  yet); no mipmaps; atlas space is not reclaimed when textures are dropped;
  vertex colors are 8-bit, so very dark tint gradients band; custom shaders that
  offset UVs should use non-atlas textures (render targets, large images).
- **Quality tiers** (planned, M9): `low/medium/high` from adapter info, limits
  and platform; overridable. Tiers scale render-target resolution, MSAA and
  which effect chains a scene enables.

## 5. Text

Implemented in M2 (`kulisa-text`).

- **Markup** (`markup.rs`): `{b}`, `{i}`, `{color=#e8a0bf}`, `{size=1.5}` or
  `{size=32px}`, `{font=Name}`, `{wave[=amp]}`, `{shake[=amp]}`,
  `{ruby=reading}base{/ruby}`, `{speed=k}` and `{w=secs}` pauses; `{{` is a
  literal brace. Errors are strict (unknown tag, bad value, mismatched or unclosed
  tag) with a byte offset and a plain-English message.
- **Shaping** with cosmic-text (bidi/RTL, fallback across loaded fonts, per-span
  size via `Attrs::metrics`). Each span's index rides along as glyph metadata, so
  every glyph maps back to its style without a second pass.
- **No bundled engine fonts.** `TextSystem::load_font` takes TTF/OTF bytes;
  `load_system_fonts` is an opt-in desktop fallback for development. The smoke
  demo ships Inter (SIL OFL 1.1, license next to the files).
- **Glyphs live in the sprite atlas.** Glyphs are rasterized (swash) at physical
  pixel size (design size × window scale, so text stays sharp) and uploaded with
  `Renderer::create_texture`, i.e. into the same atlas pages as small sprites. A
  text box is a `NodeKind::Quads` node and batches with the UI around it. Glyphs
  are cached per cosmic-text `CacheKey` (font, size, subpixel bin); a box
  re-lays out when the window scale changes. Upload goes through the `GlyphSink`
  trait so layout is unit-tested without a GPU.
- **Typewriter** (`TextBox`): each glyph's reveal time comes from character
  order, `{speed}` spans and `{w}` pauses; characters per second and per-glyph
  fade are runtime settings. `skip()` and `is_complete()` serve click-to-advance.
- **Effects** are per-frame offsets and alphas on the glyph quads: wave (sine by
  glyph order), shake (deterministic jitter at 24 Hz), color and size via spans.
- **Ruby** is shaped at half size and centered above the first line of its base
  text; lines with ruby get extra leading. It reveals with its base.
- **Known limits (M2):** no synthetic bold/italic (load the real faces); ruby over
  a base that wraps across lines sits above the first line only; vertical text and
  per-glyph custom shaders are not supported yet; RTL is shaped by cosmic-text but
  not yet tested (no RTL font is bundled).

## 6. Audio

Implemented in M2 (`kulisa-audio`) on kira.

- One kira sub-track per channel, each with its own volume (linear 0..1 in the
  API, decibels inside). Built-in `music` and `voice` are single-sound channels:
  playing a new sound fades out the current one over `crossfade` seconds.
  `sound` overlaps freely. Games add channels with `add_channel`.
- `PlayOptions { volume, looping, fade_in, crossfade }`; `stop(channel, fade)`;
  `set_volume(channel, v, fade)`; master volume; `is_playing(channel)`, which
  story code will use on `voice` for auto-advance (M3).
- `Sound::decode` loads short sounds into memory. `Sound::stream` streams long
  music on native; kira streams on a thread, which `wasm32-unknown-unknown`
  lacks, so on the web it decodes up front.
- Formats: Ogg Vorbis, WAV (PCM), MP3, FLAC. `wav::encode_mono16` and
  `wav::blip` make synthesized placeholder sounds.
- **Web:** browsers only start audio after a user gesture, so the runtime creates
  the audio system on the first click, tap or key.
- **No audio device** (headless machines) is a warning, not an error.
- Tests run against kira's mock backend, so CI needs no sound card. Linux builds
  need ALSA headers (`libasound2-dev`).
- Music position as part of saved scene state arrives with saves (M3).

## 7. Story language and VM

Implemented in M3 (`kulisa-story`; Luau host in `kulisa-lua`).

### Language

```
define mila = Character("Mila", { color = "#e8a0bf" })
default trust = 0

label start:
    scene cafe_night with fade
    show mila smile at left
    mila "You actually came. Trust: [trust]."
    menu:
        mila "So?"
        "Of course.":
            $ trust += 1
            jump cafe_talk
        "Got lost on the way." if trust < 2:
            call lost_route
```

Indentation-based (spaces only), Ren'Py-like. Top level: `label name:`,
`define name = <luau>` (constant), `default name = <luau>` (saved story
variable), `lua:` (setup code). Inside labels: say lines (`who "text"` or
narration `"text"`), `menu:` (optional caption line, choices with optional
`if` guards), `if`/`elif`/`else`, `jump`, `call`/`return`, `$ <luau>`, `lua:`
blocks, `scene [image...] [with t]`, `show image... [at position] [with t]`,
`hide tag [with t]`, `with t`, `play <channel> "file" [fadein s] [loop|noloop]`,
`stop <channel> [fadeout s]`, `pause [s]`.

- Text is the text system's markup (`{b}`, `{wave}`, `{ruby=…}`, …) plus
  `[expression]` interpolation; `[[` is a literal bracket. Brackets nest
  (`[bag[2]]`).
- Full-line comments start with `#`. A `#` inside a statement is never a
  comment (it is Luau's length operator).
- Statement keywords are reserved and cannot be character names.
- Falling off the end of a label returns to the caller, or ends the story.
- `music` loops by default; other channels play once unless `loop` is given.

### Pipeline

`lines → parser → AST → compiler (resolve + codegen) → Program`.

- Every error has file, line, column and a plain-English message, rendered with
  the source line and a caret. The parser recovers at statement boundaries, so
  one run reports every error.
- The compiler resolves labels and characters across all files, with
  "did you mean" suggestions, and reports duplicate labels and names with the
  location of the first definition.
- Luau parts (conditions, `$` lines, `lua:` blocks, `define`/`default` values,
  `[interpolation]`, character references) become **chunks** compiled by the
  host once at load time; Luau syntax errors are reported at the story's file
  and line.

### Bytecode and VM

One flat op list per label: `Say`, `Menu` (choices with guard chunk and target),
`Exec`, `JumpIfNot`, `Goto`, `Jump`, `Call`, `Return`, `Scene`, `Show`, `Hide`,
`With`, `Play`, `Stop`, `Pause`. Menus and `if` chains compile to jumps.

The VM is a call stack of `(label, pc)` frames. `next()` runs until something
the runtime must handle: stage and audio events (`Scene`, `Show`, `Hide`,
`With`, `Play`, `Stop`) are returned without blocking; `Say`, `Menu`, `Pause`
and `End` block. Menus continue with `choose(index)`; choices whose guard is
false are left out.

The VM never runs Luau itself: it calls a `Host` (`define`, `set_default`,
`eval`, `exec`, `snapshot`, `restore`). `kulisa-lua::StoryHost` keeps two
layers: **story variables** (saved; written by `$` lines and label-level
`lua:` blocks) and **constants** (not saved; `define` and top-level `lua:`
helpers). Lookups chain variables, constants, globals. Snapshots accept only
plain data and name the variable that breaks the rule.

The engine never draws dialogue itself. `kulisa-lua::Game` runs the VM and
passes its events to Luau handlers (`say`, `menu`, `stage`, `restore`, ...);
`play`/`stop` lines go straight to audio. While the VM runs, its state is taken
out of the shared game state, so `$` lines can call the whole API. Transitions
(`with`) are reported to handlers but not animated yet (M5).

### Positions that survive updates

A frame is saved as `(label, statement index, statement hash, resume_after)`.
Statement hashes are FNV-1a of the statement's first line. On load: if the hash
at the saved index matches, resume there; otherwise use the nearest statement in
the label with the same hash; otherwise the label start. Moves are reported as
warnings. Call frames resume after their `call`. `default`s added in an update
fill in for old saves.

## 8. Luau

Luau via `mlua` with the `luau` feature. Chosen over Rhai for speed, coroutines,
gradual typing, sandboxing and no-JIT operation on iOS.

Implemented in M4; the reference is [API.md](API.md).

- **Sandboxed.** `Lua::sandbox` (Luau's `luaL_sandbox`): no `io`/`os`/`loadfile`,
  library tables and the `kulisa` table read-only, game globals in a per-game
  table. Files go through `kulisa.read_text`/`require` from the game's assets.
  Mod loading (desktop only) will use the same sandbox.
- **No borrow across calls.** Engine state lives in one `Rc<RefCell<State>>`
  (Lua app data). Bindings borrow it briefly and never call Luau while holding
  it; handlers are collected first, then called. Re-entrant calls therefore work
  (a `say` handler can call `kulisa.story.next()`).
- **Handles, not tables.** Nodes, images, shaders and tweens are userdata
  wrapping ids; properties go through `__index`/`__newindex` and fail loudly on
  unknown names. A removed node's handle errors on use.
- **Coroutines for sequencing.** A Luau prelude implements `kulisa.spawn`,
  `kulisa.wait` and `kulisa.await`; the engine resumes tasks once per frame.
  `main.luau` runs as a task.
- **Errors** in handlers and tasks are logged with a traceback and passed to
  `error` handlers; the game keeps running.
- **API outline** (all under the `kulisa` global; rows without an M4 note are
  planned):

| Namespace | Purpose |
|---|---|
| `group`, `sprite`, `text`, `layer`, `target` | Scene nodes: transforms, opacity, tint, blend, clip, z, shaders (M4) |
| `mesh` | Vertex grids: positions, UVs, colors, indices editable per frame |
| `load_shader`, `effects`, `target` | WGSL shaders, params, effect chains, render targets (M4) |
| `camera`, `screen` | Pan, zoom, rotate, shake; design size, clear color, screen effects (M4) |
| `node:tween`, `spawn`/`wait`/`await` | Easing, delay, repeat, yoyo, coroutine tasks (M4); timelines later |
| `text`, `load_font` | Rich text nodes, typewriter, effects, fonts (M4) |
| `audio` | Channels, play/stop/fade, volumes (M4) |
| `on`, `input`, `node:contains` | Pointer, keys, wheel, hit testing (M4); gamepad, gyro, drag and drop later |
| `ui` | Reactive screens, layout, widgets, bindings |
| `story` | Load, start, next, choose, rollback, variables; events via `on` (M4) |
| `state`, `save` | Registered state, save/load slots, migrations (M4) |
| `platform` | Haptics, safe areas, OS info, quality tier |

- **Custom displayables** are Luau tables with `update(dt)`/`draw`
  hooks built from the same node/mesh/shader primitives.

## 9. UI system

Declarative screens defined in Luau. A screen is a function from state to a
tree of widgets; reading a reactive value (`kulisa.ui.signal`, story variables,
registered state) subscribes the screen, and changes mark only that screen
dirty. Layout: stack (h/v), grid, anchors/insets, safe-area aware. Widgets are
composed from scene nodes (text, sprite, nine-slice, mesh) so the UI renders
through the same batcher. Templates are built only from this system.

## 10. Saves and rollback

Implemented in M3 (`kulisa-core::{Value, SaveFile}`, `kulisa-story::Vm`,
`kulisa-platform::Storage`).

### State model

Everything that survives save and rollback is **plain data** in a `Value` tree
(`nil, bool, int, float, string, bytes, list, map`). Never live objects:

- The VM call stack, as update-safe positions (§7).
- Story variables (a Luau table restricted to plain data).
- **Stage**: the scene image, shown images in draw order with their *named*
  position (`at left`), and looping tracks per channel. The runtime rebuilds
  visuals and music from it; named positions keep saves portable across display
  profiles (see Coordinate system).
- Registered Luau state (`kulisa.state.register(name, { save, load })`), stored
  next to the story variables in every checkpoint and save.

### Rollback

On arriving at every `Say`, `Menu` and `Pause`, the VM records a **checkpoint**:
frames, stage and a snapshot of story variables. Checkpoints form a bounded
history (default 100). `rollback()` restores the previous checkpoint;
`roll_forward()` restores the next one again. Continuing exactly along the
recorded path keeps the redo list; any divergence (e.g. a different menu choice)
drops it. After either, the runtime rebuilds the stage, and `next()` shows that
line again. Snapshots are full copies for now; diffing is a later optimization
if story state grows.

### Save format

```
"KSAV" | format_version u16 LE | header Value | body Value
header: game_id, game_version, save_version, timestamp, slot, playtime, title, thumbnail (PNG)
body:   the VM's save: frames, stage, story variables
```

- `Value` encoding: one tag byte per value, LEB128 lengths, zigzag ints,
  string-keyed ordered maps. Decoding checks lengths and nesting depth, so
  damaged or hostile files fail cleanly. No type names, no code in saves.
- The header decodes on its own, so slot lists don't load bodies.
- Unknown fields are ignored and missing ones default.
- **Migrations**: games set `save_version` and register steps (`from` to
  `from + 1`) that transform the body; saves from a newer game are refused with
  a clear message. Luau games register steps with `kulisa.save.migrate`.
- Engine format changes bump `format_version`; files from a newer Kulisa are
  refused.
- **Storage** (`kulisa_platform::Storage`): atomic file writes under
  `%APPDATA%`, `~/Library/Application Support` or `$XDG_DATA_HOME` (desktop and
  iOS), the app's internal storage on Android, base64 in `localStorage` on the
  web. Names are restricted to `[A-Za-z0-9_-]` and exclude Windows device names.
- Thumbnails are supported by the format; capturing them arrives with the
  save/load screens (M5).

## 11. Platforms and builds

| Target | Triple | Graphics | Entry / packaging | Built |
|---|---|---|---|---|
| Windows | `x86_64-pc-windows-msvc` | Vulkan (DX12 fallback) | `main` | local + CI |
| Linux | `x86_64-unknown-linux-gnu` | Vulkan (GLES fallback) | `main` | CI (no WSL on dev host) |
| macOS | `aarch64-apple-darwin` | Metal | `main`, `.app` later | CI |
| Web | `wasm32-unknown-unknown` (see §12 risk 1) | WebGPU, WebGL2 fallback | `cdylib` + wasm-bindgen + `index.html` | local + CI |
| Android | `aarch64-linux-android` (+ `x86_64`, `armv7`) | Vulkan (GLES fallback) | `cdylib` with `android-activity` NativeActivity; APK built with `aapt2`/`aapt`/`zipalign`/`apksigner` (no Gradle) | local + CI |
| iOS | `aarch64-apple-ios` (+ `aarch64-apple-ios-sim`) | Metal | `main` in a `.app` bundle (Info.plist + ad-hoc signature, no Xcode project); logs via `os_log` | CI (simulator run) |

- CI (GitHub Actions) builds all six on every push; it is the source of truth.
- Engine-developer builds go through `cargo xtask` (`web`, `apk`, `ios-app`), so CI and
  local builds run the same steps. Game-creator builds (`kulisa build <target>`)
  arrive with `kulisa-cli` in M9 and will reuse this logic.
- Games ship scripts and assets inside the build (App Store safe), packed into
  a single archive (`.kpak`: index + blobs). `cargo xtask` bundles a game
  (`--game DIR`, default `demos/smoke`) into web, APK and iOS builds;
  `cargo xtask pack` writes a `.kpak`. Desktop runs a folder directly.
- Mod loading is desktop only.

## 12. Risks

1. **Luau on web — resolved in M0.** Luau is C++, and `mlua` only documents
   wasm through `wasm32-unknown-emscripten`, while `winit` and wgpu's WebGPU
   backend need `wasm32-unknown-unknown`. Kulisa builds Luau for
   `wasm32-unknown-unknown` directly, keeping one module and one stack:
   - Luau's C++ is compiled by clang with `--target=wasm32-wasip1` against a
     pinned wasi-sdk sysroot that ships libc++/libc++abi/libunwind built with
     **wasm exception handling** (`-fwasm-exceptions`, exnref). Luau errors and
     its compiler's `CompileError` are real C++ exceptions, so nothing is
     patched in Luau. `cargo xtask web` downloads the sysroot and sets the flags.
   - The handful of WASI calls wasi-libc makes (clock, environ, fd_write, exit)
     are defined in Rust (`kulisa-lua/src/web.rs`), so the module has no WASI
     imports and wasm-bindgen output loads in any browser.
   - C++ static constructors run once via an explicit `__wasm_call_ctors`
     call; referencing it also stops wasm-ld from re-running them on every
     export, which otherwise loops forever in Luau's flag registration.
   - `mlua` needed one small patch (a heap-dump helper uses libc stdio); it is
     vendored in `third_party/mlua` until fixed upstream.
   - **Cost:** browsers need wasm exception handling with exnref (Chrome/Edge
     and Firefox current releases, Safari 18.4+). Older browsers get a clear
     error page. Pinning exact minimum versions is an M9 task.
     Verified: eval, runtime and syntax errors, `pcall`, coroutines in Node and
     Chromium; full engine (Luau + WebGPU) in Chromium.
2. **Luau on iOS — resolved in M0 (simulator).** No JIT needed; C++ is built by
   the Xcode toolchain. CI builds the device target and runs the player in an
   iOS simulator: `cargo xtask ios-app --sim` assembles `dist/ios/Kulisa.app`,
   `.github/scripts/ios-sim-smoke.sh` boots a simulator, launches the app, takes
   a screenshot and fails unless the process is alive after 10 s and the Luau
   smoke-test line reached the unified log. Not yet run on a physical device
   (needs a signing identity and provisioning profile).
3. **Android text input.** NativeActivity has weak soft-keyboard/IME support.
   If M6 text input needs it, switch to GameActivity (requires a Gradle step).
4. **WebGL2 fallback limits.** No compute, fewer texture formats, uniform
   buffer limits. The renderer must stay within WebGL2 downlevel limits; GPU
   particles need a non-compute path.
5. **Build size on web.** wgpu + cosmic-text + Luau. Track wasm size per
   milestone; use `opt-level = "z"`, LTO and `wasm-opt` for release.
6. **Driver bugs.** Some Android Vulkan drivers crash on valid usage (the
   emulator's gfxstream driver crashes in `vkQueueSubmit` and on debug object
   names). The renderer keeps a small adapter denylist that falls through to the
   next backend (GLES), and Android builds never set debug object names. GLES
   surfaces may lack sRGB views; the shader then encodes sRGB itself.

## 13. Decision log

| Date | Decision |
|---|---|
| 2026-10-19 | `.kulisa` for story files (`.kul` clashes with existing software). |
| 2026-10-19 | Separate `kulisa-runtime` (frame loop) and `kulisa-player` (entry points) so `kulisa-platform` stays a leaf crate with no engine deps. |
| 2026-10-19 | Own self-describing save encoding instead of bincode/postcard: those are not tolerant of unknown or missing fields. |
| 2026-10-19 | Android packaging without Gradle (NativeActivity + SDK build-tools) to stay lean; revisit for IME. |
| 2026-10-19 | `winit` 0.30 (stable) rather than the 0.31 beta. |
| 2026-10-19 | Luau on web via `wasm32-unknown-unknown` + wasi-sdk 34 sysroot with wasm exceptions (risk 1). Requires exnref-capable browsers. |
| 2026-10-19 | Vendor `mlua` 0.12.2 with a one-line wasm fix rather than switching scripting languages. |
| 2026-10-19 | Static libc++ on Android (`c++_static` + `c++abi`) so APKs need no extra `.so`. |
| 2026-10-19 | `cargo xtask` for engine-developer build steps (web, APK, iOS app). |
| 2026-10-19 | Minimum OS: iOS 15, macOS 11 (Rust's iOS 10 default fails to link Luau's C++ with current Xcode), Android API 26. |
| 2026-10-19 | The name is written **Kulisa** in prose, titles and UI; code identifiers (crates, CLI, `.kulisa`, the `kulisa` Luau global) stay lowercase per Rust and file-extension conventions. |
| 2026-10-19 | M1: extraction in `kulisa-core` emits pixel-space vertices per pass; batching happens at extraction and again on GPU state (atlas pages). Keeps the renderer thin and the logic testable without a GPU. |
| 2026-10-19 | M1: user shaders are fragment (+ optional vertex) functions over a fixed prelude, validated with naga for `file:line` errors. Effects use the same contract. |
| 2026-10-19 | M1: the window always shows sRGB-correct output; GLES surfaces without sRGB views get a final encode pass instead of pushing that into every shader. |
| 2026-10-19 | M2: glyphs go into the sprite atlas (RGBA) instead of a separate R8 glyph atlas: text batches with sprites and there is one texture path. Costs 4x memory per glyph; revisit if CJK-heavy games hit atlas limits. |
| 2026-10-19 | M2: audio channels are kira sub-tracks; music streams on native but decodes up front on web (no threads). Web audio starts on first user gesture. |
| 2026-10-19 | Display profiles: one design by default; opt-in `phone`/`tablet`/`desktop` overrides, aspect clamp, orientation lock. Placed in M3 (portable saves), M5 (clamp, anchors, profiles, orientation), M8 (demo), M9 (config). |
| 2026-10-19 | M3: the VM talks to Luau only through a `Host` trait; tests use a fake host, so the VM is tested without Luau. |
| 2026-10-19 | M3: story variables and constants are separate Luau tables; `define` and top-level `lua:` write constants, so helper functions never end up in saves. |
| 2026-10-19 | M3: full-snapshot checkpoints instead of diffs, until story state is large enough to matter. |
| 2026-10-19 | M4: the engine owns no scene; `kulisa-lua::Game` runs a game folder or `.kpak` and the runtime only forwards window events. The old Rust smoke scene became `demos/smoke`, written only against the public Luau API. |
| 2026-10-19 | M4: Luau API handles are userdata with strict property access instead of plain tables, so typos fail loudly and node state lives in one place (the scene). |
| 2026-10-19 | M4: tasks (`spawn`/`wait`/`await`) are a small Luau prelude over coroutines rather than Rust-side scheduling. |
| 2026-10-19 | iOS `.app` assembled by `cargo xtask ios-app` (plist + ad-hoc `codesign`) instead of an Xcode project; iOS logs go to `os_log` (`oslog` crate) since apps have no visible stderr. |
