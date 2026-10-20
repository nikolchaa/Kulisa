# The shipping box of demos/pc-unwrap as Blockbench models, split into layers the demo stacks with
# the live PC between them (same camera and framing as the PC, see scene.py):
#   box_back    floor, the far walls' insides, the bottom foam caps (under the PC)
#   foam_rear, foam_front    the top foam caps (lifted out by the player)
#   box_near    the two walls facing the camera, printed
#   flaps_far, flaps_near    the four top flaps, animated (clip "open"): the long flaps meet over
#               the middle (the tape runs along that seam), the short end flaps lie under them
#   flaps_near_long, flap_front    the near layer split in two, for the frames where the front
#               short flap is still closed under the far long flap
# The PC stands on the bottom caps, 3.8 cm up, inside a box of 55.6 x 56.8 x 31.6 cm.
#
# Usage: python art/pc-unwrap/box.py OUTDIR   (textures from art/pc-unwrap/textures and generated)
import math, os, random, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "common"))
from bb import Model  # noqa: E402
import scene  # noqa: E402
from canvas import Canvas, clamp, hexc, vnoise, sd_box, sd_circle, sd_segment, sd_triangle  # noqa: E402

OUT = sys.argv[1] if len(sys.argv) > 1 else "."
TEX = os.path.join(OUT, "box_textures")
OLD = os.path.join(HERE, "textures")
os.makedirs(TEX, exist_ok=True)

X0, X1, Z0, Z1, H, T = -27.8, 27.8, -15.8, 15.8, 56.8, 0.8
FLAP_LONG = 15.8
FLAP_SHORT = 14.0
PC_LIFT = 3.8  # the PC stands on the bottom foam caps

INK = hexc("#5c3e26")


def board(w, h, seed, base=(0.9, 0.72, 0.5)):
    c = Canvas(w, h)
    rnd = random.Random(seed)

    def f(x, y):
        n = (vnoise(x * 0.05, y * 0.05, seed) - 0.5) * 0.05 + (rnd.random() - 0.5) * 0.03
        fl = math.sin(y * 0.9) * 0.008
        return [clamp(base[0] + n + fl), clamp(base[1] + n + fl), clamp(base[2] + n + fl), 1]
    c.paint(f)
    return c


def arrows(c, cx, top, s):
    c.shape(lambda x, y: sd_triangle(x, y, (cx, top), (cx - s, top + s * 1.2), (cx + s, top + s * 1.2)), INK)
    c.shape(lambda x, y: sd_box(x, y, cx, top + s * 2.1, s * 0.35, s * 0.95), INK)
    c.shape(lambda x, y: sd_box(x, y, cx, top + s * 3.4, s * 1.1, s * 0.16), INK)


