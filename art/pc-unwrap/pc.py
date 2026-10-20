# The PC of demos/pc-unwrap as a Blockbench model: a black mid-tower with a glass side and a glass
# front (no pillar between them), a mesh top with front I/O, and a full build inside: motherboard
# on the tray, tower cooler with its fan, four RAM sticks with light bars, a graphics card, PSU
# shroud, three front intake fans and a rear exhaust fan with ARGB rings, braided cables. The glass
# panes themselves are left out: the demo draws them (and the films on them) live.
#
# Units are centimetres. The case spans x -23..23 (front at -23), y 0..49 (feet included) and
# z -11.5..11.5 (the glass side at -11.5). The camera (scene.py) looks at the glass side from the
# front-left and from above, orthographically; it sees the north (-z), west (-x) and up faces of
# everything.
#
# Usage: python art/pc-unwrap/pc.py OUTDIR   writes pc.bbmodel and calib.bbmodel (coloured markers
# on the case's corners, to measure the projection) and the textures into OUTDIR/pc_textures.
import math, os, random, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "common"))
from bb import Model  # noqa: E402
import scene  # noqa: E402
from canvas import Canvas, clamp, mix, hexc, vnoise, fbm, sd_box, sd_circle, sd_segment, sd_triangle  # noqa: E402

OUT = sys.argv[1] if len(sys.argv) > 1 else "."
TEX = os.path.join(OUT, "pc_textures")
os.makedirs(TEX, exist_ok=True)

BLACK = hexc("#15161a")
BLACK2 = hexc("#1b1c21")
PURPLE = hexc("#9b6cff")
CYAN = hexc("#2fd8f0")


def save(c, name):
    path = os.path.join(TEX, name + ".png")
    c.save(path)
    return path


def noisy(c, base, amt, seed, scale=1.0):
    def f(x, y):
        n = (vnoise(x * scale, y * scale, seed) - 0.5) * amt + (random.random() - 0.5) * amt * 0.5
        return [clamp(base[0] + n), clamp(base[1] + n), clamp(base[2] + n), 1]
    c.paint(f)


def rgb_at(t):
    """The ARGB gradient: purple through blue to cyan and back (t wraps)."""
    t = t % 1.0
    k = 0.5 - 0.5 * math.cos(t * 2 * math.pi)
    return mix(PURPLE, CYAN, k)


# ---- textures ------------------------------------------------------------------------------

def tex_black():
    random.seed(1)
    c = Canvas(64, 64)
    noisy(c, BLACK, 0.025, 1, 0.2)
    save(c, "black")
    c = Canvas(16, 16)
    noisy(c, hexc("#0a0a0c"), 0.01, 2)
    save(c, "frit")
    c = Canvas(16, 16)
    noisy(c, hexc("#0d0d0f"), 0.02, 3)
    save(c, "rubber")
    c = Canvas(4, 4, (0, 0, 0, 0))
    save(c, "clear")


def tex_top_mesh():
    w, h = 512, 256
    c = Canvas(w, h)
    noisy(c, BLACK, 0.02, 4, 0.1)
    # The perforated dust filter: a hex grid of round holes inside a solid border.
    pitch = 8.0
    for row in range(int(h / (pitch * 0.866)) + 2):
        y = 12 + row * pitch * 0.866
        off = (row % 2) * pitch / 2
        for col in range(int(w / pitch) + 2):
            x = 12 + off + col * pitch
            if 18 < x < w - 18 and 18 < y < h - 18:
                c.shape(lambda px, py, x=x, y=y: sd_circle(px, py, x, y, 2.6), hexc("#050506"), (x - 4, y - 4, x + 4, y + 4))
                c.shape(lambda px, py, x=x, y=y: abs(sd_circle(px, py, x, y + 0.6, 2.6)) - 0.4, hexc("#26282e", 0.6),
                        (x - 4, y - 4, x + 4, y + 5))
    # A bevel line round the border.
    c.shape(lambda x, y: abs(sd_box(x, y, w / 2, h / 2, w / 2 - 15, h / 2 - 15, 4)) - 0.8, hexc("#2c2e35"))
    save(c, "top_mesh")


