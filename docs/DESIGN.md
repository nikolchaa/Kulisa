# Kulisa design

Living document. Architecture, crate boundaries and the decisions behind them.
When a decision changes, update the relevant section and add a line to the
[decision log](#13-decision-log).

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
                         kulisa-runtime  (frame loop, owns all subsystems)
             ______________/  |   |   \______________
            /           /     |   |     \            \
   kulisa-story  kulisa-lua  kulisa-ui  kulisa-audio  kulisa-text
        |            |  \______|___________|______________|
        |            |         |
        +------> kulisa-core <-+--- kulisa-render
                     |
               kulisa-platform  (fs, paths, haptics, gyro, logging; no engine deps)

   kulisa-cli  (new | run | build <target>; host-only tool, not shipped in games)
```

| Crate | Owns | Depends on |
|---|---|---|
| `kulisa-platform` | Asset/file access, save paths, haptics, gyro, logging, safe areas | std, per-OS crates |
| `kulisa-core` | Scene graph, nodes, transforms, camera, viewport, frame extraction; later tweens/timelines, `Value` type, game state, rollback log | `glam`, `bytemuck` |
| `kulisa-render` | wgpu device, batcher, textures/atlases, meshes, shaders, render targets, post-fx | `wgpu`, `image`, `kulisa-core` |
| `kulisa-text` | Markup, cosmic-text shaping, glyph cache in the sprite atlas, typewriter, effects, ruby | `cosmic-text`, `kulisa-core`, `kulisa-render` |
| `kulisa-audio` | Channels (music/sound/voice/custom), crossfades, per-channel volume, placeholder synthesis | `kira` |
| `kulisa-story` | `.kulisa` lexer, parser, compiler, bytecode, VM | `kulisa-core` |
| `kulisa-lua` | Luau VM, sandbox, the `kulisa.*` API binding | `mlua`, engine crates |
| `kulisa-ui` | Reactive declarative screens, layout, widgets | `kulisa-core`, `kulisa-text` |
| `kulisa-runtime` | Frame loop, event dispatch, live reload, wiring | all of the above |
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

One `winit` event loop on the main thread. Per frame:

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
  behaviours written against public API (see the runtime smoke scene).
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
  scene ships Inter (SIL OFL 1.1, license next to the files).
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

### Language

```
define mila = Character("Mila", color="#e8a0bf")

label start:
    scene cafe_night with fade
    show mila smile at left
    mila "You actually came."
    menu:
        "Of course.":
            $ trust += 1
            jump cafe_talk
        "Got lost on the way.":
            jump lost_route
```

Indentation-based, Ren'Py-like. Statements: `label`, `jump`, `call`/`return`,
`menu` (with optional `if` guards per choice), `if/elif/else`, `define`,
`default`, `$` (inline Luau), `lua:` blocks, `scene`, `show`, `hide`,
`with <transition>`, `at <transform>`, `play/stop/queue music|sound|voice|<channel>`,
`pause`, and say lines (`who "text"` or narration `"text"`).

### Pipeline

`lexer → parser → AST → resolver → compiler → bytecode`.

- The lexer tracks file, line and column for every token. Errors carry a span
  and a plain-English message, rendered with the offending line and a caret.
  The parser recovers at statement boundaries so one run reports many errors.
- The resolver checks labels, duplicate definitions and unknown characters
  before compiling.
- Expressions and `$`/`lua:` code are not parsed by us: they become Luau chunks
  compiled once at load time and referenced by chunk ID.

### Bytecode

Compact ops, one stream per label:

```
Say { who, text }        Menu { choices: [(text, guard_chunk?, target)] }
Scene/Show/Hide { .. }   With { transition }       Play/Stop { channel, .. }
Pause { secs? }          Jump { label }            Call { label } / Return
JumpIfNot { chunk, off } Lua { chunk }             Checkpoint
```

The VM is a small state machine: a call stack of `(label, pc)` frames. It runs
until an op **blocks** and returns an event to the runtime. The engine never
draws dialogue itself: `Say` and `Menu` events call handlers that the game
registers from Luau (`kulisa.story.on_say`, `kulisa.story.on_menu`). The
classic-vn template registers handlers that drive its textbox and choice
screen. No handler is an error, not a fallback UI.

### Positions that survive updates

A story position is saved as `(label, statement_index, statement_hash)`, not as
a raw program counter. On load, if the hash at that index still matches, resume
there; otherwise resume at the nearest earlier statement in that label whose
hash matches, falling back to the label start. This keeps saves working when
writers edit scripts.

## 8. Luau

Luau via `mlua` with the `luau` feature. Chosen over Rhai for speed, coroutines,
gradual typing, sandboxing and no-JIT operation on iOS.

- **Sandboxed.** No `io`/`os`/`loadfile`; files go through `kulisa.assets`.
  Each game script runs in its own environment. Mod loading (desktop only) uses
  the same sandbox.
- **Coroutines for sequencing.** Waits (`kulisa.wait(secs)`, `tween:await()`,
  `kulisa.input.await_click()`) yield the current coroutine; the runtime
  resumes it when due.
- **API outline** (all under the `kulisa` global):

| Namespace | Purpose |
|---|---|
| `node`, `sprite`, `layer`, `group` | Create and modify scene nodes; transforms, opacity, tint, blend, clip, z |
| `mesh` | Vertex grids: positions, UVs, colors, indices editable per frame |
| `shader`, `postfx`, `target` | WGSL shaders, uniforms, post-fx chains, render targets |
| `camera` | Pan, zoom, rotate, shake, follow pointer/gyro |
| `tween`, `timeline` | Easing, sequences, parallel tracks, loops, callbacks |
| `text` | Rich text nodes, typewriter, effects, fonts |
| `audio` | Channels, play/stop/fade, volumes |
| `input` | Pointer, keys, gamepad, gyro; hit testing, drag and drop |
| `ui` | Reactive screens, layout, widgets, bindings |
| `story` | Event handlers, jump/call, story variables |
| `state`, `save` | Registered state, save/load slots, migrations |
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

### State model

Everything that must survive save/rollback is **plain data** in a `Value` tree
(`nil, bool, int, float, string, bytes, list, map`). Never live objects:

- VM call stack (as update-safe positions, see §7).
- Story variables (a Luau table restricted to plain data).
- **Scene description**: what is shown, where, with which transforms; music
  per channel and position. Not GPU objects; the scene is rebuilt from it.
- Registered Luau state: `kulisa.state.register(name, { save = fn, load = fn })`
  or the shortcut `kulisa.state.table(name, defaults)` which tracks a plain table.

### Rollback

At every blocking interaction (`Say`, `Menu`, `Pause`) the VM records a
**checkpoint**: the state above, encoded. Checkpoints form a ring buffer with
configurable depth (default 100). Story variables and registered state are
diffed against the previous checkpoint and only changes are stored, so deep
history is cheap. Rolling back restores a checkpoint and rebuilds the scene
from its description; rolling forward replays recorded menu choices.

### Save format

```
"KSAV" | format_version u16 | header len u32 | header | body
header: game id, game version, timestamp, slot name, playtime, thumbnail (PNG)
body:   self-describing tagged Value tree (current state + optional rollback log)
```

- Self-describing encoding (type tag + length per value, string-keyed maps), so
  unknown fields are skipped and missing fields fall back to defaults. No
  type-name serialization, no pickling, no code in saves.
- **Migrations**: games declare a save version and register
  `kulisa.save.migrate(from, fn)` hooks that transform the `Value` tree.
- Engine format changes bump `format_version` with Rust-side migrations.
- Platform-correct paths from `kulisa-platform` (AppData, XDG, Application
  Support, app sandbox, IndexedDB/OPFS on web).

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
- Games ship scripts and assets inside the build (App Store safe). Release
  builds pack assets into a single archive (`.kpak`: index + blobs).
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
| 2026-10-19 | iOS `.app` assembled by `cargo xtask ios-app` (plist + ad-hoc `codesign`) instead of an Xcode project; iOS logs go to `os_log` (`oslog` crate) since apps have no visible stderr. |
