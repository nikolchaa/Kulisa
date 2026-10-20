# Paints the art of templates/text-rpg: storybook illustrations for the scenes, inked item icons,
# a hero portrait, a die, and the ornamented panels the UI is built from. Pure Python
# (art/common/canvas.py). Usage: python art/text-rpg/paint.py templates/text-rpg
import math, os, random, sys
from multiprocessing import Pool

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "common"))
from canvas import (Canvas, clamp, smooth, mix, hexc, grad, fbm, vnoise, hash2, sd_circle, sd_box,  # noqa: E402
                    sd_segment, sd_ellipse, sd_triangle, sd_arc, cov, write_png)

OUT = sys.argv[1] if len(sys.argv) > 1 else "templates/text-rpg"
W, H = 800, 450

INK = hexc("#1a120c")
GOLD = hexc("#d9b26a")


def path(*p):
    full = os.path.join(OUT, *p)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    return full


# ---- illustration helpers -----------------------------------------------------------------------

def ridge(x, base, amp, freq, seed, octaves=4):
    return base - amp * fbm(x * freq, seed * 1.7, octaves, seed)


def add_glow(c, cx, cy, r, color, k):
    x0, x1 = max(0, int(cx - r * 3)), min(c.w, int(cx + r * 3))
    y0, y1 = max(0, int(cy - r * 3)), min(c.h, int(cy + r * 3))
    for y in range(y0, y1):
        for x in range(x0, x1):
            d2 = ((x - cx) ** 2 + (y - cy) ** 2) / (r * r)
            c.add(x, y, color, k * math.exp(-d2))


def pine(c, x0, base, h, color, seed):
    """A layered pine silhouette with a ragged edge."""
    rnd = random.Random(seed)
    tiers = 5
    def sdf(x, y):
        d = 1e9
        for i in range(tiers):
            t = i / tiers
            top = base - h + h * t * 0.85
            half = h * (0.12 + 0.28 * t)
            tri = sd_triangle(x, y, (x0, top - h * 0.18), (x0 - half, top + h * 0.22), (x0 + half, top + h * 0.22))
            d = min(d, tri)
        trunk = sd_box(x, y, x0, base - h * 0.04, h * 0.03, h * 0.06)
        ragged = (vnoise(x * 0.35, y * 0.35, seed) - 0.5) * 2.2
        return min(d + ragged, trunk)
    c.shape(sdf, color, (x0 - h * 0.45, base - h * 1.25, x0 + h * 0.45, base + 2))


def leafy(c, cx, cy, r, color, seed, light=None):
    def sdf(x, y):
        return math.hypot(x - cx, (y - cy) * 1.15) - r * (0.82 + 0.3 * fbm(math.atan2(y - cy, x - cx) * 1.3 + 9, seed, 3, seed))
    c.shape(sdf, color, (cx - r * 1.3, cy - r * 1.3, cx + r * 1.3, cy + r * 1.3))
    if light:
        c.shape(lambda x, y: max(sdf(x, y), sd_circle(x, y, cx - r * 0.35, cy - r * 0.4, r * 0.7)), light,
                (cx - r * 1.3, cy - r * 1.3, cx + r * 1.3, cy + r * 1.3))


def finish(c, seed, vignette=0.55, grain=0.035):
    """Paper grain, a soft vignette and a slight warm tone over everything."""
    for y in range(c.h):
        for x in range(c.w):
            p = c.px[y * c.w + x]
            u, v = x / c.w - 0.5, y / c.h - 0.5
            vg = 1 - vignette * (u * u * 1.2 + v * v * 1.6)
            n = (fbm(x * 0.08, y * 0.08, 2, seed) - 0.5) * grain + (hash2(x, y, seed) - 0.5) * grain * 0.6
            for i in range(3):
                p[i] = p[i] * vg + n
            p[0] *= 1.02
            p[2] *= 0.97


def sky(stops, cloud_col, cloud_k, seed, sun=None):
    def f(x, y):
        col = grad(stops, y / H)
        n = fbm(x / 160 + seed, y / 50, 5, seed)
        cl = smooth(0.52, 0.8, n) * cloud_k * (1 - y / H * 0.6)
        col = mix(col, cloud_col, cl)
        if sun:
            sx, sy, sr, scol, sk = sun
            d = math.hypot(x - sx, y - sy)
            col = [col[i] + scol[i] * sk * math.exp(-(d / sr) ** 2) for i in range(3)] + [1]
        return col
    return f