def tex_tray():
    # The motherboard tray seen through the glass: x 22.2 (left) .. -22.4 (right), y 47.6 (top) .. 2.2.
    w, h = 512, 512
    c = Canvas(w, h)
    noisy(c, hexc("#1c1e23"), 0.02, 5, 0.05)
    X = lambda x: (22.2 - x) / 44.6 * w
    Y = lambda y: (47.6 - y) / 45.4 * h
    # Grommets beside the board (front side) and above it.
    for y0, y1 in ((16.0, 22.4), (24.6, 28.8), (29.4, 36.8), (38.4, 44.0)):
        cx, cy, hw, hh = X(-3.6), (Y(y0) + Y(y1)) / 2, 0.9 / 44.6 * w * 1.0 + 6, abs(Y(y1) - Y(y0)) / 2
        c.shape(lambda x, y, cx=cx, cy=cy, hw=hw, hh=hh: sd_box(x, y, cx, cy, hw, hh, 6), hexc("#08080a"))
        c.shape(lambda x, y, cx=cx, cy=cy, hh=hh: sd_box(x, y, cx, cy, 0.8, hh - 5), hexc("#000000"))
    for x0, x1 in ((12.0, 20.0),):
        cx, cy = (X(x0) + X(x1)) / 2, Y(46.6)
        c.shape(lambda x, y, cx=cx, cy=cy: sd_box(x, y, cx, cy, abs(X(x1) - X(x0)) / 2, 7, 5), hexc("#08080a"))
    # Screw holes for other board sizes and a few stamped ribs.
    rnd = random.Random(6)
    for _ in range(14):
        x, y = rnd.uniform(20, w - 20), rnd.uniform(20, h - 20)
        c.shape(lambda px, py, x=x, y=y: sd_circle(px, py, x, y, 2.2), hexc("#2a2c32"), (x - 4, y - 4, x + 4, y + 4))
    for k in range(3):
        y = Y(5 + k * 1.2)
        c.shape(lambda px, py, y=y: sd_box(px, py, X(-10), y, 70, 1.2, 1), hexc("#25272d"))
    save(c, "tray")


