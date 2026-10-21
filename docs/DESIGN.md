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
| M5 | UI system and classic-vn template | Reactive declarative screens (`kulisa-ui`), layout, widgets (incl. sliders, moved from M6), nine-slice; `templates/classic-vn` (textbox, namebox, choices, history, save/load with thumbnails, settings, quick menu, touch gestures, Ren'Py skip/auto/rollback); transitions; display profiles, aspect clamp, safe areas, anchors and orientation (§3, §9) | Done |
| M6 | Meshes, input, interaction | Deformable vertex-grid meshes from Luau, alpha hit testing and picking, drag and drop with drop zones, hotspots, multi-touch with pinch/rotate, timers, gamepad (gilrs) with keyboard/gamepad focus navigation of UI screens (classic-vn playable without a pointer), text input (`ui.input`, soft keyboards, IME), haptics, motion sensors; `demos/interact` (§9) | Done |
| M7 | PC unwrap demo | `demos/pc-unwrap`: unboxing a Blockbench-rendered PC from its box, a plastic sleeve pulled off over the top, films peeled off the side and front glass, tape, an I/O sticker, a squeegee with air bubbles, glossy plastic shading; public API only (§9, M7 section) | Done |
| M8 | More templates and the parallax demo | `templates/desktop-os`, `templates/text-rpg`; `demos/parallax` (layers, pointer/gyro camera, GPU particles, shader sky, depth-map 2.5D, phone-portrait / desktop-landscape / 21:9 display demo); example post-fx (bloom, blur, grading, CRT, grain); classic-vn reworked as a cat-themed sample (§9, M8 section) | Done |
| M9 | CLI, live reload, quality tiers | `kulisa-cli` (`new --template`, `run`, `build <target>`), live reload of stories, Luau, shaders and assets, on-screen error view, game config (`kulisa.toml`), quality tiers with render scale (§11, M9 section) | Done |
| M10 | Ren'Py importer | `kulisa import`: `.rpy` labels, dialogue, menus, conditions, jumps and calls, scene/show/hide/with, audio, simple Python; images and sounds; app icons on every platform (M10 section) | Done |

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
   (kulisa-ui needs only kulisa-core: text is measured through a trait)
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
| `kulisa-ui` | Widget elements, reconciliation, layout, hit testing, gestures, signal dependencies (draws nothing; `kulisa-lua` turns widgets into nodes) | `kulisa-core` |
| `kulisa-runtime` | Window, frame loop, finding the game, forwarding input, live reload (folder watcher, web dev polling) | `kulisa-lua`, `winit` |
| `kulisa-player` | Per-target entry points, `cdylib` for Android/web | `kulisa-runtime` |
| `kulisa-renpy` | Ren'Py import: `.rpy` to `.kulisa`, Python expressions to Luau, image and audio names, notes on what was left out | std only (host-only) |
| `kulisa-cli` | `kulisa new/import/run/build/pack/serve`, packaging (desktop folder, APK, web page, .app) from `kulisa.toml`; `cargo xtask` is a shim over it | `kulisa-platform` (host-only) |

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
reads `kulisa.toml`, applies its `[display]` and runs the game's `main.luau`. The engine has
no built-in scene: if a game is missing or fails to start, it still gets a window, and the
error view (a built-in bitmap font drawn as an overlay pass) says what went wrong.

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
the UI system's job: screens are laid out in the screen, safe or design area (§9).

### Display profiles, aspect and orientation

Implemented in M5 (the demo in M8) except the build config (M9). Goal: one design
and one layout per game by default, with opt-in support for games that want
different layouts per device class. API: `kulisa.display{ ... }` (docs/API.md).

- **Aspect clamp**. `Viewport::aspect = [min, max]` (e.g. 4:3 to 21:9). The
  window is first clamped to that range (bars beyond it, centered), then the scale
  mode applies inside. `Fit` is the special case min = max = design aspect. This
  keeps 21:9 and 32:9 monitors from revealing areas the art never painted.
- **Anchors and areas**. Each UI screen is laid out in one area: `screen` (what
  the window shows), `safe` (screen minus insets) or `design` (the design
  rectangle); widgets anchor inside it. Safe-area insets come from Android's
  `WindowInsets` (system bars and cutouts, via JNI; Android 15 draws apps edge to
  edge), iOS's safe area (winit's inner rect), CSS `env(safe-area-inset-*)` on the
  web, and `KULISA_SAFE_AREA` / `?safe=` for testing. The classic-vn text box uses
  the safe area with a width capped at 88% of the design width, so lines stay
  readable on ultrawide and clear the gesture bar on phones.
- **Orientation**: `landscape | portrait | auto | any`. `auto` (default)
  follows the design aspect. The runtime locks it at startup
  (`setRequestedOrientation` through JNI on Android, `set_valid_orientations` on
  iOS); desktop windows open in that shape; the web cannot lock, so on touch
  screens `screen.mismatch` is set and the template shows a "turn your device"
  overlay. `any` lets the device rotate; `display` events report changes. Writing
  the orientation into manifests and plists is part of the build config (M9).