# ---- scenes -----------------------------------------------------------------------------------

def crossroads():
    c = Canvas(W, H)
    sun = (W * 0.62, H * 0.5, 120, hexc("#ffb36b"), 0.9)
    c.paint(sky([(0, hexc("#24264f")), (0.45, hexc("#7a4c78")), (0.7, hexc("#e48a63")), (1, hexc("#f6c27a"))],
                hexc("#d77d74"), 0.55, 3, sun))
    layers = [
        (H * 0.6, 60, 1 / 220, 1, hexc("#6d5677"), hexc("#e8a07f")),
        (H * 0.68, 45, 1 / 160, 2, hexc("#4b3d5c"), hexc("#c97f6a")),
        (H * 0.76, 35, 1 / 120, 3, hexc("#2f2840"), hexc("#a46356")),
    ]
    for base, amp, freq, seed, col, rim in layers:
        c.shape(lambda x, y, base=base, amp=amp, freq=freq, seed=seed: ridge(x, base, amp, freq, seed) - y,
                lambda x, y, base=base, amp=amp, freq=freq, seed=seed, col=col, rim=rim:
                    mix(col, rim, math.exp(-(y - ridge(x, base, amp, freq, seed)) / 3) * 0.6),
                (0, base - amp - 4, W, H))
    # Valley fog.
    for y in range(int(H * 0.62), int(H * 0.86)):
        for x in range(W):
            k = math.exp(-((y - H * 0.74) / 22) ** 2) * (0.25 + 0.35 * fbm(x / 90, y / 30, 3, 8))
            c.blend(x, y, hexc("#e9b5a0"), k * 0.6)
    # Forest edges on both sides.
    rnd = random.Random(5)
    for i in range(26):
        x0 = rnd.uniform(-20, W * 0.32) if i % 2 == 0 else rnd.uniform(W * 0.7, W + 20)
        hh = rnd.uniform(90, 170)
        pine(c, x0, H * 0.84 + rnd.uniform(-10, 14), hh, mix(hexc("#1d1a2a"), hexc("#3a2c3e"), rnd.random() * 0.4), i)
    # Ground and the forking road.
    c.shape(lambda x, y: H * 0.82 + 6 * math.sin(x / 70) - y, lambda x, y: mix(hexc("#3a2b2e"), hexc("#1a1418"), (y - H * 0.82) / 80))
    def road(x, y):
        t = clamp((y - H * 0.8) / (H * 0.2))
        cx = W * 0.5 + (1 - t) * 30 * math.sin(y / 30)
        half = 8 + t * 150
        main = abs(x - cx) - half
        left = abs(x - (cx - (H * 0.86 - y) * 2.4)) - (4 + t * 30) if y < H * 0.86 else 1e9
        right = abs(x - (cx + (H * 0.86 - y) * 2.2)) - (4 + t * 30) if y < H * 0.86 else 1e9
        return max(min(main, left, right), H * 0.8 - y)
    c.shape(road, lambda x, y: mix(hexc("#a87a62"), hexc("#5c4036"), clamp((y - H * 0.8) / 90) * 0.6 + 0.3 * fbm(x / 6, y / 6, 2, 4)))
    # Signpost.
    px, py = W * 0.56, H * 0.97
    post = lambda x, y: sd_box(x, y, px, py - 75, 5, 75, 1)
    c.shape(post, hexc("#2a1c16"))
    c.shape(lambda x, y: min(sd_box(x, y, px - 40, py - 125, 40, 9, 1.5), sd_triangle(x, y, (px - 80, py - 134), (px - 80, py - 116), (px - 92, py - 125))),
            hexc("#5a3e2c"))
    c.shape(lambda x, y: min(sd_box(x, y, px + 38, py - 103, 38, 8, 1.5), sd_triangle(x, y, (px + 76, py - 111), (px + 76, py - 95), (px + 88, py - 103))),
            hexc("#4e3526"))
    add_glow(c, px - 40, py - 128, 30, hexc("#ffc58a"), 0.12)
    # Fireflies.
    for i in range(18):
        fx, fy = rnd.uniform(60, W - 60), rnd.uniform(H * 0.62, H * 0.9)
        add_glow(c, fx, fy, rnd.uniform(1.5, 3), hexc("#ffe7a0"), 0.9)
    finish(c, 1)
    return c