def tex_mobo():
    # x 22 (left) .. -2.4 (right), y 45.8 (top) .. 15.3; 384 x 480.
    w, h = 384, 480
    c = Canvas(w, h)
    noisy(c, hexc("#111418"), 0.02, 7, 0.08)
    X = lambda x: (22.0 - x) / 24.4 * w
    Y = lambda y: (45.8 - y) / 30.5 * h
    rnd = random.Random(8)
    # Traces: thin right-angled runs in a slightly lighter green-black.
    for _ in range(140):
        x, y = rnd.uniform(0, w), rnd.uniform(0, h)
        l = rnd.uniform(10, 60)
        col = hexc("#1a2229", 0.9)
        if rnd.random() < 0.5:
            c.shape(lambda px, py, x=x, y=y, l=l: sd_box(px, py, x + l / 2, y, l / 2, 0.5), col, (x - 1, y - 2, x + l + 1, y + 2))
        else:
            c.shape(lambda px, py, x=x, y=y, l=l: sd_box(px, py, x, y + l / 2, 0.5, l / 2), col, (x - 2, y - 1, x + 2, y + l + 1))
    # Silkscreen: small labels and outlines.
    for _ in range(40):
        x, y = rnd.uniform(10, w - 30), rnd.uniform(10, h - 10)
        lw = rnd.uniform(6, 22)
        c.shape(lambda px, py, x=x, y=y, lw=lw: sd_box(px, py, x + lw / 2, y, lw / 2, 1.0), hexc("#5d6470", 0.7),
                (x - 1, y - 2, x + lw + 1, y + 2))
    # Mounting holes (silver rings).
    for hx, hy in ((21.3, 45.0), (21.3, 30.0), (21.3, 16.0), (10.0, 45.0), (10.0, 30.0), (10.0, 16.0), (-1.6, 45.0),
                   (-1.6, 30.0), (-1.6, 16.0)):
        x, y = X(hx), Y(hy)
        c.shape(lambda px, py, x=x, y=y: abs(sd_circle(px, py, x, y, 5)) - 1.6, hexc("#a7adb5"), (x - 8, y - 8, x + 8, y + 8))
        c.shape(lambda px, py, x=x, y=y: sd_circle(px, py, x, y, 3), hexc("#050506"), (x - 4, y - 4, x + 4, y + 4))
    # PCIe slots with steel armour (the first sits under the graphics card, the second shows).
    for sy in (22.7, 16.6):
        y = Y(sy)
        c.shape(lambda px, py, y=y: sd_box(px, py, X(9.5), y, (X(2.0) - X(17.0)) / 2, 5, 1.5), hexc("#a3a9b2"))
        c.shape(lambda px, py, y=y: sd_box(px, py, X(9.5), y, (X(2.0) - X(17.0)) / 2 - 2, 2.2, 1), hexc("#1d1f24"))
    # M.2 heatsink between the CPU and the graphics card.
    x0, x1, y0, y1 = X(15.0), X(0.5), Y(27.6), Y(24.6)
    c.shape(lambda px, py: sd_box(px, py, (x0 + x1) / 2, (y0 + y1) / 2, (x1 - x0) / 2, (y1 - y0) / 2, 2),
            lambda px, py: mix(hexc("#2d3037"), hexc("#3b3f48"), (px - x0) / (x1 - x0)))
    for k in range(5):
        xx = x0 + 14 + k * 9
        c.shape(lambda px, py, xx=xx: sd_segment(px, py, xx, y0 + 6, xx + 10, y1 - 6) - 1.2, hexc("#1f2126"))
    c.shape(lambda px, py: sd_box(px, py, x1 - 50, (y0 + y1) / 2, 30, 2.2, 1), hexc("#7b8290"))
    # Chipset heatsink at the bottom front, the audio cover at the bottom rear.
    cx, cy = X(3.5), Y(18.4)
    c.shape(lambda px, py: sd_box(px, py, cx, cy, 42, 30, 4), hexc("#2a2d33"))
    c.shape(lambda px, py: sd_box(px, py, cx, cy, 30, 3, 1.5), hexc("#8b6cff"))
    cx, cy = X(19.5), Y(18.5)
    c.shape(lambda px, py: sd_box(px, py, cx, cy, 30, 46, 3), hexc("#25282e"))
    for k in range(4):
        yy = cy - 30 + k * 18
        c.shape(lambda px, py, yy=yy: sd_circle(px, py, cx + 10, yy, 5), hexc("#c9a64a"), (cx, yy - 8, cx + 20, yy + 8))
    # DIMM slots (mostly under the sticks) and the 24-pin socket's footprint.
    for i in range(4):
        xx = X(1.2 + 0.68 * i)
        c.shape(lambda px, py, xx=xx: sd_box(px, py, xx, Y(35.1), 3, (Y(28.4) - Y(41.9)) / 2, 1), hexc("#2a2c31"))
    save(c, "mobo")


def tex_heatsink():
    c = Canvas(128, 64)
    noisy(c, hexc("#2a2d33"), 0.03, 9, 0.3)
    for k in range(7):
        y = 6 + k * 8.5
        c.shape(lambda x, py, y=y: sd_box(x, py, 64, y, 62, 1.2), hexc("#17191d"))
        c.shape(lambda x, py, y=y: sd_box(x, py, 64, y + 2, 62, 0.6), hexc("#3d414a"))
    c.shape(lambda x, y: sd_box(x, y, 22, 32, 14, 2), hexc("#9b6cff", 0.8))
    save(c, "heatsink")


def tex_fins():
    c = Canvas(64, 256)
    for y in range(256):
        for x in range(64):
            k = (y % 6)
            v = 0.62 if k < 2 else (0.18 if k > 3 else 0.42)
            v += (vnoise(x * 0.2, y * 0.05, 10) - 0.5) * 0.06
            c.px[y * 64 + x] = [v * 0.95, v, v * 1.06, 1]
    save(c, "fins")