- **Profiles by form factor**: `desktop`, `tablet`, `phone`, each optionally
  overriding design resolution, scale mode, aspect clamp and orientation on top of
  a base. Form factor comes from the OS and screen size (touch with the short side
  under 600 logical px = phone; touch up to 1400 on the long side = tablet, so
  touch laptops stay desktops); `KULISA_PROFILE` / `?profile=` force one. It is
  chosen once, at startup.
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
- **Demo** (M8, done). `demos/parallax`'s display lab shows the same scene as portrait on a
  phone (the `phone` profile's 1080x1920 design), landscape on desktop, and 16:9 / 21:9 / 32:9 /
  4:3 (bleed, aspect clamp on and off, anchors in the safe area), switchable while it runs.

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
    (linear, premultiplied), 20 bytes. Meshes (M6) reuse it: their vertices go through the
    same extraction and batching (see Meshes below).
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
- **Effects.** Screens, layers and targets carry ordered effect chains. Each effect may run at
  a fraction of the pass's size (`scale`) and read the chain's source or any earlier output as
  `tex` or `mask_tex` (M8), which is enough for multi-pass blur and bloom without compute. The
  renderer plans scratch textures per size and returns each to the pool after its last reader, so
  a plain chain still ping-pongs between two; the last effect writes into the pass target.
  Vignette, grain and wave ship as examples in the runtime, not built-ins.
- **Blend modes** (premultiplied): alpha, add, multiply, screen.
- **Color.** Textures and targets are `Rgba8UnormSrgb`; blending happens in
  linear space. Images are premultiplied in linear space at load. If the surface
  cannot be viewed as sRGB (some GLES drivers), the screen is drawn into a
  scratch texture and a final pass encodes sRGB, so user shaders never care.
- **Headless mode** renders offscreen with pixel readback. GPU tests use it and
  run in CI on software adapters; save thumbnails (M3) will too.
- **Masks** (M5). Materials and effects take an optional second texture
  (`mask_tex`/`mask_samp`, bind group 3; a 1x1 white texture when unset). Masks
  that live in an atlas page are copied into their own texture on first use, so
  shaders sample them with the quad's 0..1 UVs. Image wipes use it.
- **Nine-slice** (M5). A sprite's `slice` borders (texture pixels, drawn at
  `scale`) become up to nine quads at extraction; they batch like any sprite.
  Borders larger than the sprite shrink proportionally.
- **Capture and readback** (M5). `Renderer::capture` draws a scene into a render
  target as if it were the window (transition freezes, save thumbnails).
  `read_texture` + `poll_readback` copy a texture to the CPU without blocking,
  since browsers deliver mapped buffers only after yielding; native callers may
  wait. `create_texture_unpacked` + `free_texture` give thumbnails memory back.
- **Known limits (M1):** linear filtering only (no per-texture nearest/repeat
  yet); no mipmaps; atlas space is not reclaimed when textures are dropped;
  vertex colors are 8-bit, so very dark tint gradients band.
- **Shaded nodes get their own texture** (M7). A sprite or mesh with a material draws from an
  unpacked copy of its image (made on first use, like masks), so its UVs stay 0..1 over the image:
  procedural shaders (`kulisa.sprite{ shader = ... }` with no image) get real coordinates and may
  offset UVs safely. These nodes never batched with unshaded ones, so draw calls do not change
  (`DrawItem::unpack`; text with a shader keeps the shared atlas).
- **Particles** (M8, `kulisa_core::ParticleSystem`, `NodeKind::Particles`). GPU particles without
  compute shaders, so WebGL2 runs them: a ring buffer of 48-byte birth records (position,
  velocity, birth time, life, size, spin, angle, seed, tint) lives in a GPU buffer per system and is
  drawn as one instanced draw (6 vertices from `vertex_index`, one instance per record). The CPU
  writes only the records born since the last frame. The vertex shader (`particles.wgsl`) computes
  each particle in closed form from the system's own clock: velocity with linear drag and gravity,
  a wander drift, size and three-stop color over life, fades, spin or velocity stretch, sprite-sheet
  frames; dead records collapse to degenerate triangles. A system's uniform block (the material's 16
  params plus eleven `vec4`s) carries the node-to-pixel transform, so particles follow the node,
  camera and parallax. User particle shaders are the sprite `fragment` contract compiled against
  the particle prelude. Particles are not culled (where they are is known only on the GPU).
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
`stop <channel> [fadeout s]`, `pause [s]`, `pass` (does nothing).

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
out of the shared game state, so `$` lines can call the whole API.

**Transitions** (M5). A transition draws a frozen frame of the screen over the
live scene, through a shader whose progress goes from 0 to 1
(`kulisa.screen:freeze()` / `:transition{...}`); templates ship the shaders
(fade, dissolve, wipes, image dissolve), the engine only the mechanism. The
engine freezes for story `with` the Ren'Py way: before a statement with its own
`with`, so only that change transitions, and at the last interaction (or
transition) before a lone `with` line. The VM does not block on `with`; the
classic-vn template holds the next line until the transition ends, which looks
the same to the player.

