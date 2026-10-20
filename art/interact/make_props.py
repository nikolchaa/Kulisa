# Draws the props of demos/interact (coin, gem, ring, arrow, checkered flag) with a tiny shaded
# height-field renderer: each prop is a shape with a bevel profile, lit from the upper left.
# Pure Python, no dependencies. Usage: python make_props.py OUTDIR
import math, os, struct, sys, zlib

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)
SS = 2  # supersampling per axis


def write_png(path, w, h, rgba):
    rows = b"".join(b"\x00" + bytes(rgba[y * w * 4:(y + 1) * w * 4]) for y in range(h))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def clamp(v, a=0.0, b=1.0):
    return a if v < a else (b if v > b else v)


def smooth(a, b, x):
    t = clamp((x - a) / (b - a))
    return t * t * (3 - 2 * t)


def mix(a, b, t):
    return [a[i] + (b[i] - a[i]) * t for i in range(3)]


LIGHT = (-0.45, -0.55, 0.7)
_l = math.sqrt(sum(c * c for c in LIGHT))
LIGHT = tuple(c / _l for c in LIGHT)


def shade(normal, ramp, spec_color=(1.0, 1.0, 1.0), gloss=40.0, spec=0.7):
    """ramp: (dark, mid, light) colors 0..1."""
    nx, ny, nz = normal
    d = clamp(nx * LIGHT[0] + ny * LIGHT[1] + nz * LIGHT[2])
    col = mix(ramp[0], ramp[1], smooth(0.0, 0.55, d)) if d < 0.55 else mix(ramp[1], ramp[2], smooth(0.55, 1.0, d))
    # Blinn-Phong with the viewer straight on.
    hx, hy, hz = LIGHT[0], LIGHT[1], LIGHT[2] + 1.0
    hl = math.sqrt(hx * hx + hy * hy + hz * hz)
    s = clamp((nx * hx + ny * hy + nz * hz) / hl) ** gloss * spec
    return [clamp(col[i] + spec_color[i] * s) for i in range(3)]


def render(path, w, h, field):
    """field(x, y) -> None (outside) or (height, color callback args) ... see props below."""
    px = bytearray(w * h * 4)
    for y in range(h):
        for x in range(w):
            acc = [0.0, 0.0, 0.0]
            cov = 0
            for sy in range(SS):
                for sx in range(SS):
                    fx, fy = x + (sx + 0.5) / SS, y + (sy + 0.5) / SS
                    c = field(fx, fy)
                    if c is not None:
                        acc = [acc[i] + c[i] for i in range(3)]
                        cov += 1
            o = (y * w + x) * 4
            if cov:
                px[o:o + 3] = bytes(int(clamp(acc[i] / cov) * 255 + 0.5) for i in range(3))
                px[o + 3] = int(255 * cov / (SS * SS))
    write_png(os.path.join(OUT, path), w, h, px)
    print("wrote", path)


def normal_from(hf, x, y, scale=1.0):
    e = 0.75
    dx = hf(x + e, y) - hf(x - e, y)
    dy = hf(x, y + e) - hf(x, y - e)
    nx, ny, nz = -dx * scale, -dy * scale, 2 * e
    l = math.sqrt(nx * nx + ny * ny + nz * nz)
    return (nx / l, ny / l, nz / l)


def sd_polygon(p, pts):
    x, y = p
    d = (x - pts[0][0]) ** 2 + (y - pts[0][1]) ** 2
    s = 1.0
    n = len(pts)
    j = n - 1
    for i in range(n):
        ex, ey = pts[j][0] - pts[i][0], pts[j][1] - pts[i][1]
        wx, wy = x - pts[i][0], y - pts[i][1]
        t = clamp((wx * ex + wy * ey) / (ex * ex + ey * ey))
        bx, by = wx - ex * t, wy - ey * t
        d = min(d, bx * bx + by * by)
        c1, c2, c3 = y >= pts[i][1], y < pts[j][1], ex * wy > ey * wx
        if (c1 and c2 and c3) or (not c1 and not c2 and not c3):
            s = -s
        j = i
    return s * math.sqrt(d)


