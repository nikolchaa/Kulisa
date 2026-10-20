# Paints the two backgrounds of templates/classic-vn: a tea shop at dusk and the street outside at
# night (soft gradients, a window with a skyline, lamps with glow, bokeh). Pure Python.
# Usage: python paint_bg.py OUTDIR
import math, os, random, struct, sys, zlib

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)
W, H = 1280, 720


def write_png(path, w, h, rgb):
    rows = b"".join(b"\x00" + bytes(rgb[y * w * 3:(y + 1) * w * 3]) for y in range(h))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def clamp(v, a=0.0, b=1.0):
    return a if v < a else (b if v > b else v)


def smooth(a, b, x):
    t = clamp((x - a) / (b - a))
    return t * t * (3 - 2 * t)


def mix(a, b, t):
    return [a[i] + (b[i] - a[i]) * t for i in range(3)]


def add(a, b, k=1.0):
    return [a[i] + b[i] * k for i in range(3)]


def hash2(x, y):
    n = math.sin(x * 127.1 + y * 311.7) * 43758.5453
    return n - math.floor(n)


def sd_box(x, y, cx, cy, hw, hh, r=0.0):
    qx, qy = abs(x - cx) - (hw - r), abs(y - cy) - (hh - r)
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r


def sd_ellipse(x, y, cx, cy, rx, ry):
    return (math.hypot((x - cx) / rx, (y - cy) / ry) - 1.0) * min(rx, ry)


def cov(d):
    return clamp(0.5 - d)


def glow(x, y, cx, cy, r):
    d = math.hypot(x - cx, y - cy) / r
    return math.exp(-d * d * 2.2)


def paint(path, fn):
    px = bytearray(W * H * 3)
    for y in range(H):
        for x in range(W):
            c = fn(x + 0.5, y + 0.5)
            o = (y * W + x) * 3
            # A little dither against banding in the gradients.
            n = (hash2(x, y) - 0.5) / 255.0
            px[o:o + 3] = bytes(int(clamp(c[i] + n) * 255 + 0.5) for i in range(3))
    write_png(path, W, H, px)
    print("wrote", path)


def vignette(c, x, y, strength=0.45):
    v = math.hypot((x - W / 2) / (W * 0.62), (y - H / 2) / (H * 0.75))
    return [ch * (1 - smooth(0.4, 1.1, v) * strength) for ch in c]


def bokeh(x, y, seed, count, color, area, radius):
    out = [0.0, 0.0, 0.0]
    for i in range(count):
        cx = area[0] + hash2(seed, i) * (area[2] - area[0])
        cy = area[1] + hash2(i, seed) * (area[3] - area[1])
        r = radius * (0.5 + hash2(seed + i, 3))
        d = math.hypot(x - cx, y - cy) / r
        if d < 1.2:
            k = (1 - smooth(0.75, 1.05, d)) * 0.5 + 0.5 * math.exp(-d * d * 3) * 0.3
            out = add(out, color, k * (0.4 + 0.6 * hash2(i, seed + 7)))
    return out