**Seen lines** (M5). `Vm::line_key` hashes the label and statement hash, so a
line keeps its identity across sessions and edits that keep its text. The game
keeps a seen set (and chosen menu choices) in persistent storage; `say` events
carry `seen`, menu items `chosen`. `Vm::block_rollback` drops the history.

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
| `mesh` | Vertex grids and raw triangle meshes: positions, UVs, colors, indices editable per frame (M6) |
| `load_shader`, `effects`, `target` | WGSL shaders, params, effect chains, render targets (M4) |
| `camera`, `screen` | Pan, zoom, rotate, shake; design size, clear color, screen effects (M4) |
| `node:tween`, `spawn`/`wait`/`await` | Easing, delay, repeat, yoyo, coroutine tasks (M4); timelines later |
| `text`, `load_font` | Rich text nodes, typewriter, effects, fonts (M4) |
| `audio` | Channels, play/stop/fade, volumes (M4) |
| `on`, `input`, `pick`, `node:contains` | Pointers with ids, keys, wheel, pinch/rotate, text, gamepads, alpha-aware hit testing and picking (M4, M6) |
| `drag`, `drop_zone`, `hotspot`, `after`, `every` | Drag and drop, hotspots, timers (M6) |
| `sensors`, `haptics` | Gyro, gravity, tilt; vibration and rumble (M6) |
| `ui` | Reactive screens, layout, widgets, signals, gestures (M5) |
| `display`, `screen`, `window` | Display profiles, safe area, orientation, transitions, captures, fullscreen, quit (M5) |
| `persist` | Data outside saves and rollback: settings, unlocks (M5) |
| `story` | Load, start, next, choose, rollback, variables; events via `on` (M4) |
| `state`, `save` | Registered state, save/load slots, migrations (M4) |
| `platform` | OS info, quality tier |

- **Custom displayables** are Luau tables with `update(dt)`/`draw`
  hooks built from the same node/mesh/shader primitives.

## 9. UI system

Implemented in M5 (`kulisa-ui`, bound in `kulisa-lua/src/game/ui.rs`; API in
[API.md](API.md)).

- **Screens are functions.** `kulisa.ui.screen(name, fn, { z, modal, area })`;
  `fn(props)` returns a tree of plain tables made by the widget constructors.
  Reading a signal while a screen builds subscribes it; `signal:set` marks only
  its readers dirty, and dirty screens rebuild once per frame (or at once on
  `ui.show` / `ui.flush`). Every shown screen also rebuilds after each story step,
  which is how screens show story variables without tracking them (Ren'Py
  re-evaluates screens each interaction the same way).
- **Reconciliation.** The new tree is matched to the retained one by `key`, or by
  position among unkeyed siblings with the same kind. Matched widgets keep their
  scene nodes, hover/press state, scroll offset and typewriter progress; text is
  laid out again only when its markup, style or wrap width changed.
- **Layout** (two passes, design units): natural sizes bottom up given the space
  available (text measured through the `Measure` trait, so `kulisa-ui` has no
  text or GPU dependency), then positions top down. Boxes are overlays (children
  placed by anchor) or stacks (row/column with gap, `align`, `justify`, `fill`
  weights) or grids; sizes are px, percent, fill or auto with min/max; padding and
  margins. Scroll viewports stack their children, clip them and scroll by wheel
  or drag (`start = "end"` stays at the end as content grows).
- **Looks and states.** Visual properties are optional so state overrides
  (`hover_*`, `pressed_*`, `selected_*`, `disabled_*`) replace just what they
  name; widgets inside a button follow its state, as Ren'Py's style prefixes do.
- **Scene sync.** Each screen is a parallax-0 layer; each widget a group
  positioned relative to its parent, with a background sprite (color or
  nine-sliced image), text node, image sprite or slider parts. Embedded game
  nodes (`ui.node`) are placed by the layout and hidden, not removed, with it.
- **Input routing.** Screens see the left button and touches first, topmost
  first. Buttons click on press and release on the same widget; sliders report
  snapped values while dragged; a press inside a scroll that travels past a slop
  becomes a scroll drag. What the UI takes, and anything over a modal screen,
  does not reach `press`/`release`/gestures/`wheel`; other buttons and keys always
  reach the game. Gestures (tap, long press, swipe) are recognized on what is
  left, for mouse and touch alike.
- **Windows and dragging** (M8). Screens can live in a rectangle (`area = { x, y, width,
  height }`) that `ui.place` moves or resizes (a pure move translates the built widgets instead of
  laying them out again) and `ui.set_z` reorders; widgets take `on_press`, `on_drag` (start, move,
  end with deltas, after a small slop), `block` (swallow input over them), `clip` and `cursor`.
  The cursor follows script > UI > hotspot; desktop windows can be resized from Luau.
- **classic-vn** (`templates/classic-vn`, its README) is built only on this and
  the rest of the public API; a headless test (`kulisa-lua/tests/template.rs`)
  plays through it on every CI run.
- **Not yet:** multi-touch UI (the UI follows the primary pointer; see below),
  per-widget animation (tween the nodes from `ui.find`, or the screen layer from
  `ui.layer`).

### Meshes, hit testing and interaction (M6)