def star_points(cx, cy, r_out, r_in, n=5, rot=-math.pi / 2):
    pts = []
    for i in range(n * 2):
        r = r_out if i % 2 == 0 else r_in
        a = rot + math.pi * i / n
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


# --- Coin -----------------------------------------------------------------------------------
GOLD = ((0.42, 0.24, 0.04), (0.9, 0.62, 0.14), (1.0, 0.9, 0.5))
star = star_points(128, 130, 52, 22)


def coin_height(x, y):
    r = math.hypot(x - 128, y - 128)
    edge = smooth(112, 100, r)  # outer bevel
    rim = smooth(112, 98, r) * 0.6
    dish = smooth(94, 100, r)  # rim up, field down
    h = rim * 0.0 + 0.55 * edge + 0.35 * dish
    # A fine milled edge.
    if 99 < r < 108:
        a = math.atan2(y - 128, x - 128)
        h += 0.03 * math.sin(a * 60)
    d = sd_polygon((x, y), star)
    h += 0.4 * smooth(6, -2, d) * smooth(100, 90, r)
    return h * 22


def coin(x, y):
    r = math.hypot(x - 128, y - 128)
    if r > 112.5:
        return None
    n = normal_from(coin_height, x, y, 0.045)
    col = shade(n, GOLD, spec=0.9, gloss=30)
    # A darker outline.
    k = smooth(112, 108, r)
    return mix([0.2, 0.1, 0.02], col, k)


# --- Gem ------------------------------------------------------------------------------------
BLUE = ((0.03, 0.2, 0.55), (0.22, 0.62, 0.95), (0.85, 0.98, 1.0))


def gem(x, y):
    cx, cy = 128, 130
    dx, dy = x - cx, y - cy
    r = math.hypot(dx, dy)
    a = math.atan2(dy, dx)
    # Outer shape: an octagon (cut corners), table a smaller octagon.
    k = 8
    sector = (a + math.pi / k) % (2 * math.pi / k) - math.pi / k  # angle within the sector
    edge_r = 112 * math.cos(math.pi / k) / math.cos(sector)
    if r > edge_r:
        return None
    table_r = 52 * math.cos(math.pi / k) / math.cos(sector)
    if r < table_r:
        n = (0.0, 0.0, 1.0)
        base = shade(n, BLUE, gloss=60, spec=1.0)
        return mix(base, [1, 1, 1], 0.25 * smooth(table_r, 0, r) * (0.5 + 0.5 * math.sin(a * 2)))
    # Crown: sectors split into a kite facet and two side triangles; each tilts outward by a different amount.
    idx = int(((a + math.pi / k + 2 * math.pi) % (2 * math.pi)) / (2 * math.pi / k))
    t = (r - table_r) / max(1.0, edge_r - table_r)
    frac = ((a + math.pi / k + 2 * math.pi) % (2 * math.pi / k)) / (2 * math.pi / k)
    tilt = 0.78 + 0.25 * ((idx * 3) % 5) / 4
    if frac < 0.5 - 0.25 * t or frac > 0.5 + 0.25 * t:
        tilt *= 0.72  # side facets are shallower
    ang = a + (0.2 if frac < 0.5 else -0.2)
    n = (math.cos(ang) * tilt, math.sin(ang) * tilt, 1 - tilt * 0.45)
    l = math.sqrt(sum(c * c for c in n))
    n = tuple(c / l for c in n)
    col = shade(n, BLUE, gloss=18, spec=0.8)
    # Facet edges catch the light.
    if abs(frac - 0.5) < 0.03 or frac < 0.02 or frac > 0.98:
        col = mix(col, [1, 1, 1], 0.35)
    outline = smooth(edge_r, edge_r - 4, r)
    return mix([0.02, 0.1, 0.3], col, outline)


# --- Ring -----------------------------------------------------------------------------------
PINK = ((0.45, 0.04, 0.3), (0.95, 0.35, 0.65), (1.0, 0.85, 0.95))


