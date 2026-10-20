# Paints the valley layers of demos/parallax: two pine-forest strips and a grass strip that tile
# horizontally (painted in neutral greys so the demo tints them for the time of day, with a lighter
# rim along the top edges), a cabin, and particle sprites (a leaf sheet, a smoke puff).
# Usage: python art/parallax/layers.py demos/parallax
import math, os, random, sys
from multiprocessing import Pool

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "common"))
from canvas import Canvas, clamp, mix, hexc, fbm, vnoise, hash2, sd_box, sd_triangle, sd_ellipse, sd_circle, sd_segment  # noqa: E402

OUT = sys.argv[1] if len(sys.argv) > 1 else "demos/parallax"


def path(name):
    os.makedirs(os.path.join(OUT, "images"), exist_ok=True)
    return os.path.join(OUT, "images", name)


def wrapped(w, x0, reach):
    """The copies of a shape at x0 that touch a strip of width w (for seamless tiling)."""
    return [x for x in (x0 - w, x0, x0 + w) if -reach <= x <= w + reach]


def pine_sdf(x, y, cx, base, h, seed):
    d = 1e9
    tiers = 6
    for i in range(tiers):
        t = i / (tiers - 1)
        top = base - h + h * 0.82 * t
        half = h * (0.07 + 0.2 * t)
        d = min(d, sd_triangle(x, y, (cx, top - h * 0.12), (cx - half, top + h * 0.17), (cx + half, top + h * 0.17)))
    d += (vnoise(x * 0.25, y * 0.25, seed) - 0.5) * 2.4
    trunk = sd_box(x, y, cx, base - h * 0.03, max(1.5, h * 0.02), h * 0.05)
    return min(d, trunk)


def forest(name, w, h, count, hmin, hmax, ground, shade, rim, seed):
    rnd = random.Random(seed)
    c = Canvas(w, h)
    trees = []
    for _ in range(count):
        trees.append((rnd.uniform(0, w), rnd.uniform(hmin, hmax), rnd.uniform(-6, 10)))
    trees.sort(key=lambda t: t[1])
    # Rolling ground the trees stand on (periodic in x).
    def ground_y(x):
        a = 2 * math.pi * x / w
        return h - ground + 10 * math.sin(a * 3 + seed) + 6 * math.sin(a * 7 + 1.3 * seed)
    c.shape(lambda x, y: ground_y(x) - y, [shade, shade, shade * 1.04, 1])
    for k, (tx, th, dy) in enumerate(trees):
        for cx in wrapped(w, tx, th * 0.4):
            base = ground_y(tx) + 6 + dy
            col = shade * (0.85 + 0.3 * hash2(k, 3, seed))
            c.shape(lambda x, y, cx=cx, base=base, th=th, k=k: pine_sdf(x, y, cx, base, th, k + seed),
                    [col, col, col * 1.05, 1], (cx - th * 0.4, base - th * 1.2, cx + th * 0.4, base + 4))
    # Rim light along the top edges: lighter where the pixel above is empty.
    px = c.px
    for y in range(h - 1, 0, -1):
        for x in range(w):
            p = px[y * w + x]
            if p[3] > 0.5:
                above = px[(y - 1) * w + x][3]
                above2 = px[max(0, y - 3) * w + x][3]
                k = clamp((1 - above) * 0.9 + (1 - above2) * 0.4)
                if k > 0:
                    for i in range(3):
                        p[i] = p[i] + (rim - p[i]) * k * 0.7
    c.save(path(name))


def grass(name, w, h, seed):
    rnd = random.Random(seed)
    c = Canvas(w, h)
    def ground_y(x):
        a = 2 * math.pi * x / w
        return h * 0.55 + 18 * math.sin(a * 2 + 0.7) + 9 * math.sin(a * 5 + 2.1)
    c.shape(lambda x, y: ground_y(x) + 8 - y, [0.1, 0.1, 0.11, 1])
    for _ in range(900):
        bx = rnd.uniform(0, w)
        bh = rnd.uniform(30, 120) * (0.6 + 0.6 * vnoise(bx / 90, 0.5, seed))
        lean = rnd.uniform(-0.45, 0.45)
        width = rnd.uniform(2.0, 4.5)
        col = rnd.uniform(0.07, 0.16)
        for cx in wrapped(w, bx, 60):
            base = ground_y(bx) + 14
            tipx, tipy = cx + lean * bh, base - bh
            midx = cx + lean * bh * 0.35
            c.shape(lambda x, y, cx=cx, base=base, tipx=tipx, tipy=tipy, midx=midx, width=width:
                    max(sd_segment(x, y, cx, base, tipx, tipy) - width * clamp((y - tipy) / (base - tipy + 1e-3)), -1e9),
                    [col, col, col * 1.1, 1], (min(cx, tipx) - 6, tipy - 2, max(cx, tipx) + 6, base + 2))
    # A few flowers catching the light.
    for _ in range(40):
        fx = rnd.uniform(0, w)
        fy = ground_y(fx) - rnd.uniform(20, 70)
        stem_base = ground_y(fx) + 14
        for cx in wrapped(w, fx, 20):
            c.shape(lambda x, y, cx=cx, fy=fy, sb=stem_base: sd_segment(x, y, cx, sb, cx, fy) - 1.2, [0.1, 0.1, 0.1, 1],
                    (cx - 3, fy - 2, cx + 3, stem_base))
            c.shape(lambda x, y, cx=cx, fy=fy: sd_circle(x, y, cx, fy, 4.2), [0.62, 0.6, 0.66, 1], (cx - 6, fy - 6, cx + 6, fy + 6))
    c.save(path(name))