- **Meshes** (`kulisa_core::Mesh`, `NodeKind::Mesh`). Positions, UVs, vertex colors and a
  triangle list in local units; `Mesh::grid` builds the regular case. Extraction transforms
  the vertices into pass pixels and emits one draw item, so a mesh batches with sprites of
  the same texture and state. Scripts edit vertices through bulk setters that take flat
  tables or Luau buffers (no per-vertex userdata) or per-vertex calls. Limits: 65,536
  vertices; UVs outside 0..1 read neighbours of atlased images (as for custom shaders).
- **Hit testing.** `Scene::hit_alpha` tests a sprite rectangle, mesh triangles (so deformed
  meshes are hit where drawn) or text boxes, and with a threshold also the pixel under the
  point through the `AlphaSource` trait. `kulisa-lua` implements it by decoding the alpha of
  the asset a texture came from on first use (nothing is decoded for games that do not ask);
  textures that are not files count as opaque. `Scene::pick_all` walks the tree backwards in
  draw order, honouring visibility, opacity, `clip`, layer parallax and skipping render
  targets; `paint_order` ranks overlapping candidates.
- **Pointers.** The mouse is pointer 0 and each finger gets its own id. The primary pointer
  (the first finger down, else the mouse) drives hover, UI and the tap/long-press/swipe
  gestures; other fingers produce `press`/`move`/`release` with their id and, when two free
  fingers are down (neither taken by the UI, a draggable or a hotspot), `pinch` and `rotate`.
  Order for a press: UI, then draggables and hotspots, then the game.
- **Interaction** (`kulisa-lua/src/game/interact.rs`). `drag`, `drop_zone` and `hotspot`
  attach to nodes; their area is the drawn area of the node or its descendants, or a rect or
  polygon in the node's space, optionally alpha-tested. The engine decides what is grabbed,
  entered, accepted or clicked and snaps accepted items with a tween; everything else is
  callbacks (`on_drop`, `on_reject`, ...). Zone `accept` functions are Luau, so a release is
  decided in phases (collect candidates, ask with the state free, apply) and callbacks run
  after the state borrow ends, like every handler.
- **Focus navigation** (`kulisa-ui/src/focus.rs`, `kulisa-lua/src/game/pad.rs`). The topmost
  screen with `nav` (modal screens by default) takes arrows, d-pad, left stick (repeating
  while held), Enter/Space/A, Page Up/Down and the right stick. Focus moves to the nearest
  button or slider in the pressed direction, sliders take their axis, scrolls follow the focus
  and scroll alone when nothing is focusable. A focused widget gets the `hover` look. Focus
  appears after the first direction input and for screens that open afterwards, until the
  mouse or a touch is used. Enter/A on a screen with nothing focused only focuses, so
  mashing Enter to advance text never picks a choice that just appeared.
- **Gamepads** (`kulisa-runtime/src/gamepad.rs`) use gilrs on desktop and the web (the
  Gamepad API), with a dead zone applied and y pointing down; they are not available on
  Android and iOS (gilrs has no backend there). Games read `pad_*` events and
  `kulisa.input.pad_*`.
- **Text input.** Platform text (key `text`, IME commits and preedit) is delivered as `text`
  and `preedit` events only while a script has called `start_text`; while active, characters
  are not `key`s and navigation gives its keys back. Fields are Luau (`ui.input` in the
  prelude: caret, editing keys, `max_length`, `password`, a `value` signal). The runtime
  turns the IME on (`set_ime_allowed`: the soft keyboard on Android) and, on touch browsers,
  focuses a hidden `<input>` whose events are queued for the runtime
  (`kulisa-platform/src/web_text.rs`). Known gaps: IME composition inside NativeActivity is
  not delivered (Latin keyboards work), winit's web backend does not receive synthetic `insertText` on desktop browsers, no
  clipboard.
  The view lifts above an on-screen keyboard (`Viewport::pan`, set from the keyboard height the
  runtime reports: Android window insets, the web visual viewport, iOS keyboard notifications),
  so a field is never hidden; this replaced the idea of a native text box, which would not match
  the game's look or work for in-scene fields.
- **Haptics and sensors.** `kulisa.haptics.pulse` queues pulses the runtime plays while
  handling input (browsers need a user gesture): Android `VibrationEffect` through JNI,
  `navigator.vibrate`, UIKit impact feedback, and gilrs rumble on desktop pads.
  `kulisa.sensors` reads gyro and gravity in the device frame (`tilt` derives from gravity,
  in the natural orientation): the NDK sensor manager on its own looper thread on Android,
  CoreMotion on iOS, `devicemotion`/`deviceorientation` events in browsers (with iOS's
  permission prompt, which needs a tap). iOS haptics and motion are compiled in CI only.

### The PC unwrap demo (M7)

`demos/pc-unwrap` (pure Luau; README there) is the stress test of meshes, shaders and input.

- **Engine features it needed**, all general and documented in API.md: per-vertex `data` on meshes
  (`in.data` in shaders, stored as f16x4), sprites and meshes with a shader read their image through
  an unpacked texture so `uv` is 0..1 (procedural shaders), `audio.play` `rate` and `pan`.