def mill():
    c = Canvas(W, H)
    c.paint(sky([(0, hexc("#121a33")), (0.5, hexc("#2b3a5e")), (1, hexc("#596b8a"))], hexc("#7d8db0"), 0.45, 11,
                (W * 0.2, H * 0.18, 60, hexc("#dfe6ff"), 0.7)))
    c.shape(lambda x, y: sd_circle(x, y, W * 0.2, H * 0.18, 18), hexc("#f2f0e6"))
    c.shape(lambda x, y: ridge(x, H * 0.62, 50, 1 / 180, 4) - y, hexc("#2a3450"))
    rnd = random.Random(9)
    for i in range(22):
        x0 = rnd.uniform(-20, W * 0.42) if i % 3 else rnd.uniform(W * 0.8, W + 20)
        pine(c, x0, H * 0.73 + rnd.uniform(-6, 4), rnd.uniform(70, 150), mix(hexc("#0f1526"), hexc("#1d2640"), rnd.random()), 40 + i)
    # Stream with moonlight.
    c.shape(lambda x, y: H * 0.72 - y, lambda x, y: mix(hexc("#22304f"), hexc("#0c1222"), (y - H * 0.72) / (H * 0.28)))
    for y in range(int(H * 0.72), H):
        for x in range(W):
            k = math.exp(-((x - W * 0.2) / (20 + (y - H * 0.72) * 0.5)) ** 2) * smooth(0.55, 0.8, fbm(x / 12, y / 3, 3, 2))
            c.add(x, y, hexc("#c6d2ff"), k * 0.4)
    # The mill: stone base, timber upper floor, roof, lit window, wheel.
    mx, my = W * 0.58, H * 0.74
    c.shape(lambda x, y: sd_box(x, y, mx, my - 50, 110, 50, 2), lambda x, y: mix(hexc("#4a4552"), hexc("#2a2733"), fbm(x / 9, y / 7, 3, 3)))
    c.shape(lambda x, y: sd_box(x, y, mx, my - 135, 100, 36, 1), hexc("#3b2a26"))
    for k in range(-4, 5):
        c.shape(lambda x, y, k=k: sd_box(x, y, mx + k * 22, my - 135, 1.5, 36), hexc("#24181a"))
    c.shape(lambda x, y: sd_triangle(x, y, (mx - 128, my - 168), (mx + 128, my - 168), (mx, my - 250)), hexc("#1f1720"))
    c.shape(lambda x, y: sd_box(x, y, mx - 40, my - 130, 14, 16, 1), hexc("#ffcf7d"))
    c.shape(lambda x, y: sd_box(x, y, mx - 40, my - 130, 1.5, 16), hexc("#3b2a26"))
    add_glow(c, mx - 40, my - 130, 40, hexc("#ffb560"), 0.35)
    wx, wy = mx + 140, my - 40
    c.shape(lambda x, y: abs(sd_circle(x, y, wx, wy, 70)) - 6, hexc("#2c1f1a"))
    for k in range(12):
        a = k * math.pi / 6
        c.shape(lambda x, y, a=a: sd_segment(x, y, wx, wy, wx + math.cos(a) * 68, wy + math.sin(a) * 68) - 3, hexc("#2c1f1a"))
    c.shape(lambda x, y: sd_circle(x, y, wx, wy, 10), hexc("#1d1512"))
    # Reflected window light.
    for y in range(int(my + 2), min(H, int(my + 110))):
        for x in range(int(mx - 60), int(mx - 20)):
            k = smooth(0.5, 0.75, fbm(x / 8, y / 2, 3, 6)) * (1 - (y - my) / 110)
            c.add(x, y, hexc("#ffb560"), k * 0.35)
    finish(c, 2)
    return c