def tex_cooler_cap():
    # The cooler's top plate (it faces the glass): 6.0 x 12.4 cm, so 96 x 200 keeps circles round.
    w, h = 96, 200
    c = Canvas(w, h)
    def brushed(x, y):
        n = (vnoise(x * 0.04, y * 2.5, 11) - 0.5) * 0.05
        return [0.075 + n, 0.08 + n, 0.09 + n, 1]
    c.paint(brushed)
    c.shape(lambda x, y: abs(sd_box(x, y, w / 2, h / 2, w / 2 - 3, h / 2 - 3, 4)) - 0.8, hexc("#3a3d45"))
    # Six nickel-plated heatpipe tips poking through in two staggered rows.
    for i in range(3):
        for j in range(2):
            x, y = 30 + j * 36, 52 + i * 44 + j * 20
            c.shape(lambda px, py, x=x, y=y: sd_circle(px, py, x + 0.8, y + 1.2, 5.6), hexc("#000000", 0.5),
                    (x - 8, y - 8, x + 9, y + 9))
            c.shape(lambda px, py, x=x, y=y: sd_circle(px, py, x, y, 5.2), lambda px, py, x=x, y=y:
                    mix(hexc("#eef1f5"), hexc("#7d838c"), clamp((py - y + 5 + (px - x) * 0.4) / 10)),
                    (x - 7, y - 7, x + 7, y + 7))
    c.shape(lambda x, y: sd_box(x, y, w / 2, 22, 22, 2.4, 1.2), hexc("#c8ccd4"))
    save(c, "cooler_cap")


def tex_fan():
    s = 256
    c = Canvas(s, s)
    cx = cy = s / 2
    noisy(c, hexc("#121316"), 0.02, 12, 0.2)
    # The frame: rounded square, its corners with screw holes and rubber pads.
    for qx in (18, s - 18):
        for qy in (18, s - 18):
            c.shape(lambda x, y, qx=qx, qy=qy: sd_circle(x, y, qx, qy, 6), hexc("#050506"), (qx - 8, qy - 8, qx + 8, qy + 8))
            c.shape(lambda x, y, qx=qx, qy=qy: abs(sd_circle(x, y, qx, qy, 9)) - 1.2, hexc("#2b2d33"), (qx - 11, qy - 11, qx + 11, qy + 11))
    R = s * 0.465
    c.shape(lambda x, y: sd_circle(x, y, cx, cy, R), hexc("#060607"))
    # Blades: nine swept blades between the hub and the rim, lit a little on their leading edges.
    for k in range(9):
        a0 = k * 2 * math.pi / 9

        def blade(x, y, a0=a0):
            dx, dy = x - cx, y - cy
            r = math.hypot(dx, dy)
            if r < s * 0.15 or r > R - 3:
                return 1.0
            a = math.atan2(dy, dx) - a0 - (r / R) * 0.9
            a = (a + math.pi) % (2 * math.pi) - math.pi
            return (abs(a) - 0.24) * r

        c.shape(blade, lambda x, y: mix(hexc("#25272d"), hexc("#3a3d46"), clamp((math.hypot(x - cx, y - cy) - s * 0.15) / R)))
    c.shape(lambda x, y: sd_circle(x, y, cx, cy, s * 0.15), hexc("#1b1c21"))
    c.shape(lambda x, y: abs(sd_circle(x, y, cx, cy, s * 0.1)) - 1.5, hexc("#3a3d46"))
    c.shape(lambda x, y: abs(sd_circle(x, y, cx, cy, R)) - 2.5, hexc("#1f2126"))
    save(c, "fan")
    # The ARGB ring alone (emissive, drawn just in front of the fan).
    g = Canvas(s, s)
    def ring(x, y):
        d = abs(math.hypot(x - cx, y - cy) - (R - 4))
        a = clamp(1.4 - d / 3.2)
        col = rgb_at(math.atan2(y - cy, x - cx) / (2 * math.pi))
        return [col[0], col[1], col[2], a]
    g.paint(ring)
    save(g, "fan_ring")
    side = Canvas(128, 32)
    noisy(side, hexc("#121316"), 0.02, 13, 0.3)
    for y in (8, 24):
        side.shape(lambda x, py, y=y: sd_box(x, py, 64, y, 60, 0.8), hexc("#26282e"))
    save(side, "fan_side")


def tex_dimm():
    # A stick's side: 4 cm deep (across) by 13.5 cm long (down).
    w, h = 64, 216
    c = Canvas(w, h)
    noisy(c, hexc("#16171b"), 0.02, 14, 0.2)
    c.shape(lambda x, y: sd_triangle(x, y, (0, 0), (0, 80), (64, 30)), hexc("#202228"))
    c.shape(lambda x, y: sd_triangle(x, y, (0, 216), (64, 216), (64, 130)), hexc("#202228"))
    c.shape(lambda x, y: sd_box(x, y, 34, 120, 9, 36, 2), hexc("#b9bec7"))
    c.shape(lambda x, y: sd_box(x, y, 34, 120, 2, 28, 1), hexc("#2a2c31"))
    save(c, "dimm")
    bar = Canvas(16, 128)
    for y in range(128):
        col = rgb_at(y / 128 * 0.5)
        for x in range(16):
            bar.px[y * 16 + x] = [col[0], col[1], col[2], 1]
    save(bar, "dimm_light")


