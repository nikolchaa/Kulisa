# Art sources for demos/pc-unwrap

The PC and its shipping box are Blockbench models built by scripts, rendered with one fixed
orthographic camera, and stacked by the demo as layers with its live meshes in between. Everything
here is original and generated, so the assets can be rebuilt.

| File | What |
|---|---|
| `bb.py` | A small `.bbmodel` writer: embedded textures, cubes with per-face textures, nested groups, rotation keyframes. |
| `scene.py` | The camera (pitch and yaw groups), the framing anchors every model shares, and the calibration markers. |
| `pc.py` | The PC (case, motherboard, cooler, RAM, graphics card, PSU shroud, fans, cables, top I/O) and its textures; also `calib.bbmodel`. |
| `box.py` | The box in layers: back with the bottom foam caps, near walls, top foam caps, animated flaps (`open` clip). |
| `textures/make_textures.py` | Cardboard and foam textures the box uses (run it first; the PNGs are not kept). |
| `postprocess.py` | Measures the projection from `calib.png`, cleans and crops the renders, writes `demos/pc-unwrap/images/pc/*` and `lib/art.luau`. |
| `pngtools.py` | PNG reading and writing, shadow removal, cropping, scaling. |
| `gen_audio.py` | Synthesizes the demo's sounds into `demos/pc-unwrap/audio`. |

Rebuilding:

1. `python textures/make_textures.py`, then `python pc.py OUT` and `python box.py OUT` (OUT is a
   folder the Blockbench renderer may read, e.g. `Documents/BlockbenchModels/pc2`).
2. Render with the Blockbench renderer (the MCP's `bbmodel_render`): view `front`, orthographic,
   `studio` lighting (`flat` for `calib`), transparent, 2048x2048, into `OUT/r` as `<name>_lit.png`
   (`calib.png` for the markers): `pc`, `box_back`, `box_near`, `foam_rear`, `foam_front`,
   `flap_front` (time 0), `flaps_far<i>` and `flaps_near<i>` for i = 0..11 at clip `open`, time
   i / 11, and `flaps_near_long<i>` for i = 0..3.
3. `python postprocess.py OUT/r demos/pc-unwrap`.