def cellar():
    c = Canvas(W, H)
    lx, ly = W * 0.36, H * 0.42
    def wall(x, y):
        row = int(y / 34)
        off = 30 if row % 2 else 0
        bx = (x + off) % 60
        by = y % 34
        mortar = min(bx, 60 - bx, by, 34 - by)
        stone = fbm(x / 14 + row * 3, y / 14, 3, row)
        base = mix(hexc("#3d3029"), hexc("#5b4a3c"), stone)
        base = mix(hexc("#1c1511"), base, smooth(0.5, 3.0, mortar))
        light = math.exp(-((x - lx) ** 2 + (y - ly) ** 2) / (2 * 190 ** 2))
        return [base[0] * (0.12 + 1.25 * light) + 0.05 * light, base[1] * (0.11 + 1.05 * light), base[2] * (0.12 + 0.8 * light), 1]
    c.paint(wall)
    # Floor.
    c.shape(lambda x, y: H * 0.78 - y, lambda x, y: mix(hexc("#3a2c22"), hexc("#120d0a"), (y - H * 0.78) / 100 + abs(x - lx) / W))
    # Barrels.
    for bx, by, s in ((W * 0.78, H * 0.8, 1.0), (W * 0.9, H * 0.84, 0.9), (W * 0.1, H * 0.83, 0.95)):
        body = lambda x, y, bx=bx, by=by, s=s: sd_ellipse(x, y, bx, by - 60 * s, 46 * s, 64 * s)
        c.shape(body, lambda x, y, bx=bx: mix(hexc("#5a3b25"), hexc("#20140c"), clamp(abs(x - bx) / 50 + 0.15)))
        for hy in (-100, -20):
            c.shape(lambda x, y, bx=bx, by=by, s=s, hy=hy: max(abs(y - (by + hy * s)) - 3 * s, body(x, y)), hexc("#151010"))
    # Stairs down from the top right.
    for k in range(6):
        c.shape(lambda x, y, k=k: sd_box(x, y, W * 0.63 + k * 26, H * 0.32 + k * 30, 70, 6), mix(hexc("#4a3a2e"), hexc("#191210"), k / 6))
    # A chest with a key glinting on it.
    cx, cy = W * 0.5, H * 0.82
    c.shape(lambda x, y: sd_box(x, y, cx, cy - 26, 62, 30, 4), hexc("#5b3a20"))
    c.shape(lambda x, y: sd_box(x, y, cx, cy - 62, 64, 12, 8), hexc("#6d4627"))
    for bx in (-40, 40):
        c.shape(lambda x, y, bx=bx: sd_box(x, y, cx + bx, cy - 40, 5, 45), hexc("#2b2a2e"))
    c.shape(lambda x, y: min(abs(sd_circle(x, y, cx - 8, cy - 80, 6)) - 2, sd_box(x, y, cx + 10, cy - 80, 14, 2)), hexc("#e8c46a"))
    add_glow(c, cx - 8, cy - 80, 8, hexc("#fff0b0"), 0.8)
    # The lantern on a hook.
    c.shape(lambda x, y: sd_segment(x, y, lx, ly - 70, lx, ly - 30) - 1.5, hexc("#1a1410"))
    c.shape(lambda x, y: sd_box(x, y, lx, ly, 14, 22, 3), hexc("#ffd88a"))
    c.shape(lambda x, y: abs(sd_box(x, y, lx, ly, 14, 22, 3)) - 2, hexc("#2a1d14"))
    add_glow(c, lx, ly, 34, hexc("#ffc56b"), 0.8)
    add_glow(c, lx, ly, 120, hexc("#ff9a40"), 0.12)
    finish(c, 3, 0.7)
    return c