def ring_height(x, y):
    r = math.hypot(x - 128, y - 128)
    mid, half = 86, 26
    u = (r - mid) / half
    if abs(u) >= 1:
        return 0.0
    return math.sqrt(1 - u * u) * 30


def ring(x, y):
    r = math.hypot(x - 128, y - 128)
    if r < 60 or r > 112:
        return None
    n = normal_from(ring_height, x, y, 0.06)
    col = shade(n, PINK, gloss=45, spec=1.0)
    return mix([0.25, 0.02, 0.15], col, smooth(61, 66, r) * smooth(112, 107, r))


# --- Arrow ----------------------------------------------------------------------------------
GREEN = ((0.04, 0.3, 0.14), (0.25, 0.78, 0.42), (0.88, 1.0, 0.9))
ARROW = [(128, 18), (232, 128), (170, 128), (170, 238), (86, 238), (86, 128), (24, 128)]


def arrow_height(x, y):
    d = sd_polygon((x, y), ARROW)
    return smooth(0, -22, d) * 26


def arrow(x, y):
    d = sd_polygon((x, y), ARROW)
    if d > 0.5:
        return None
    n = normal_from(arrow_height, x, y, 0.07)
    col = shade(n, GREEN, gloss=36, spec=0.8)
    return mix([0.02, 0.15, 0.07], col, smooth(0, -3, d))


render("coin.png", 256, 256, coin)
render("gem.png", 256, 256, gem)
render("ring.png", 256, 256, ring)
render("arrow.png", 256, 256, arrow)


# --- Checkered flag ---------------------------------------------------------------------------
def flag(x, y):
    w, h = 512, 320
    pole = 18
    if x < pole:
        # A steel pole with a rounded highlight.
        t = x / pole
        v = 0.35 + 0.65 * math.sin(t * math.pi)
        return [0.45 * v + 0.1, 0.48 * v + 0.1, 0.55 * v + 0.12]
    fx = (x - pole) / (w - pole)
    cols, rows = 8, 5
    cx, cy = int(fx * cols), int(y / h * rows)
    black = (cx + cy) % 2 == 0
    base = [0.06, 0.06, 0.07] if black else [0.96, 0.96, 0.94]
    # Cloth: soft folds and a hem.
    fold = 0.04 * math.sin(fx * 14 + y * 0.02) + 0.03 * math.sin(fx * 31 - y * 0.05)
    base = [clamp(c * (1 + fold) + (0.0 if black else -0.0)) for c in base]
    hem = smooth(0, 6, min(y, h - y, w - x))
    edge = 0.0 if hem >= 1 else 0.25 * (1 - hem)
    return [clamp(c - edge * c) for c in base]


render("flag.png", 512, 320, flag)


# --- UI pieces: rounded panel, slot and button images (nine-sliced and tinted by the demo) -----
def rounded(w, h, r, fill, inner_glow=0.0, border=2.0):
    def f(x, y):
        cx, cy = x - w / 2, y - h / 2
        qx, qy = abs(cx) - (w / 2 - r), abs(cy) - (h / 2 - r)
        d = math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r
        if d > 0.5:
            return None
        v = fill(x, y, d)
        edge = smooth(-border - 1, -border + 1, d)
        out = [clamp(v[i] + 0.25 * edge * (1 - v[i])) for i in range(3)]
        return out
    return f


render("panel.png", 96, 96, rounded(96, 96, 22, lambda x, y, d: [1.0 - 0.1 * (y / 96), 1.0 - 0.1 * (y / 96), 1.0 - 0.1 * (y / 96)]))
render("slot.png", 96, 96, rounded(96, 96, 18, lambda x, y, d: [0.78 + 0.22 * smooth(0, 40, -d) - 0.12 * (1 - y / 96), 0.78 + 0.22 * smooth(0, 40, -d) - 0.12 * (1 - y / 96), 0.8 + 0.2 * smooth(0, 40, -d)], border=3))
render("button.png", 64, 64, rounded(64, 64, 14, lambda x, y, d: [0.96 - 0.16 * (y / 64), 0.96 - 0.16 * (y / 64), 0.98 - 0.12 * (y / 64)]))