def tex_gpu():
    # The side of the shroud, seen through the glass: rear (bracket end) on the left.
    w, h = 512, 104
    c = Canvas(w, h)
    def metal(x, y):
        n = (vnoise(x * 0.03, y * 0.6, 15) - 0.5) * 0.03
        return [0.16 + n, 0.17 + n, 0.19 + n, 1]
    c.paint(metal)
    # Angular facets, a dark window with the fan fins behind it, a wordmark bar.
    c.shape(lambda x, y: sd_triangle(x, y, (40, 104), (150, 104), (110, 20)), hexc("#22252b"))
    c.shape(lambda x, y: sd_triangle(x, y, (380, 104), (512, 104), (512, 10)), hexc("#22252b"))
    c.shape(lambda x, y: sd_box(x, y, 300, 60, 120, 22, 4), hexc("#08090b"))
    for k in range(40):
        x = 186 + k * 6
        c.shape(lambda px, py, x=x: sd_box(px, py, x, 60, 1.0, 20), hexc("#3b3f47"), (x - 2, 36, x + 2, 84))
    c.shape(lambda x, y: abs(sd_box(x, y, 300, 60, 120, 22, 4)) - 1.2, hexc("#4a4f59"))
    c.shape(lambda x, y: sd_box(x, y, 96, 52, 34, 5, 2), hexc("#c8ccd4"))
    c.shape(lambda x, y: sd_segment(x, y, 0, 6, 512, 6) - 1.5, hexc("#2c3038"))
    save(c, "gpu_side")
    # The backplate, seen from above: rear on the left again, the slot side at the bottom.
    w, h = 512, 224
    c = Canvas(w, h)
    def plate(x, y):
        n = (vnoise(x * 0.02, y * 0.5, 16) - 0.5) * 0.03
        return [0.07 + n, 0.075 + n, 0.085 + n, 1]
    c.paint(plate)
    for i in range(8):
        for j in range(5):
            x, y = 360 + i * 16, 60 + j * 24
            c.shape(lambda px, py, x=x, y=y: sd_box(px, py, x, y, 4, 9, 3), hexc("#000000"), (x - 6, y - 11, x + 6, y + 11))
    c.shape(lambda x, y: sd_box(x, y, 150, 112, 70, 6, 2), hexc("#7b8290"))
    c.shape(lambda x, y: abs(sd_box(x, y, w / 2, h / 2, w / 2 - 6, h / 2 - 6, 6)) - 1, hexc("#2c2f36"))
    save(c, "gpu_back")
    # The front end: shroud edge and the heatsink fins behind it.
    c = Canvas(160, 72)
    noisy(c, hexc("#14161a"), 0.02, 17, 0.2)
    for k in range(24):
        x = 12 + k * 5.6
        c.shape(lambda px, py, x=x: sd_box(px, py, x, 46, 1.0, 16), hexc("#737a85"), (x - 2, 28, x + 2, 64))
    save(c, "gpu_end")
    strip = Canvas(128, 8)
    for x in range(128):
        col = rgb_at(x / 128)
        for y in range(8):
            strip.px[y * 128 + x] = [col[0], col[1], col[2], 1]
    save(strip, "gpu_strip")