def wolves():
    c = Canvas(W, H)
    moon = (W * 0.7, H * 0.22)
    c.paint(sky([(0, hexc("#060912")), (0.6, hexc("#141d33")), (1, hexc("#24314a"))], hexc("#3c4a68"), 0.3, 21,
                (moon[0], moon[1], 90, hexc("#b8c8ff"), 0.5)))
    c.shape(lambda x, y: sd_circle(x, y, moon[0], moon[1], 26), hexc("#eef2ff"))
    rnd = random.Random(17)
    # Trunks in three depths with mist between.
    for depth, (n, col, wmin, wmax) in enumerate(((16, hexc("#26304a"), 6, 12), (12, hexc("#161c2e"), 10, 20), (7, hexc("#080a12"), 22, 40))):
        for i in range(n):
            tx = rnd.uniform(0, W)
            tw = rnd.uniform(wmin, wmax)
            lean = rnd.uniform(-0.05, 0.05)
            c.shape(lambda x, y, tx=tx, tw=tw, lean=lean: abs(x - tx - (H - y) * lean) - tw * (1 + (y / H) * 0.3), col)
        for y in range(int(H * 0.45), H):
            for x in range(W):
                k = math.exp(-((y - H * (0.72 + depth * 0.08)) / 40) ** 2) * fbm(x / 120 + depth, y / 40, 3, depth)
                c.blend(x, y, hexc("#7f8fb8"), k * 0.25)
    # Ground.
    c.shape(lambda x, y: H * 0.9 + 8 * fbm(x / 40, 0, 3, 3) - y, hexc("#05070c"))
    # Eyes in the dark.
    for ex, ey in ((W * 0.3, H * 0.66), (W * 0.36, H * 0.7), (W * 0.82, H * 0.64)):
        for side in (-9, 9):
            c.shape(lambda x, y, ex=ex, ey=ey, side=side: sd_ellipse(x, y, ex + side, ey, 4, 2.4), hexc("#ffe27a"))
            add_glow(c, ex + side, ey, 6, hexc("#ffcf4a"), 0.5)
    finish(c, 4, 0.75)
    return c


def dawn():
    c = Canvas(W, H)
    sun = (W * 0.45, H * 0.6)
    c.paint(sky([(0, hexc("#6f8fc8")), (0.4, hexc("#f2b8b0")), (0.7, hexc("#ffd59a")), (1, hexc("#fff0c8"))],
                hexc("#fff3e6"), 0.5, 31, (sun[0], sun[1], 160, hexc("#fff0b0"), 0.8)))
    c.shape(lambda x, y: sd_circle(x, y, sun[0], sun[1], 28), hexc("#fffbe8"))
    for base, amp, freq, seed, col in ((H * 0.62, 40, 1 / 200, 5, hexc("#b6a6c8")), (H * 0.7, 30, 1 / 150, 6, hexc("#8fa08a")),
                                       (H * 0.8, 25, 1 / 120, 7, hexc("#5f7d4e"))):
        c.shape(lambda x, y, base=base, amp=amp, freq=freq, seed=seed: ridge(x, base, amp, freq, seed) - y, col)
    # A village: roofs, warm windows, chimney smoke.
    rnd = random.Random(3)
    for i in range(9):
        hx = W * 0.55 + i * 32 + rnd.uniform(-6, 6)
        hy = H * 0.77 + rnd.uniform(-4, 6)
        hw = rnd.uniform(12, 18)
        c.shape(lambda x, y, hx=hx, hy=hy, hw=hw: sd_box(x, y, hx, hy, hw, 10, 1), hexc("#e9d6b8"))
        c.shape(lambda x, y, hx=hx, hy=hy, hw=hw: sd_triangle(x, y, (hx - hw - 4, hy - 9), (hx + hw + 4, hy - 9), (hx, hy - 26)), hexc("#8a4636"))
        c.shape(lambda x, y, hx=hx, hy=hy: sd_box(x, y, hx - 4, hy + 1, 2.5, 3), hexc("#ffb85a"))
        if i % 3 == 0:
            for k in range(14):
                add_glow(c, hx + 6 + math.sin(k * 0.6) * 4 + k * 1.5, hy - 32 - k * 7, 5 + k * 0.6, hexc("#ffffff"), 0.05)
    # Meadow with flowers in the foreground.
    c.shape(lambda x, y: H * 0.86 + 10 * math.sin(x / 90) - y, lambda x, y: mix(hexc("#6f9a4c"), hexc("#2f4a26"), (y - H * 0.86) / 60))
    for i in range(160):
        fx, fy = rnd.uniform(0, W), rnd.uniform(H * 0.88, H)
        col = rnd.choice([hexc("#ffe8f0"), hexc("#ffd36b"), hexc("#d9a7ff")])
        c.shape(lambda x, y, fx=fx, fy=fy: sd_circle(x, y, fx, fy, 1.6 + (fy - H * 0.88) / 40), col, (fx - 6, fy - 6, fx + 6, fy + 6))
    finish(c, 5, 0.4)
    return c