- **Films are meshes written in one loop per frame** into reused buffers: positions, a normal in
  world space (its length carries a curl shadow) and an air-gap value in `data`, plus a half-resolution
  shadow mesh. `lib/film.luau` has the model: a film lies along faces of the case (`spans`), is
  grabbed at a tab, and peels with a V-shaped fold (the middle leads, so it comes away from the
  middle outwards) or, held at a corner, a diagonal fold sweeping across from it, that rolls the
  free part over a cylinder of tunable radius (a cone, wider away from the middle) and runs a straight,
  drooping tail to the hand. The fold position is solved from where the hand is, sticks, and slips
  forward past an adhesion threshold (sound, haptic pulse); a released flap keeps its fold and
  settles with a damped spring, small peels stick back, and a fully peeled film detaches, hangs from
  the hand and falls when let go.
  Only the peeled prefix of the vertices (sorted by distance along the foil) is recomputed.
- **Shading** (`shaders/plastic.wgsl`): fresnel, a procedural softbox reflection, a highlight, an underside
  tint (which side is visible follows from the winding of `uv` against screen derivatives, so no extra
  attribute), crease-noise wrinkle normals, edge glints, bubble lenses; light and view are world
  vectors, normals arrive in world space so a foil that turns a corner is lit consistently.
- **The PC and the box** are Blockbench models rendered with one orthographic camera into layered
  PNGs: the PC, the box's back and near walls, the foam caps, 12 frames of each flap pair opening.
  `lib/art.luau` holds the camera's projection (fitted to calibration markers) and each layer's
  place, so `lib/geom.luau` turns centimetres into stage positions and the live tape, bag, films and
  glass sit exactly on the renders; the PC is drawn between the box's back and front layers.
- **The bag** (`lib/bag.luau`) is a sleeve with three numbers of state: how far it slid, how far
  its hem was raised into a bunch, and the fold point. A high grab slides it; a low grab lifts the
  hem and squeezes the plastic above the hand into accordion folds; it only comes off over the top.
  It sleeps when settled. **Protector** (`lib/protector.luau`): bubbles are smooth
  lenses pushed ahead of a squeegee blade, merged, and let out at the edges.
- **Frame time**: the demo shows `script ms/frame` (F3 hides it). Desktop release/web release: about 0.2 to
  3 ms of script while peeling or dragging the bag; on the web the 144 Hz frame rate holds. See the
  M7 notes in the decision log.

### Templates and the parallax demo (M8)

All built only on the public API; each has a headless test that plays it (`desktop_os.rs`,
`text_rpg.rs`, `parallax.rs`, `template.rs`), and `KULISA_SHOTS=<dir>` makes those tests save
screenshots.

- **`templates/desktop-os`**: a story told through a fake desktop. Draggable, resizable windows
  with focus and z-order (the M8 rect screens), a taskbar and start menu, desktop icons, a chat app
  driven by the story (typing indicator, replies as choices), a browser with fake pages, links and
  history, notes, settings, notifications; a shader wallpaper.
- **`templates/text-rpg`**: an illustration panel (crossfading pictures), a scrolling text log,
  choices, a stats panel, an inventory, save/load; rules in Luau (`give`, `take`, `has`, `heal`,
  `hurt`, `pay`, `roll`) called from the `.kulisa` story; landscape and phone portrait layouts.
- **`demos/parallax`**: an eleven-layer valley (shader sky with a day-night cycle, three mountain
  ranges rendered from heightfields into data textures that a shader colours and lights for the
  time of day, forest and grass strips with solid fills under them, a cabin), GPU particles
  (fireflies, leaves, snow, smoke, embers), a camera that follows the pointer, the phone's tilt or
  drifts; a depth-map diorama (steep parallax with a refine step); post-processing chains on the
  world layer only (bloom, tilt-shift, grading presets, CRT, grain), all working under WebGL2; the
  display lab (§3).
- **classic-vn** was reworked into a cat-themed sample (two cats, a kitchen and a playroom, warm
  colours, paw-print marks); characters are sized per tag (`config.sprite_scale`) and the swirl
  transition is computed in the shader (a low-resolution mask stretched over the screen banded).
- **Engine features it needed** (all in API.md): rect screens, `ui.place`/`ui.set_z`, `on_press`,
  `on_drag`, `block`, `clip`, `cursor`, `kulisa.input.cursor`, `window:resize`; effect chains
  with `scale`, `input` and `mask` (§4); GPU particles (§4); `kulisa.story.vars()` from helpers
  called by story lines.
- **Verified**: Windows desktop and the web (WebGPU and `?webgl`) for desktop-os and parallax
  (parallax: about 7 ms per frame and 0.01 ms of script on desktop release, CRT checked on both
  web backends); text-rpg and classic-vn headless (with screenshots). iOS, Linux and macOS through CI.
- **Gaps**: the Android emulator was not run for the M8 templates and the demo; text-rpg was not
  checked on the web; the art is fixed images (no in-repo sources, see the decision log).

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
- **Thumbnails** (M5): `save.write(slot, { thumbnail = image })` stores a PNG
  of a `screen:capture`. Native platforms wait for the pixels; the web writes the
  save at once and adds the thumbnail when the readback arrives. `save.list()`
  decodes each thumbnail once per save into a texture that is freed when the
  slot is overwritten or deleted.