def tex_side():
    # The long side facing the camera, 55.6 x 56.8 cm: a line drawing of the tower, arrows,
    # a fragile mark and a shipping label.
    w, h = 512, 520
    c = board(w, h, 31)
    # The tower, drawn the way the product boxes do: outlines, a hint of the fans.
    ox, oy, sw, sh = 150, 120, 210, 230
    c.shape(lambda x, y: abs(sd_box(x, y, ox + sw / 2, oy + sh / 2, sw / 2, sh / 2, 4)) - 2.2, INK)
    c.shape(lambda x, y: sd_segment(x, y, ox, oy, ox + 46, oy - 30) - 2.2, INK)
    c.shape(lambda x, y: sd_segment(x, y, ox + 46, oy - 30, ox + sw + 46, oy - 30) - 2.2, INK)
    c.shape(lambda x, y: sd_segment(x, y, ox + sw, oy, ox + sw + 46, oy - 30) - 2.2, INK)
    c.shape(lambda x, y: sd_segment(x, y, ox + sw + 46, oy - 30, ox + sw + 46, oy + sh - 30) - 2.2, INK)
    c.shape(lambda x, y: sd_segment(x, y, ox + sw, oy + sh, ox + sw + 46, oy + sh - 30) - 2.2, INK)
    for k in range(3):
        cy = oy + 40 + k * 66
        c.shape(lambda x, y, cy=cy: abs(sd_circle(x, y, ox + sw - 30, cy, 24)) - 1.8, INK)
    c.shape(lambda x, y: sd_box(x, y, ox + 80, oy + 175, 70, 14, 2), hexc("#5c3e26", 0.5))
    c.shape(lambda x, y: sd_box(x, y, ox + 80, oy + 210, 70, 3), INK)
    arrows(c, 60, 40, 18)
    arrows(c, w - 60, 40, 18)
    # The label: a white sticker with a barcode and lines of text.
    lx, ly = 300, 400
    c.shape(lambda x, y: sd_box(x, y, lx + 80, ly + 45, 82, 46, 2), hexc("#efece4"))
    for i in range(34):
        bw = 1 + (i * 7) % 3
        x0 = lx + 12 + i * 3.6
        c.shape(lambda x, y, x0=x0, bw=bw: sd_box(x, y, x0 + bw / 2, ly + 30, bw / 2, 18), hexc("#222226"))
    for k in range(3):
        c.shape(lambda x, y, k=k: sd_box(x, y, lx + 12 + (100 - k * 26) / 2, ly + 62 + k * 9, (100 - k * 26) / 2, 2.2),
                hexc("#55555c"))
    # Fragile: a glass in red.
    red = hexc("#b02e28")
    gx, gy = 90, 420
    c.shape(lambda x, y: sd_box(x, y, gx, gy, 16, 3), red)
    c.shape(lambda x, y: sd_box(x, y, gx, gy + 24, 3, 22), red)
    c.shape(lambda x, y: sd_box(x, y, gx, gy + 48, 16, 3), red)
    c.save(os.path.join(TEX, "box_side.png"))


def tex_end():
    w, h = 288, 520
    c = board(w, h, 32)
    arrows(c, w / 2, 40, 22)
    # A specification panel: rows of short bars.
    for k in range(6):
        c.shape(lambda x, y, k=k: sd_box(x, y, 70 + 40, 240 + k * 26, 40, 3), INK)
        c.shape(lambda x, y, k=k: sd_box(x, y, 190 + 20, 240 + k * 26, 20 + (k * 13) % 14, 3), hexc("#5c3e26", 0.6))
    c.shape(lambda x, y: abs(sd_box(x, y, w / 2, 300, 120, 92, 4)) - 1.5, INK)
    c.save(os.path.join(TEX, "box_end.png"))


def textures():
    tex_side()
    tex_end()


def load(m):
    for n in ("cardboard", "cardboard_inner", "foam"):
        m.texture(n, os.path.join(OLD, n + ".png"))
    for n in ("box_side", "box_end"):
        m.texture(n, os.path.join(TEX, n + ".png"))


def keyframes(t0, t1, closed, opened, steps=12):
    keys = [(0.0, closed)]
    for k in range(steps + 1):
        f = k / steps
        e = f * f * (3 - 2 * f)
        keys.append((t0 + (t1 - t0) * f, tuple(closed[i] + (opened[i] - closed[i]) * e for i in range(3))))
    if t1 < 1.0:
        keys.append((1.0, opened))
    return keys


def flap_long(m, side):
    """The long flap on the near (-1) or far (+1) side, hinged on the outer top edge of its wall,
    so standing up it continues the wall board (no gap)."""
    z = Z0 if side < 0 else Z1
    with m.group("flap_long_%d" % side, (0, H, z)) as g:
        z0, z1 = (z - FLAP_LONG, z) if side < 0 else (z, z + FLAP_LONG)
        m.cube("flap", (X0, H - T, z0), (X1, H, z1), {"all": "cardboard", "down": "cardboard_inner"})
    # Near: up and over, hanging down in front. Far: up and back, standing open.
    closed, opened = ((180, 0, 0), (-62, 0, 0)) if side < 0 else ((-180, 0, 0), (-60, 0, 0))
    m.keyframes(g, "flap_long_%d" % side, keyframes(0.0, 0.55, closed, opened))