SCENES = {"crossroads": crossroads, "mill": mill, "cellar": cellar, "wolves": wolves, "dawn": dawn}


def scene_job(name):
    SCENES[name]().save(path("images", name + ".png"))
    return name


# ---- icons (96 px, inked) -----------------------------------------------------------------------

def inked(name, parts):
    """parts: [(sdf, fill)]; draws a soft shadow, each part with a dark outline, then fills."""
    c = Canvas(96, 96)
    sh = Canvas(96, 96)
    for sdf, _ in parts:
        sh.shape(lambda x, y, sdf=sdf: sdf(x - 2, y - 3) - 2, [0, 0, 0, 0.45])
    sh.blur_alpha(2)
    c.over(sh)
    for sdf, fill in parts:
        c.shape(lambda x, y, sdf=sdf: sdf(x, y) - 2.2, INK)
        c.shape(sdf, fill)
    c.save(path("items", name + ".png"))


def shade(top, bottom, y0, y1):
    return lambda x, y: mix(hexc(top), hexc(bottom), clamp((y - y0) / (y1 - y0)))


def icons():
    inked("lantern", [
        (lambda x, y: sd_arc(x, y, 48, 20, 10, math.pi, 2 * math.pi) - 2, hexc("#8a7a6a")),
        (lambda x, y: sd_box(x, y, 48, 26, 18, 5, 2), shade("#b08a4a", "#6d5126", 21, 31)),
        (lambda x, y: sd_box(x, y, 48, 52, 16, 22, 4), shade("#ffe9a6", "#ffb04a", 30, 74)),
        (lambda x, y: sd_box(x, y, 48, 77, 20, 5, 2), shade("#b08a4a", "#6d5126", 72, 82)),
        (lambda x, y: sd_box(x, y, 48, 52, 1.5, 21), hexc("#6d5126")),
    ])
    inked("bread", [
        (lambda x, y: sd_ellipse(x, y, 48, 54, 34, 20), shade("#f2b56b", "#a8622b", 34, 74)),
        (lambda x, y: max(sd_ellipse(x, y, 48, 50, 30, 13), -sd_ellipse(x, y, 48, 60, 40, 12)), shade("#ffd9a0", "#e39a50", 37, 60)),
    ])
    inked("key", [
        (lambda x, y: abs(sd_circle(x, y, 30, 48, 13)) - 5, shade("#ffe08a", "#b8862c", 30, 62)),
        (lambda x, y: min(sd_box(x, y, 60, 48, 22, 4.5, 2), sd_box(x, y, 74, 57, 3.5, 6), sd_box(x, y, 64, 56, 3.5, 5)), shade("#ffe08a", "#b8862c", 42, 63)),
    ])
    inked("coins", [
        (lambda x, y: sd_ellipse(x, y, 40, 64, 22, 9), shade("#ffe08a", "#b07a24", 55, 73)),
        (lambda x, y: sd_ellipse(x, y, 56, 52, 22, 9), shade("#fff0a0", "#c08a30", 43, 61)),
        (lambda x, y: sd_ellipse(x, y, 44, 40, 22, 9), shade("#fff4b8", "#c99436", 31, 49)),
    ])
    inked("herb", [
        (lambda x, y: sd_segment(x, y, 48, 84, 50, 30) - 2, hexc("#4c7a30")),
        (lambda x, y: sd_ellipse(x, y, 34, 50, 14, 7), shade("#9fdc6a", "#3f7a2a", 43, 57)),
        (lambda x, y: sd_ellipse(x, y, 64, 42, 14, 7), shade("#a8e472", "#3f7a2a", 35, 49)),
        (lambda x, y: sd_ellipse(x, y, 38, 30, 11, 6), shade("#b4ec80", "#4a8a30", 24, 36)),
        (lambda x, y: sd_ellipse(x, y, 60, 64, 12, 6), shade("#9fdc6a", "#3f7a2a", 58, 70)),
    ])
    inked("rope", [
        (lambda x, y: abs(sd_circle(x, y, 48, 50, 24)) - 6, shade("#d8b47a", "#8a6538", 26, 74)),
        (lambda x, y: abs(sd_circle(x, y, 48, 50, 12)) - 4, shade("#e6c48a", "#9a7442", 38, 62)),
    ])
    inked("map", [
        (lambda x, y: sd_box(x, y, 48, 48, 32, 24, 3), shade("#f4e2b8", "#c9a66a", 24, 72)),
        (lambda x, y: sd_segment(x, y, 26, 62, 46, 40) - 1.6, hexc("#a8452e")),
        (lambda x, y: min(sd_segment(x, y, 58, 36, 68, 46), sd_segment(x, y, 68, 36, 58, 46)) - 2.2, hexc("#a8452e")),
    ])
    inked("dagger", [
        (lambda x, y: min(sd_triangle(x, y, (74, 16), (54, 30), (62, 40)), sd_triangle(x, y, (54, 30), (62, 40), (36, 58))), shade("#ffffff", "#9aa6b8", 16, 58)),
        (lambda x, y: sd_segment(x, y, 30, 50, 46, 66) - 4, hexc("#c9a24a")),
        (lambda x, y: sd_segment(x, y, 36, 62, 22, 76) - 4.5, shade("#6d3d24", "#3a2014", 60, 78)),
    ])
    # A die (d6 face) for checks.
    inked("die", [
        (lambda x, y: sd_box(x, y, 48, 48, 30, 30, 8), shade("#fff8ec", "#d9c9ad", 18, 78)),
        (lambda x, y: min(sd_circle(x, y, 34, 34, 5), sd_circle(x, y, 62, 62, 5), sd_circle(x, y, 48, 48, 5),
                          sd_circle(x, y, 62, 34, 5), sd_circle(x, y, 34, 62, 5)), hexc("#9a2e22")),
    ])


