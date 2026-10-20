# pc-unwrap

A brand-new PC in its bag, and the pleasure of unwrapping it. Pure Luau on the public `kulisa` API
(no engine special cases, no art assets: the PC, the room and the squeegee are WGSL, the printed
films are drawn into render targets, the sounds are synthesized).

```
cargo run -p kulisa-player -- demos/pc-unwrap
cargo xtask web --game demos/pc-unwrap --serve 8080      # also ?webgl
cargo xtask apk --game demos/pc-unwrap --abi arm64-v8a,x86_64
```

## What to do

1. **Slide the bag off.** Grab it anywhere and drag. It is simulated cloth: it lags behind your
   hand, bunches up, lifts into a tent. Pull it far enough and it slips away by itself.
2. **Peel the films and the sticker.** Grab the colored corner tab and pull. The fold rolls the
   film into a cylinder (the *Roll radius* slider changes it), sticks and slips as it peels
   (a click and a haptic tick each time), and a film you let go of keeps its fold: a small peel
   sticks back down, a big one hangs. Peel it all the way and it flutters away.
3. **Fit a fresh protector** (appears once the glass film is off). It lands with air bubbles
   trapped under it: drag the squeegee over it to iron it flat and push the bubbles to the edges.

Sounds and haptics follow how fast you drag: faster means more and louder crackles.

Keys: `R` starts over, `B` tosses the bag, `F3` hides the frame timer.

## How it works

| File | |
|---|---|
| `lib/geom.luau` | The case's visible faces (oblique projection) and their coordinate frames. |
| `lib/film.luau` | The peel: fold position, wrap angle, stick-slip, hanging, detaching and fluttering, all written into 48x48 mesh buffers each frame (no tables allocated). Header comment has the math. |
| `lib/bag.luau` | Verlet cloth over a 16x10 grid; compression becomes wrinkles. |
| `lib/protector.luau` | The spare film: landing, bubbles (smooth lenses), squeegee sweep. |
| `lib/prints.luau` | Printed layers drawn once into render targets and used as mesh textures. |
| `lib/sfx.luau` | Speed-dependent crinkle, rustle, squeak and pop playback, haptics. |
| `shaders/plastic.wgsl` | Fresnel, softbox reflection, sharp highlight, underside tint, wrinkle normals, edge glints, curl shadow, bubble lenses. |
| `shaders/case_*.wgsl`, `room.wgsl`, `squeegee.wgsl`, `shadow.wgsl` | The procedural art. |

The meshes carry a surface normal per vertex in `data` (read by the shader as `in.data`), and the
shader works out which side of the film you are looking at from the winding of `uv` on screen.
The F3 line shows frames per second and the milliseconds the script spends per frame.

The sounds in `audio/` were synthesized for this demo (filtered noise bursts and tones) and are
released under the repository's MIT license, as is Inter (SIL OFL 1.1, license next to the fonts).