def flap_short(m, side):
    """The short flap at the front (-1) or rear (+1) end. The end walls are a board lower than the
    long ones, so the closed short flaps lie flush under the long flaps."""
    x = X0 if side < 0 else X1
    with m.group("flap_short_%d" % side, (x, H - T, 0)) as g:
        x0, x1 = (x - FLAP_SHORT, x) if side < 0 else (x, x + FLAP_SHORT)
        m.cube("flap", (x0, H - 2 * T, Z0 + T), (x1, H - T, Z1 - T), {"all": "cardboard", "down": "cardboard_inner"})
    closed, opened = ((0, 0, 180), (0, 0, 420)) if side < 0 else ((0, 0, 180), (0, 0, 60))
    m.keyframes(g, "flap_short_%d" % side, keyframes(0.35, 1.0, closed, opened))


def foam_cap(m, side, top):
    """A U-shaped foam end cap at the rear (+1) or front (-1) end, above (top) or under the PC."""
    s = side
    xa, xb = sorted((s * 16.0, s * 27.0))
    ea, eb = sorted((s * 23.4, s * 27.0))
    ia, ib = sorted((s * 16.0, s * 23.4))
    if top:
        y_slab, y_end = (PC_LIFT + 49.2, 56.0), (PC_LIFT + 42.0, PC_LIFT + 49.2)
    else:
        y_slab, y_end = (T, PC_LIFT), (PC_LIFT, PC_LIFT + 7.0)
    m.cube("slab", (xa, y_slab[0], -15.0), (xb, y_slab[1], 15.0), "foam")
    m.cube("end", (ea, y_end[0], -15.0), (eb, y_end[1], 15.0), "foam")
    m.cube("lip_near", (ia, y_end[0], -15.0), (ib, y_end[1], -11.9), "foam")
    m.cube("lip_far", (ia, y_end[0], 11.9), (ib, y_end[1], 15.0), "foam")


def model(name, build):
    m = Model(name)
    load(m)
    with scene.world(m):
        build(m)
    path = os.path.join(OUT, name + ".bbmodel")
    m.save(path)
    print("wrote", path)


def box_back(m):
    m.cube("floor", (X0, 0, Z0), (X1, T, Z1), "cardboard_inner")
    m.cube("wall_far", (X0, T, Z1 - T), (X1, H, Z1), {"all": "cardboard", "north": "cardboard_inner"})
    m.cube("wall_rear", (X1 - T, T, Z0 + T), (X1, H - T, Z1 - T), {"all": "cardboard", "west": "cardboard_inner"})
    foam_cap(m, 1, False)
    foam_cap(m, -1, False)


def box_near(m):
    m.cube("wall_near", (X0, T, Z0), (X1, H, Z0 + T), {"all": "cardboard", "north": "box_side", "south": "cardboard_inner"})
    m.cube("wall_front", (X0, T, Z0 + T), (X0 + T, H - T, Z1 - T), {"all": "cardboard", "west": "box_end", "east": "cardboard_inner"})
    m.cube("floor_edge", (X0, 0, Z0), (X1, T, Z0 + T), "cardboard")


if __name__ == "__main__":
    textures()
    model("box_back", box_back)
    model("box_near", box_near)
    model("foam_rear", lambda m: foam_cap(m, 1, True))
    model("foam_front", lambda m: foam_cap(m, -1, True))
    model("flaps_far", lambda m: (flap_long(m, 1), flap_short(m, 1)))
    model("flaps_near", lambda m: (flap_long(m, -1), flap_short(m, -1)))
    # While the short flaps are still closed (the first four frames) the front one lies under the
    # far long flap, so it goes in a layer of its own and the near long flap is rendered alone.
    model("flaps_near_long", lambda m: flap_long(m, -1))
    model("flap_front", lambda m: flap_short(m, -1))
    # Everything at once, to look at (not used by the demo).
    model("box_preview", lambda m: (box_back(m), box_near(m), foam_cap(m, 1, True), foam_cap(m, -1, True),
                                    flap_long(m, 1), flap_short(m, 1), flap_long(m, -1), flap_short(m, -1)))