def portrait():
    s = 192
    c = Canvas(s, s)
    c.paint(lambda x, y: mix(hexc("#3b2a3e"), hexc("#14101a"), y / s))
    add_glow(c, s * 0.35, s * 0.4, 70, hexc("#ffb060"), 0.25)
    # Cloak and hood around a face in shadow.
    c.shape(lambda x, y: sd_ellipse(x, y, s / 2, s * 1.08, s * 0.48, s * 0.5), shade("#5a3c32", "#2a1a18", s * 0.6, s))
    hood = lambda x, y: sd_ellipse(x, y, s / 2, s * 0.46, s * 0.27, s * 0.33)
    c.shape(hood, lambda x, y: mix(hexc("#6a4436"), hexc("#2e1d18"), clamp((x - s * 0.3) / (s * 0.5))))
    c.shape(lambda x, y: sd_ellipse(x, y, s * 0.5, s * 0.52, s * 0.15, s * 0.2), shade("#c9906a", "#5a3626", s * 0.35, s * 0.72))
    c.shape(lambda x, y: max(sd_ellipse(x, y, s * 0.5, s * 0.36, s * 0.2, s * 0.13), y - s * 0.43), hexc("#2a1a16"))
    for ex in (0.44, 0.56):
        c.shape(lambda x, y, ex=ex: sd_ellipse(x, y, s * ex, s * 0.5, 2.6, 1.8), hexc("#1a0f0c"))
    c.shape(lambda x, y: sd_segment(x, y, s * 0.46, s * 0.62, s * 0.54, s * 0.62) - 1.1, hexc("#6d3a2a"))
    # A clasp.
    c.shape(lambda x, y: sd_circle(x, y, s / 2, s * 0.8, 7), shade("#ffe08a", "#a8782a", s * 0.76, s * 0.84))
    finish(c, 9, 0.6, 0.03)
    c.save(path("gui", "portrait.png"))


# ---- UI panels --------------------------------------------------------------------------------