def tex_shroud():
    w, h = 512, 128
    c = Canvas(w, h)
    noisy(c, BLACK, 0.02, 18, 0.1)
    # Perforated vents at the front (right) and a raised wordmark plate at the rear (left).
    for i in range(22):
        for j in range(7):
            x, y = 360 + i * 6.5 + (j % 2) * 3.2, 26 + j * 11
            c.shape(lambda px, py, x=x, y=y: sd_circle(px, py, x, y, 2.2), hexc("#040405"), (x - 3, y - 3, x + 3, y + 3))
    # A raised plate with a slim light bar and a diamond mark.
    c.shape(lambda x, y: sd_box(x, y, 150, 64, 96, 14, 5), hexc("#1d1f24"))
    c.shape(lambda x, y: abs(sd_box(x, y, 150, 64, 96, 14, 5)) - 0.8, hexc("#30333b"))
    c.shape(lambda x, y: sd_box(x, y, 170, 64, 62, 1.6, 1), hexc("#8a919d"))
    c.shape(lambda x, y: abs(x - 82) + abs(y - 64) - 7, hexc("#9b6cff"))
    c.shape(lambda x, y: sd_segment(x, y, 0, 3, 512, 3) - 1, hexc("#2c2e35"))
    save(c, "shroud_side")
    w, h = 512, 256
    c = Canvas(w, h)
    noisy(c, hexc("#0c0d10"), 0.015, 19, 0.1)
    # Cable cut-outs next to the tray (top of the texture is the tray side).
    for x0 in (60, 300, 430):
        c.shape(lambda x, y, x0=x0: sd_box(x, y, x0, 26, 34, 12, 5), hexc("#050506"))
    save(c, "shroud_top")


def tex_misc():
    c = Canvas(32, 32)
    def braid(x, y):
        k = ((x + y) % 8 < 4) ^ ((x - y) % 8 < 4)
        v = 0.06 if k else 0.16
        return [v, v, v * 1.1, 1]
    c.paint(braid)
    save(c, "sleeve")
    c = Canvas(64, 256)
    noisy(c, hexc("#202227"), 0.02, 20, 0.2)
    for k in range(6):
        y = 30 + k * 36
        c.shape(lambda x, py, y=y: sd_segment(x, py, 8, y, 56, y - 18) - 1.2, hexc("#30333a"))
    c.shape(lambda x, y: sd_box(x, y, 32, 236, 18, 2), hexc("#9b6cff"))
    save(c, "io_shroud")
    # The top front I/O plate, 13 x 3 cm along the front edge: a recess for the power button,
    # USB-C, two USB-A, the audio jack and the activity light, in a row.
    w, h = 260, 60
    c = Canvas(w, h)
    noisy(c, hexc("#0e0f12"), 0.02, 21, 0.3)
    cy = h / 2
    c.shape(lambda x, y: sd_circle(x, y, 22, cy, 15), hexc("#060607"))
    c.shape(lambda x, y: sd_box(x, y, 66, cy, 4.5, 9, 4.5), hexc("#c9ced6"))
    c.shape(lambda x, y: sd_box(x, y, 66, cy, 2.5, 7, 2.5), hexc("#050506"))
    for xx in (106, 146):
        c.shape(lambda x, y, xx=xx: sd_box(x, y, xx, cy, 7, 14, 1), hexc("#c9ced6"))
        c.shape(lambda x, y, xx=xx: sd_box(x, y, xx, cy, 5.5, 12.5, 0.5), hexc("#050506"))
        c.shape(lambda x, y, xx=xx: sd_box(x, y, xx - 2, cy, 1.8, 10), hexc("#2a5bd7"))
    c.shape(lambda x, y: abs(sd_circle(x, y, 190, cy, 6)) - 1.6, hexc("#c9ced6"))
    c.shape(lambda x, y: sd_circle(x, y, 190, cy, 3.6), hexc("#050506"))
    c.shape(lambda x, y: sd_circle(x, y, 226, cy, 2.6), hexc("#e8ebf0"))
    c.shape(lambda x, y: abs(sd_box(x, y, w / 2, h / 2, w / 2 - 1.5, h / 2 - 1.5, 4)) - 0.8, hexc("#2a2c32"))
    # Drawn along x for convenience; an up face takes its texture's width along x (the plate's
    # 3 cm) and its height along z (the 13 cm), so store it transposed.
    t = Canvas(h, w)
    for y in range(h):
        for x in range(w):
            t.px[x * h + y] = c.px[y * w + x]
    save(t, "top_io")
    # The power button's face: a brushed metal disc with the power symbol.
    s = 64
    c = Canvas(s, s)
    noisy(c, hexc("#0e0f12"), 0.01, 23, 0.3)
    c.shape(lambda x, y: sd_circle(x, y, 32, 32, 30), lambda x, y: mix(hexc("#d5d9df"), hexc("#7f858e"), clamp((y - 2) / 60)))
    c.shape(lambda x, y: sd_circle(x, y, 32, 32, 24), hexc("#202227"))
    def symbol(x, y):
        ring = abs(sd_circle(x, y, 32, 33, 11)) - 2.2
        if y < 30 and abs(x - 32) < 7:
            ring = 9
        return min(ring, sd_box(x, y, 32, 24, 2.2, 9, 1))
    c.shape(symbol, hexc("#e8ebf0"))
    save(c, "power_btn")
    c = Canvas(128, 256)
    noisy(c, BLACK, 0.02, 22, 0.2)
    # Inside of the rear panel: seven slot covers low down, the exhaust fan opening above.
    for k in range(7):
        y = 150 + k * 13
        c.shape(lambda x, py, y=y: sd_box(x, py, 64, y, 44, 4.5, 1), hexc("#25272d"))
    save(c, "rear_inner")