- **Persistent data** (M5): `kulisa.persist` keeps plain data per game outside
  saves and rollback (`<data>/<id>/persistent`, or localStorage); the engine keeps
  the seen set there too, flushed every few seconds and on suspend.

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
- Game-creator builds go through `kulisa build <target>` (`kulisa-cli`); engine-developer
  builds through `cargo xtask` (`web`, `apk`, `ios-app`, `pack`), a shim over the same code
  with the smoke demo and `dist/` as defaults, so CI and local builds run the same steps.
- Games ship scripts and assets inside the build (App Store safe), packed into
  a single archive (`.kpak`: index + blobs). `cargo xtask` bundles a game
  (`--game DIR`, default `demos/smoke`) into web, APK and iOS builds;
  `cargo xtask pack` writes a `.kpak`. Desktop runs a folder directly.
- Mod loading is desktop only.

### CLI, live reload and quality tiers (M9)

**Game config.** `kulisa.toml` (a TOML subset parsed in `kulisa-platform`, no new
dependency) holds what is needed before scripts run or without them: `[game]` id, title,
version, build number; `[display]` (exactly the `kulisa.display{}` table, applied before
`main.luau`); `[window]` size, fullscreen, background; `[quality]`; `[android]` package and
`[ios]` bundle id. The runtime titles and sizes the window from it, `kulisa build` writes the
Android manifest (package, label, version, `screenOrientation`), the iOS plist (id, name,
versions, orientations) and the web page (title, background) from it, and scripts see it as
`kulisa.config`.

**CLI.** `kulisa-cli` is a library plus the `kulisa` binary and depends only on
`kulisa-platform`: it drives cargo in the engine's source tree (found where the tool was
built, or `KULISA_ENGINE`). `kulisa new NAME --template T` copies `templates/T` and sets
the id and title; `kulisa run` is `cargo run -p kulisa-player -- --dev GAME` (or `--web`:
build, then a dev server; `--android`: APK, `adb install`, start); `kulisa build
windows|linux|macos|web|android|ios` puts results in `GAME/dist/TARGET` (desktop: the
player renamed after the game with `game.kpak` beside it). Packs, the watcher and the dev
server skip `dist/`, hidden files and editor leftovers.

**Live reload.** Development runs watch the game. Desktop polls file sizes and times every
250 ms (no OS watcher dependency) and reports changes once they settle for 150 ms; the web
page polls the dev server's `__kulisa/version` every second, and the server repacks the
folder when its stamp changes, so the runtime gets a new pack and diffs it. `Game::reload`
swaps shaders (renderer `reload_shader`), images (`replace_texture`, same id) and sounds in
place, and recompiles stories, continuing at the current line through the update-safe save
path. Anything else restarts: the story is saved to a hidden slot, the game and renderer are
dropped and recreated (simpler than resetting every engine table), and the new game gets a
`reload` event with the slot to load. Errors go to the error view and leave the old version
running.

**Error view.** `kulisa-lua` keeps the last errors and draws them as a separate overlay scene
in window pixels after the frame (`extract_overlay`), so camera, letterbox and screen effects
never touch it. The font is a 5x7 ASCII bitmap in the source, scaled by whole pixels, so the
view works with no game assets. It shows startup failures and errors no `error` handler takes;
development runs show every error.

**App icons** (added with M10). `[game] icon` (default `icon.png`) is decoded by the runtime for
desktop windows; `kulisa build` turns it into a Windows `.ico` (PNG entries 16 to 256 px)
that the player's build script compiles into a resource with `llvm-rc` or the SDK's `rc`
(no new crate), an adaptive Android icon (the art as the 432 px background layer, plus a
192 px legacy icon, compiled with `aapt2`), iOS `AppIcon*` PNGs listed in the plist (no
asset catalog: enough for devices and the simulator, not for the App Store) and the web
page's icon links. macOS gets no icon until there is a `.app`.

**Quality tiers.** The tier (low, medium, high) comes from the adapter (software and GL low,
discrete and Apple silicon high, phones and browsers medium) unless the config, an env var or
the URL names one. The engine applies one knob itself, a render scale that caps the pixels
drawn per frame: window passes draw into a smaller screen texture and present stretches it,
while geometry, layout, `in.pixel` and `resolution` stay in window units (the projection
does the mapping), so input and shaders are unaffected. In `auto` mode a 3 s window under
40 fps steps the tier down. Games read `kulisa.quality` for their own costs.

### Ren'Py import (M10)

`kulisa import RENPY` makes a new game from the classic-vn template, replaces its story with
the converted scripts, copies the art and sounds, and writes `IMPORT.md`. The converter
(`kulisa-renpy`, no engine dependencies) works on logical lines (brackets and triple-quoted
strings joined, comments stripped) and converts block by block, re-indenting to four spaces:

- **Statements**: labels (parameters dropped), say lines (character, attributes as a `show`
  of the character's image tag, string speakers as generated characters, `extend`,
  narration from `narrator`, `centered` and nameless characters), `menu` (caption, guarded
  choices), `if`/`elif`/`else`, `jump`/`call`/`return`, `scene`/`show`/`hide`/`with`
  (positions and transitions mapped to the template's), `play`/`queue`/`voice`/`stop`
  (Ren'Py audio names and the `audio/` folder resolved), `pause`, `$` lines and `python:`
  blocks of simple assignments, `define`/`default` (hoisted out of labels).
- **Python to Luau**: literals, `and`/`or`/`not`, `!=`, `**`, string `+`, lists, dicts,
  calls with keyword arguments as a trailing table (`Character("Eileen", { color = ... })`),
  a few builtins. Indexing (0- vs 1-based), methods, comprehensions, `in`, formatting and
  f-strings are reported instead of guessed.
- **Text**: shared tags stay; `{w}`, `{p}`, `{size}`, `{cps=*n}` map; the rest are dropped
  with their closing tags. `[name!t]` keeps the expression.
- **Left out, with a note**: screens, transforms (ATL), styles, translations, `while`,
  Ren'Py's own APIs. Unconverted lines stay in the story as `# Ren'Py:` comments; blocks
  that end up empty get `pass` (added to the story language for this).
- **Assets**: files in `images/` are named the Ren'Py way (folders dropped, lowercase);
  `image` statements with a file name theirs. `options.rpy` and `gui.rpy` give the title,
  version, design size (`gui.init`) and icon (`config.window_icon`).

The result keeps Ren'Py's resolution (one design unit is one Ren'Py pixel, so art fits as
authored); classic-vn scales its interface with the design height so it looks the same at
1280x720 as at 1920x1080.

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
     M9 pins the minimums: Chrome/Edge 137, Firefox 131, Safari 18.4. The page validates a
     tiny module with `try_table` before loading and names them when it fails.
2. **Luau on iOS — resolved in M0 (simulator).** No JIT needed; C++ is built by
   the Xcode toolchain. CI builds the device target and runs the player in an
   iOS simulator: `cargo xtask ios-app --sim` assembles `dist/ios/Kulisa.app`,
   `.github/scripts/ios-sim-smoke.sh` boots a simulator, launches the app, takes
   a screenshot and fails unless the process is alive after 10 s and the Luau
   smoke-test line reached the unified log. Not yet run on a physical device
   (needs a signing identity and provisioning profile).
3. **Android text input — mostly fine in M6.** NativeActivity shows the soft keyboard through
   winit (`set_ime_allowed`) and Latin keyboards deliver key events, verified on the emulator.
   IME composition (CJK, predictive text) is not delivered; games needing it would switch to
   GameActivity (a Gradle step). Not done: it was not needed.
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
| 2026-10-19 | M5: basic sliders moved from M6 into the M5 UI system (the settings screen needs them); drag and drop, multi-touch, gamepad and focus navigation stay in M6. |
| 2026-10-19 | M5: screens rebuild whole and reconcile by key or position (per-screen granularity, as §9 planned) rather than fine-grained bindings: simpler, and cheap at VN UI sizes. Screens also rebuild after every story step instead of tracking story-variable reads. |
| 2026-10-19 | M5: `kulisa-ui` depends only on `kulisa-core`; text is measured through a trait and all scene nodes are made by `kulisa-lua`, so layout, reconciliation and input are unit-tested without fonts or a GPU. |
| 2026-10-19 | M5: transitions are an engine mechanism (freeze + shader overlay with progress) and the shaders live in templates, like the M1 vignette: the engine keeps no look. |
| 2026-10-19 | M5: the VM does not block on `with`; the engine freezes the screen the Ren'Py way and the template holds the next line until the transition ends. Keeps the VM free of timing. |
| 2026-10-19 | M5: skip, auto-forward and rollback controls are template behaviour over engine primitives (`seen`, `chosen`, `can_rollback`, `block_rollback`), so games can change them; classic-vn follows Ren'Py's defaults. |
| 2026-10-19 | M5: screen captures read back asynchronously everywhere (the web cannot block); native save writes may wait, the web patches the thumbnail in afterwards. |
| 2026-10-19 | M5: safe-area insets on Android come from `WindowInsets` through raw `jni-sys` calls (no `jni` crate API), since Android 15 forces edge-to-edge and the content rect no longer excludes the bars. |
| 2026-10-20 | M6: meshes reuse the sprite vertex format and the existing batcher; scripts edit them through bulk setters (tables or Luau buffers), not per-vertex userdata, so a 48x48 grid per frame stays cheap. |
| 2026-10-20 | M6: alpha hit testing decodes alpha lazily from the asset a texture came from (no CPU copy kept at load), and treats non-file textures as opaque. |
| 2026-10-20 | M6: pointers carry ids (mouse 0, fingers 1..); the UI stays single-pointer on the primary pointer, other fingers drive drag/hotspots/pinch. Multi-touch UI is not needed by anything yet. |
| 2026-10-20 | M6: draggables, drop zones and hotspots are engine mechanisms with Luau callbacks (accept functions included), so templates and games decide every look and rule. |
| 2026-10-20 | M6: keyboard/gamepad navigation is opt-in per screen (`nav`, modal by default) and only starts from direction input, so Enter/Space/A never pick things the player did not aim at. |
| 2026-10-20 | M6: gamepads through gilrs (needs libudev-dev on Linux CI); Android/iOS pads are left for later (gilrs has no backend). |
| 2026-10-20 | M6: text fields are Luau on top of `start_text` + `text`/`key` events; the engine only routes text, IME and the soft keyboard. Stayed on NativeActivity (Latin soft keyboards verified on the emulator). |
| 2026-10-20 | M6: Android sensors use the NDK on a dedicated looper thread (the activity's looper logs queue events as spurious); haptics call `Vibrator` through the existing raw JNI helpers. |
| 2026-10-20 | M7: meshes carry a per-vertex `data` vec4 (f16 on the GPU, 8 bytes per vertex for everything) instead of the demo packing normals into vertex colors: general, no color management, no per-mesh pipelines. |
| 2026-10-20 | M7: shaded sprites and meshes draw from an unpacked copy of their image (like masks), so UVs are 0..1 and procedural shaders get coordinates; they never batched with unshaded nodes anyway. |
| 2026-10-20 | M7: sounds gain `rate` and `pan` (kira playback rate and panning) for crinkle variation; pitch variety from a handful of samples instead of more assets. |
| 2026-10-20 | M7: film shading derives "which side am I seeing" from the winding of `uv` against the screen derivatives, so a flipped flap needs no extra vertex attribute. |
| 2026-10-20 | M8: windows are UI screens in a rectangle (`area`) moved by `ui.place` and raised by `ui.set_z`, with `on_drag` on any widget, instead of a separate window manager in the engine: desktop-os builds its own on top. |
| 2026-10-20 | M8: effects take a `scale` and read the chain's source or any earlier output (as `tex` or `mask_tex`); the renderer plans scratch textures with lifetimes. Enough for bloom and blur without compute or a render graph. |
| 2026-10-20 | M8: GPU particles without compute (birth records in an instance buffer, closed-form motion in the vertex shader), so WebGL2 runs the same particles as WebGPU. |
| 2026-10-20 | M8: Luau is compiled with `kulisa` as a mutable global, so `kulisa.screen.mode` and other engine paths read inside functions are never cached at load time. |
| 2026-10-20 | M8: embedded game nodes (`ui.node`) are all moved out of a dropped screen before its groups are removed, so hiding a screen hides them however deep they sit (text-rpg's illustration was being removed). |
| 2026-10-20 | M8: `show tag look` without `at` keeps the tag's place in the story's stage state too, as in Ren'Py; before, rollback and loads put it back at the default position. |
| 2026-10-20 | M7: the demo's PC and box are Blockbench renders made with one fixed, calibrated camera, so live meshes can be laid over them exactly; the room, glass and plastic are WGSL. Art sources (models, painting scripts) are kept outside the repository; the repository holds the finished images. |
| 2026-10-21 | M9: `kulisa.toml` uses a hand-written TOML subset parser (tables, dotted keys, strings, numbers, booleans, arrays, inline tables) instead of the `toml`/`serde` crates: the player reads it on every target, including wasm, and needs nothing more. |
| 2026-10-21 | M9: `[display]` in `kulisa.toml` is the `kulisa.display{}` table, applied before `main.luau`, so one description serves the runtime, the manifests and the plists. |
| 2026-10-21 | M9: `kulisa-cli` builds players from the engine source with cargo (and `cargo xtask` became a shim over it) rather than shipping prebuilt per-platform players; prebuilt "export templates" can come when the engine is distributed. |
| 2026-10-21 | M9: live reload restarts the whole game (new Lua state and renderer) for Luau changes and hands the game a hidden save to reload, instead of patching Luau modules in place: scripts hold state everywhere, and a restart plus the update-safe save path is predictable. Assets, shaders and stories reload in place. |
| 2026-10-21 | M9: the desktop watcher polls file times instead of using an OS file-watching crate; the web uses the dev server's version endpoint. |
| 2026-10-21 | M9: the error view uses a 5x7 bitmap font in the engine source (no bundled font file, works without game assets) and draws as an overlay pass outside the game's scene. |
| 2026-10-21 | M9: the only engine-side quality knob is a render scale (pixel budget per tier); particle counts, effect chains and the like stay game decisions read from `kulisa.quality`. |
| 2026-10-21 | M9: `Vm::load` binds defines and runs init blocks when the VM has not started (saves loaded from a main menu lost their characters), and saves record each label's statement count so a line edited in place resumes at its new version. |
| 2026-10-21 | M10: the Ren'Py importer converts what maps cleanly and leaves the rest as `# Ren'Py:` comments plus a notes file, rather than emulating Ren'Py's Python or ATL: the output is ordinary Kulisa a person can keep editing. |
| 2026-10-21 | M10: imports keep Ren'Py's resolution as the design size, and templates scale their interface to the design height, so imported art keeps its proportions. |
| 2026-10-21 | M10: `pass` joins the story language (Ren'Py has it; converted blocks can end up empty). |
| 2026-10-21 | App icons come from one square PNG in `kulisa.toml`; the Windows icon is embedded by the player's build script through `llvm-rc`/`rc` instead of a resource crate, and Android gets an adaptive icon with the art as its background layer. |