def panel():
    # An ink panel with a gold hairline border and diamond ornaments at the corners. Nine-slice 24.
    s = 96
    c = Canvas(s, s)
    body = lambda x, y: sd_box(x, y, s / 2, s / 2, s / 2 - 1, s / 2 - 1, 6)
    c.shape(body, lambda x, y: mix(hexc("#2a1f1a", 0.96), hexc("#1a1310", 0.97), y / s))
    c.shape(lambda x, y: abs(sd_box(x, y, s / 2, s / 2, s / 2 - 7, s / 2 - 7, 3)) - 0.8, [*GOLD[:3], 0.75])
    c.shape(lambda x, y: abs(body(x, y) + 0.6) - 0.6, [0, 0, 0, 0.8])
    for cx, cy in ((7, 7), (s - 7, 7), (7, s - 7), (s - 7, s - 7)):
        c.shape(lambda x, y, cx=cx, cy=cy: abs(x - cx) + abs(y - cy) - 5, GOLD)
        c.shape(lambda x, y, cx=cx, cy=cy: abs(x - cx) + abs(y - cy) - 2, hexc("#2a1f1a"))
    c.save(path("gui", "panel.png"))


def simple(name, size, sdf, fill, border=None):
    c = Canvas(size, size)
    c.shape(sdf, fill)
    if border:
        c.shape(lambda x, y: abs(sdf(x, y) + 1) - 0.8, border)
    c.save(path("gui", name))


def ui():
    panel()
    s = 48
    # Choice button: dark with a gold edge (tinted in code for hover).
    simple("choice.png", s, lambda x, y: sd_box(x, y, s / 2, s / 2, s / 2, s / 2, 10), lambda x, y: mix([1, 1, 1, 1], [0.86, 0.86, 0.86, 1], y / s),
           [*GOLD[:3], 0.9])
    # Inventory slot: recessed square.
    c = Canvas(64, 64)
    c.shape(lambda x, y: sd_box(x, y, 32, 32, 31, 31, 8), lambda x, y: mix(hexc("#120d0b"), hexc("#2a201b"), y / 64))
    c.shape(lambda x, y: abs(sd_box(x, y, 32, 32, 31, 31, 8)) - 0.8, [*GOLD[:3], 0.35])
    c.shape(lambda x, y: max(abs(sd_box(x, y, 32, 33, 28, 28, 6)) - 1, 18 - y), [0, 0, 0, 0.35])
    c.save(path("gui", "slot.png"))
    simple("bar.png", 32, lambda x, y: sd_box(x, y, 16, 16, 16, 9, 9), [1, 1, 1, 1])
    simple("round.png", 32, lambda x, y: sd_box(x, y, 16, 16, 16, 16, 8), [1, 1, 1, 1])
    # A divider: a gold line with a diamond in the middle.
    c = Canvas(256, 16)
    c.shape(lambda x, y: max(abs(y - 8) - 0.6, abs(x - 128) - 124), lambda x, y: [*GOLD[:3], 0.7 * (1 - abs(x - 128) / 128)])
    c.shape(lambda x, y: abs(x - 128) + abs(y - 8) - 6, GOLD)
    c.shape(lambda x, y: abs(x - 128) + abs(y - 8) - 2.5, hexc("#1a1310"))
    c.save(path("gui", "divider.png"))
    # The illustration frame: a gold double border, transparent inside. Nine-slice 20.
    c = Canvas(80, 80)
    c.shape(lambda x, y: abs(sd_box(x, y, 40, 40, 39, 39, 4)) - 1.5, GOLD)
    c.shape(lambda x, y: abs(sd_box(x, y, 40, 40, 34, 34, 2)) - 0.6, [*GOLD[:3], 0.6])
    for cx, cy in ((4, 4), (76, 4), (4, 76), (76, 76)):
        c.shape(lambda x, y, cx=cx, cy=cy: abs(x - cx) + abs(y - cy) - 7, GOLD)
    c.save(path("gui", "frame.png"))


# ---- background behind the panels -------------------------------------------------------------------

def backdrop():
    c = Canvas(480, 270)
    c.paint(lambda x, y: mix(hexc("#1c1512"), hexc("#0b0807"), math.hypot(x / 480 - 0.5, y / 270 - 0.45) * 1.5))
    finish(c, 12, 0.3, 0.05)
    c.save(path("gui", "backdrop.png"))


if __name__ == "__main__":
    ui()
    icons()
    portrait()
    backdrop()
    with Pool(5) as pool:
        for n in pool.imap_unordered(scene_job, list(SCENES)):
            print("painted", n)
    print("done")