def textures():
    random.seed(0)
    tex_black()
    tex_top_mesh()
    tex_tray()
    tex_mobo()
    tex_heatsink()
    tex_fins()
    tex_cooler_cap()
    tex_fan()
    tex_dimm()
    tex_gpu()
    tex_shroud()
    tex_misc()


# ---- the model ------------------------------------------------------------------------------

def load(m, names, emissive=()):
    for n in names:
        m.texture(n, os.path.join(TEX, n + ".png"), emissive=n in emissive)


def fan(m, name, x0, x1, yc, zc, size):
    """A fan facing -x (blades on its west face) at x0..x1, centred on (yc, zc)."""
    h = size / 2
    m.cube(name, (x0, yc - h, zc - h), (x1, yc + h, zc + h),
           {"all": "fan_side", "west": "fan", "east": "fan"})
    m.cube(name + "_ring", (x0 - 0.05, yc - h, zc - h), (x0 - 0.02, yc + h, zc + h), {"west": "fan_ring"})


def build(path):
    m = Model("pc")
    load(m, ["black", "frit", "rubber", "top_mesh", "tray", "mobo", "heatsink", "fins", "cooler_cap",
             "fan", "fan_ring", "fan_side", "dimm", "dimm_light", "gpu_side", "gpu_back", "gpu_end", "gpu_strip",
             "shroud_side", "shroud_top", "sleeve", "io_shroud", "top_io", "power_btn", "rear_inner"],
         emissive=("fan_ring", "dimm_light", "gpu_strip"))
    with scene.world(m):
        parts(m)
    m.save(path)
    print("wrote", path, len(m.elements), "elements")


def build_calib(path):
    m = Model("calib")
    with scene.world(m):
        scene.markers(m)
    m.save(path)
    print("wrote", path)