def cabin():
    w, h = 220, 190
    c = Canvas(w, h)
    g = 0.22
    c.shape(lambda x, y: sd_box(x, y, 110, 140, 78, 46, 2), [g, g, g * 1.05, 1])
    c.shape(lambda x, y: sd_triangle(x, y, (18, 100), (202, 100), (110, 28)), [g * 0.8, g * 0.8, g * 0.85, 1])
    c.shape(lambda x, y: sd_box(x, y, 160, 52, 9, 22, 1), [g * 0.9, g * 0.9, g * 0.9, 1])
    for wx in (70, 150):
        c.shape(lambda x, y, wx=wx: sd_box(x, y, wx, 138, 15, 15, 1.5), [0.08, 0.08, 0.09, 1])
    c.shape(lambda x, y: sd_box(x, y, 110, 158, 13, 28, 1.5), [0.1, 0.1, 0.11, 1])
    c.save(path("cabin.png"))
    # The lit windows, drawn additively by the demo at night.
    gl = Canvas(w, h)
    for wx in (70, 150):
        gl.shape(lambda x, y, wx=wx: sd_box(x, y, wx, 138, 13, 13, 1), [1.0, 0.78, 0.45, 1])
        gl.shape(lambda x, y, wx=wx: sd_box(x, y, wx, 138, 1.5, 13), [0.5, 0.3, 0.15, 1])
        gl.shape(lambda x, y, wx=wx: sd_box(x, y, wx, 138, 13, 1.5), [0.5, 0.3, 0.15, 1])
    gl.save(path("cabin_windows.png"))


def leaves():
    # A 4x1 sheet of leaves in autumn colors (each 64 px).
    s = 64
    c = Canvas(s * 4, s)
    cols = [hexc("#e8892e"), hexc("#c9452c"), hexc("#f2c14a"), hexc("#9a5a2a")]
    for i, col in enumerate(cols):
        ox = i * s
        blade = lambda x, y, ox=ox, i=i: max(sd_ellipse(x, y, ox + 32, 32, 22 - i * 2, 12 + i), -sd_ellipse(x, y, ox + 32, 50, 30, 10) if i == 1 else -1e9)
        c.shape(blade, lambda x, y, col=col, ox=ox: mix(col, [col[0] * 0.6, col[1] * 0.5, col[2] * 0.4, 1], clamp((y - 20) / 30)),
                (ox, 0, ox + s, s))
        c.shape(lambda x, y, ox=ox: max(sd_segment(x, y, ox + 12, 32, ox + 54, 32) - 0.9, -1e9), [0.35, 0.18, 0.1, 0.8], (ox, 0, ox + s, s))
    c.save(path("leaves.png"))


def smoke():
    s = 96
    c = Canvas(s, s)
    def f(x, y):
        d = math.hypot(x - 48, y - 48) / 44
        n = fbm(x / 14, y / 14, 3, 5)
        a = clamp(1 - d) ** 1.5 * (0.55 + 0.6 * n)
        return [1, 1, 1, clamp(a)]
    c.paint(f)
    c.save(path("smoke.png"))


def round_rect():
    # A white rounded rectangle for the demo's panel and buttons (nine-slice 10).
    c = Canvas(32, 32)
    c.shape(lambda x, y: sd_box(x, y, 16, 16, 16, 16, 9), [1, 1, 1, 1])
    os.makedirs(os.path.join(OUT, "gui"), exist_ok=True)
    c.save(os.path.join(OUT, "gui", "round.png"))


def job(name):
    if name == "forest_far":
        forest("forest_far.png", 2048, 360, 120, 90, 190, 70, 0.55, 0.85, 3)
    elif name == "forest_near":
        forest("forest_near.png", 2048, 520, 46, 220, 380, 90, 0.26, 0.6, 9)
    elif name == "grass":
        grass("grass.png", 2048, 260, 4)
    elif name == "misc":
        cabin()
        leaves()
        smoke()
        round_rect()
    return name


if __name__ == "__main__":
    with Pool(4) as pool:
        for n in pool.imap_unordered(job, ["forest_far", "forest_near", "grass", "misc"]):
            print("painted", n)