# ---- Tea shop at dusk -------------------------------------------------------------------------------
def skyline(x, y, base_y, seed, lit=0.15):
    """A row of buildings as silhouette coverage and lit-window glints."""
    cov_ = 0.0
    win = 0.0
    cell = 56
    i = int(x // cell)
    bx = i * cell
    h = 70 + 150 * hash2(i, seed)
    top = base_y - h
    if y > top and x - bx < cell - 4:
        cov_ = 1.0
        wx, wy = (x - bx - 10) % 14, (y - top - 10) % 18
        if (x - bx) > 8 and wx < 8 and wy < 11 and hash2(int((x - bx - 10) // 14) + i * 9, int((y - top - 10) // 18)) < lit:
            win = 1.0
    return cov_, win


def tea_room(x, y):
    # Wall: warm cream with faint vertical stripes, darker towards the corners.
    wall = mix([0.93, 0.78, 0.6], [0.78, 0.55, 0.4], smooth(0, 700, y))
    wall = [c * (0.97 + 0.03 * math.sin(x * 0.22)) for c in wall]
    col = wall
    # Wainscot: wooden panels in the lower part.
    wood_top = 478
    if y > wood_top:
        t = (y - wood_top) / (H - wood_top)
        grain = 0.5 + 0.5 * math.sin(x * 0.04 + math.sin(y * 0.3) * 0.6)
        col = mix([0.52, 0.31, 0.2], [0.34, 0.2, 0.13], smooth(0, 1, t))
        col = [c * (0.92 + 0.08 * grain) for c in col]
        # Panel lines.
        col = [c * (1 - 0.35 * (1 - smooth(0.0, 3.0, abs(((x - 40) % 210) - 105) - 100))) for c in col]
        col = [c * (1 - 0.3 * (1 - smooth(0.0, 3.0, abs(y - wood_top - 3)))) for c in col]
    # The window: an arched opening with a dusk sky and a skyline.
    wx0, wx1, wy0, wy1 = 150, 570, 80, 470
    dwin = max(sd_box(x, y, (wx0 + wx1) / 2, (wy0 + wy1) / 2 + 60, (wx1 - wx0) / 2, (wy1 - wy0) / 2 - 60 + 60 * 0 + 0, 0),
               0)
    arch = sd_ellipse(x, y, (wx0 + wx1) / 2, wy0 + 110, (wx1 - wx0) / 2, 110)
    inside_box = sd_box(x, y, (wx0 + wx1) / 2, (wy0 + wy1) / 2 + 55, (wx1 - wx0) / 2, (wy1 - wy0) / 2 - 55)
    d_open = min(arch if y < wy0 + 110 else 1e9, inside_box) if y >= wy0 + 110 else arch
    if y >= wy0 + 110:
        d_open = inside_box
    if d_open < 1.0:
        t = (y - wy0) / (wy1 - wy0)
        sky = mix([0.28, 0.22, 0.55], [0.95, 0.5, 0.45], smooth(0.1, 0.8, t))
        sky = mix(sky, [1.0, 0.8, 0.45], smooth(0.62, 1.0, t))
        # Sun glow near the horizon and some wispy clouds.
        sky = add(sky, [1.0, 0.7, 0.3], 0.35 * glow(x, y, 330, 420, 180))
        cl = 0.5 + 0.5 * math.sin(x * 0.03 + y * 0.09 + math.sin(x * 0.011) * 3)
        sky = mix(sky, [1.0, 0.72, 0.62], 0.25 * cl * smooth(0.25, 0.7, t) * (1 - smooth(0.7, 0.9, t)))
        # Two layers of skyline.
        c1, w1 = skyline(x, y, 560, 3, 0.04)
        c2, w2 = skyline(x + 17, y, 600, 8, 0.2)
        far = [0.45, 0.28, 0.45]
        near = [0.2, 0.12, 0.25]
        sky = mix(sky, far, c1 * 0.85)
        sky = mix(sky, near, c2)
        sky = add(sky, [1.0, 0.82, 0.45], 0.8 * w2 + 0.4 * w1)
        col = mix(col, sky, cov(d_open))
    # Window frame and muntins.
    frame_d = abs(d_open) if d_open > -9 else 99
    on_frame = (d_open > -9 and d_open < 5)
    if on_frame:
        wood = mix([0.55, 0.34, 0.2], [0.38, 0.22, 0.13], smooth(0, 1, (y - wy0) / 400.0))
        col = mix(col, wood, smooth(5, 2, d_open) * smooth(-9, -6, d_open) * 0.0 + cov(d_open - 5) * (1 - cov(d_open + 9)) * 0.0 + (smooth(5.0, 1.0, d_open) if d_open > -9 else 0))
    if d_open < -3:
        # Muntins: a cross.
        mx = abs(x - (wx0 + wx1) / 2)
        my = abs(y - (wy0 + 250))
        m = min(mx, my)
        col = mix(col, [0.42, 0.25, 0.15], 1 - smooth(2.5, 4.5, m))
    # Sill.
    sill = sd_box(x, y, (wx0 + wx1) / 2, wy1 + 12, (wx1 - wx0) / 2 + 26, 12, 4)
    col = mix(col, [0.62, 0.4, 0.26], cov(sill))
    col = [c * (1 - 0.4 * (1 - smooth(0, 14, sill)) * (1 if sill > 0 and y > wy1 else 0)) for c in col]
    # Curtains: soft drapes at the sides of the window.
    for cx0, side in ((wx0 - 30, -1), (wx1 + 30, 1)):
        dc = sd_box(x, y, cx0, (wy0 + wy1) / 2 + 20, 62, 240, 14)
        fold = 0.5 + 0.5 * math.sin((x - cx0) * 0.16 + math.sin(y * 0.01) * 2)
        cur = mix([0.62, 0.22, 0.3], [0.45, 0.14, 0.22], fold)
        cur = [c * (1 - 0.3 * smooth(300, 480, y)) for c in cur]
        col = mix(col, cur, cov(dc))
    # Shelves on the right with jars and cups.
    for sy_, items in ((250, 3), (370, 4)):
        shelf = sd_box(x, y, 960, sy_, 250, 8, 3)
        col = mix(col, [0.45, 0.27, 0.17], cov(shelf))
        col = [c * (1 - 0.3 * (1 - smooth(0, 16, shelf)) * (1 if y > sy_ + 8 else 0)) for c in col]
        for k in range(items):
            jx = 760 + k * (480 / max(1, items - 1)) * 0.78 + 20 * hash2(k, sy_)
            jh = 40 + 36 * hash2(k, sy_ + 1)
            jw = 22 + 12 * hash2(k, sy_ + 2)
            jar = sd_box(x, y, jx, sy_ - 8 - jh / 2, jw, jh / 2, 8)
            hue = hash2(k + sy_, 5)
            jc = mix([0.8, 0.4, 0.3], [0.35, 0.6, 0.55], hue)
            shine = smooth(jw, -jw, x - jx - jw * 0.3) * 0.0
            jc = mix(jc, [1, 1, 1], 0.3 * (1 - smooth(0, 6, abs(x - (jx - jw * 0.45)))))
            col = mix(col, [c * (0.75 + 0.35 * (1 - (y - (sy_ - 8 - jh)) / jh)) for c in jc], cov(jar))
            lid = sd_box(x, y, jx, sy_ - 8 - jh - 4, jw * 0.8, 4, 3)
            col = mix(col, [0.3, 0.2, 0.15], cov(lid))
    # A hanging plant at the right edge.
    # Pendant lamps.
    glow_total = 0.0
    for lx, ly, lr in ((700, 150, 150), (880, 100, 140), (1070, 160, 150)):
        cord = abs(x - lx) < 1.5 and y < ly - 28
        if cord:
            col = mix(col, [0.15, 0.1, 0.08], 0.9)
        dome = sd_ellipse(x, y, lx, ly - 6, 44, 30)
        if y < ly - 6:
            col = mix(col, mix([0.95, 0.72, 0.4], [0.55, 0.32, 0.18], smooth(ly - 36, ly, y)), cov(dome))
        bulb = sd_ellipse(x, y, lx, ly + 2, 16, 10)
        col = mix(col, [1.0, 0.95, 0.75], cov(bulb))
        glow_total += glow(x, y, lx, ly + 20, lr) * 0.55
        # A cone of light downward.
        cone = clamp(1 - abs(x - lx) / (30 + (y - ly) * 0.55)) if y > ly else 0
        glow_total += cone * 0.12 * (1 - smooth(ly, ly + 360, y))
    col = add(col, [1.0, 0.72, 0.38], glow_total)
    # A counter in front: dark wood top and a front panel.
    counter_top = 560
    if y > counter_top:
        t = (y - counter_top) / (H - counter_top)
        grain = 0.5 + 0.5 * math.sin(x * 0.02 + math.sin(y * 0.8) * 0.4)
        ctr = mix([0.5, 0.3, 0.18], [0.24, 0.13, 0.09], smooth(0, 0.8, t))
        ctr = [c * (0.9 + 0.1 * grain) for c in ctr]
        edge = 1 - smooth(0, 5, y - counter_top)
        ctr = add(ctr, [1.0, 0.85, 0.6], 0.18 * edge)
        col = ctr
    # Cups and a teapot on the counter.
    for tx, kind in ((830, "pot"), (990, "cup"), (1100, "cup"), (260, "cup")):
        if kind == "pot":
            body = sd_ellipse(x, y, tx, counter_top - 36, 58, 42)
            spout = sd_box(x, y, tx + 66, counter_top - 52, 22, 6, 3)
            handle = abs(sd_ellipse(x, y, tx - 62, counter_top - 40, 22, 26)) - 3.5
            lid = sd_ellipse(x, y, tx, counter_top - 80, 18, 10)
            d = min(body, spout, handle, lid)
            c = mix([0.9, 0.45, 0.38], [0.55, 0.2, 0.2], smooth(counter_top - 80, counter_top, y))
            c = mix(c, [1, 1, 1], 0.35 * (1 - smooth(0, 10, abs(x - (tx - 22)))) * (1 - smooth(counter_top - 70, counter_top - 20, y)))
            col = mix(col, c, cov(d))
        else:
            body = sd_ellipse(x, y, tx, counter_top - 18, 34, 22)
            if y < counter_top - 18:
                body = max(body, 1e9) if False else body
            handle = abs(sd_ellipse(x, y, tx + 36, counter_top - 22, 12, 14)) - 3
            d = min(body, handle)
            c = mix([0.97, 0.95, 0.92], [0.78, 0.72, 0.7], smooth(counter_top - 36, counter_top, y))
            col = mix(col, c, cov(d))
            steam = glow(x, y, tx + 3 * math.sin(y * 0.05), counter_top - 70, 18) * 0.0
    # Warm bokeh in the air and cool ones through the window.
    col = add(col, bokeh(x, y, 5, 14, [0.5, 0.32, 0.12], (560, 120, 1240, 520), 36), 0.7)
    return vignette(col, x, y, 0.4)


# ---- The street at night -------------------------------------------------------------------------------
def night(x, y):
    t = y / H
    sky = mix([0.02, 0.03, 0.1], [0.12, 0.1, 0.3], smooth(0.0, 0.75, t))
    sky = mix(sky, [0.35, 0.22, 0.4], smooth(0.5, 0.85, t) * 0.5)
    # Stars.
    if y < 360:
        cx_, cy_ = int(x // 7), int(y // 7)
        s = hash2(cx_, cy_)
        if s > 0.93:
            sx_ = (cx_ + 0.2 + 0.6 * hash2(cx_, 3)) * 7
            sy2 = (cy_ + 0.2 + 0.6 * hash2(3, cy_)) * 7
            r_ = math.hypot(x - sx_, y - sy2)
            sky = add(sky, [0.9, 0.9, 1.0], math.exp(-r_ * r_ / (1.1 + 2.5 * hash2(cx_, cy_))) * (0.4 + 0.9 * (s - 0.93) * 14) * (1 - y / 400))
    # Moon.
    mx, my = 1010, 150
    md = math.hypot(x - mx, y - my)
    sky = add(sky, [0.55, 0.6, 0.85], 0.3 * glow(x, y, mx, my, 160))
    sky = mix(sky, [0.97, 0.96, 0.88], cov(md - 46))
    crater = glow(x, y, mx - 14, my + 6, 12) * 0.12 + glow(x, y, mx + 16, my - 14, 8) * 0.1
    sky = [c * (1 - crater * cov(md - 46) * 0 ) for c in sky]
    col = sky
    # Three layers of skyline.
    for base, seed, color, lit in ((470, 2, [0.1, 0.1, 0.24], 0.06), (520, 7, [0.07, 0.07, 0.17], 0.12), (590, 12, [0.04, 0.04, 0.1], 0.18)):
        c_, w_ = skyline(x + seed * 11, y, base, seed, lit)
        col = mix(col, color, c_)
        col = add(col, [1.0, 0.8, 0.45], 0.7 * w_)
    # Road with a wet shine and a pavement.
    road_top = 585
    if y > road_top:
        t2 = (y - road_top) / (H - road_top)
        road = mix([0.1, 0.1, 0.17], [0.04, 0.04, 0.08], t2)
        col = road
        # Reflections of lamp light on the wet asphalt.
        for lx, lcol in ((260, [1.0, 0.8, 0.45]), (900, [1.0, 0.8, 0.45]), (610, [0.5, 0.7, 1.0])):
            rr = math.exp(-((x - lx) / (30 + 80 * t2)) ** 2) * (1 - t2 * 0.8) * 0.5
            col = add(col, lcol, rr * (0.6 + 0.4 * math.sin(y * 0.6)))
        # A sidewalk edge.
        curb = 1 - smooth(0, 4, abs(y - road_top - 20))
        col = add(col, [0.3, 0.3, 0.45], 0.25 * curb)
    # The shop on the left: a lit window and a sign.
    shop = sd_box(x, y, 1160, 470, 150, 130, 2) - 0.0
    if x > 1010 and y > 340:
        col = mix(col, [0.1, 0.07, 0.12], cov(shop) * (0.92 - 0.12 * smooth(340, 590, y)))
        col = add(col, [0.25, 0.14, 0.1], 0.5 * cov(shop) * smooth(420, 560, y) * glow(x, y, 1160, 480, 220))
        win_d = sd_box(x, y, 1160, 480, 100, 70, 6)
        warm = mix([1.0, 0.75, 0.4], [0.9, 0.5, 0.3], smooth(410, 550, y))
        col = mix(col, warm, cov(win_d))
        col = mix(col, [0.28, 0.16, 0.12], (1 - smooth(2.5, 4.5, abs(x - 1160))) * cov(win_d + 0.5))
        col = mix(col, [0.28, 0.16, 0.12], (1 - smooth(2.5, 4.5, abs(y - 480))) * cov(win_d + 0.5))
        # The little hanging sign.
        sign = sd_box(x, y, 1048, 392, 30, 16, 6)
        col = mix(col, [0.95, 0.85, 0.65], cov(sign))
        col = add(col, [1.0, 0.7, 0.35], 0.2 * glow(x, y, 1160, 480, 260))
    # Lamp posts.
    for lx, h_ in ((300, 260), (620, 240), (900, 230)):
        post = sd_box(x, y, lx, road_top - h_ / 2 + 8, 4, h_ / 2, 2)
        col = mix(col, [0.06, 0.06, 0.1], cov(post))
        head = sd_ellipse(x, y, lx, road_top - h_ - 6, 18, 12)
        col = mix(col, [1.0, 0.92, 0.65], cov(head))
        col = add(col, [1.0, 0.75, 0.4], 0.8 * glow(x, y, lx, road_top - h_, 110))
        col = add(col, [1.0, 0.75, 0.4], 0.15 * glow(x, y, lx, road_top - h_ + 80, 220))
    # Mist near the ground and soft bokeh.
    mist = smooth(500, 640, y) * (1 - smooth(640, 720, y)) * 0.12
    col = add(col, [0.3, 0.35, 0.55], mist)
    col = add(col, bokeh(x, y, 9, 18, [0.35, 0.28, 0.12], (0, 380, 1280, 620), 26), 0.5)
    return vignette(col, x, y, 0.5)


paint(os.path.join(OUT, "bg room.png"), tea_room)
paint(os.path.join(OUT, "bg night.png"), night)