def parts(m):
    with m.group("case"):
        m.cube("bottom", (-23, 1.2, -11.5), (23, 2.2, 11.5), "black")
        m.cube("top", (-23, 47.6, -11.5), (23, 49, 11.5), {"all": "black", "up": "top_mesh"})
        m.cube("rear", (22.2, 2.2, -11.5), (23, 47.6, 11.5), {"all": "black", "west": "rear_inner"})
        m.cube("tray", (-23, 2.2, 10.7), (22.2, 47.6, 11.5), {"all": "black", "north": "tray"})
        m.cube("pillar_front", (-23, 2.2, -11.5), (-22.5, 47.6, -11.0), "black")
        m.cube("pillar_back", (-23, 2.2, 10.9), (-22.4, 47.6, 11.5), "black")
        # The black print round the edges of the glass.
        m.cube("frit_side_top", (-22.5, 46.3, -11.5), (22.2, 47.6, -11.35), "frit")
        m.cube("frit_side_bottom", (-22.5, 2.2, -11.5), (22.2, 3.3, -11.35), "frit")
        m.cube("frit_side_rear", (21.2, 3.3, -11.5), (22.2, 46.3, -11.35), "frit")
        m.cube("frit_front_top", (-23, 46.3, -11.0), (-22.85, 47.6, 10.9), "frit")
        m.cube("frit_front_bottom", (-23, 2.2, -11.0), (-22.85, 3.3, 10.9), "frit")
        for i, (x, z) in enumerate(((-19.5, -8.5), (19.5, -8.5), (-19.5, 8.5), (19.5, 8.5))):
            m.cube("foot%d" % i, (x - 1.8, 0, z - 1.6), (x + 1.8, 1.2, z + 1.6), "rubber")
        # Front I/O on the top, at the front edge, and the power button standing proud.
        m.cube("top_io", (-22.4, 49, -6.5), (-19.4, 49.08, 6.5), {"up": "top_io", "all": "black"})
        m.cube("power", (-21.4, 49.08, -6.0), (-20.4, 49.32, -5.0), {"all": "black", "up": "power_btn"})
    with m.group("inside"):
        m.cube("shroud", (-22.4, 2.2, -10.9), (22.2, 12.6, 10.7),
               {"all": "black", "north": "shroud_side", "up": "shroud_top"})
        m.cube("fan_bracket", (-20.4, 12.6, -10.9), (-20.0, 47.6, 10.7), "black")
        m.cube("mobo", (-2.4, 15.3, 10.2), (22.0, 45.8, 10.7), {"all": "black", "north": "mobo"})
        m.cube("io_shroud", (19.0, 33.0, 6.6), (22.0, 45.2, 10.2), {"all": "black", "north": "io_shroud", "west": "io_shroud"})
        m.cube("vrm_top", (6.0, 42.2, 7.6), (18.6, 44.6, 10.2), "heatsink")
        m.cube("vrm_left", (15.6, 30.0, 7.6), (18.6, 42.2, 10.2), "heatsink")
        m.cube("atx24", (-2.4, 29.6, 8.8), (-1.2, 36.4, 10.2), "black")
        m.cube("eps8", (18.0, 44.6, 9.2), (20.0, 45.6, 10.2), "black")
        # The tower cooler: fin stack, top cap, fan on its intake (front) side.
        m.cube("cooler", (6.2, 28.8, -3.9), (12.2, 41.2, 9.4), {"all": "fins", "north": "cooler_cap"})
        m.cube("cooler_base", (7.0, 32.0, 9.4), (11.4, 38.0, 10.2), "heatsink")
        fan(m, "cooler_fan", 4.6, 6.2, 35.0, 2.75, 12.4)
        # RAM: four sticks with light bars along their top edges.
        for i in range(4):
            x0 = 1.0 + 0.68 * i
            m.cube("dimm%d" % i, (x0, 28.4, 6.2), (x0 + 0.42, 41.9, 10.2),
                   {"all": "dimm", "north": "dimm_light", "up": "black"})
        # Graphics card: shroud side faces the glass, backplate on top, fins at the front end.
        m.cube("gpu", (-8.0, 17.2, -3.4), (21.8, 23.2, 10.0),
               {"all": "black", "north": "gpu_side", "up": "gpu_back", "west": "gpu_end"})
        m.cube("gpu_strip", (-6.0, 22.55, -3.47), (16.0, 22.95, -3.42), {"north": "gpu_strip"})
        m.cube("gpu_power", (-4.8, 23.2, -0.9), (-2.4, 23.8, 1.1), "black")
        # Braided cables, each out of a grommet in the tray and into its connector: GPU power
        # up from the card and back to the tray, the 24-pin round into the board's edge, the CPU
        # power down from the top cut-out.
        m.cube("cable_gpu_up", (-4.3, 23.8, -0.5), (-2.9, 27.2, 0.9), "sleeve")
        m.cube("cable_gpu_back", (-4.3, 25.8, -0.5), (-2.9, 27.2, 10.8), "sleeve")
        m.cube("cable_24_out", (-4.6, 30.0, 8.4), (-3.0, 36.0, 10.8), "sleeve")
        m.cube("cable_24_in", (-4.6, 30.0, 8.4), (-2.4, 36.0, 9.9), "sleeve")
        m.cube("cable_eps", (18.3, 45.6, 9.3), (19.7, 47.0, 10.8), "sleeve")
        # Airflow: three intake fans behind the front glass, one exhaust at the back.
        for i, yc in enumerate((18.6, 30.0, 41.4)):
            fan(m, "front_fan%d" % i, -22.2, -20.4, yc, -0.1, 11.2)
        fan(m, "rear_fan", 20.6, 22.2, 38.6, 0.4, 11.6)


if __name__ == "__main__":
    textures()
    build(os.path.join(OUT, "pc.bbmodel"))
    build_calib(os.path.join(OUT, "calib.bbmodel"))
