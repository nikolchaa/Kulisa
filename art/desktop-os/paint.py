# Paints the art of templates/desktop-os: window chrome and panels (nine-slices), white glyph
# icons for tinting, app icons, contact avatars, the photos the fake web pages show, and UI sounds.
# Pure Python (art/common/canvas.py). Usage: python art/desktop-os/paint.py templates/desktop-os
import math, os, random, struct, sys, wave
from multiprocessing import Pool

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "common"))
from canvas import (Canvas, clamp, smooth, mix, hexc, grad, fbm, vnoise, hash2, sd_circle, sd_box, sd_box4,  # noqa: E402
                    sd_segment, sd_ellipse, sd_superellipse, sd_arc, sd_triangle, cov)

OUT = sys.argv[1] if len(sys.argv) > 1 else "templates/desktop-os"
WHITE = [1, 1, 1, 1]


def path(*p):
    full = os.path.join(OUT, *p)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    return full


# ---- chrome ---------------------------------------------------------------------------------

def window():
    # The window body: dark glass with a hairline highlight along the top edge and a darker rim.
    c = Canvas(64, 64)
    r = 14
    body = lambda x, y: sd_box(x, y, 32, 32, 32, 32, r)
    def col(x, y):
        t = y / 64
        return mix(hexc("#1d2130"), hexc("#171a26"), t)
    c.shape(body, col)
    # Rim (inside, 1 px) and the top highlight.
    c.shape(lambda x, y: abs(body(x, y) + 0.5) - 0.5, [0, 0, 0, 0.55])
    c.shape(lambda x, y: max(abs(body(x, y) + 1.5) - 0.5, y - 20), lambda x, y: [1, 1, 1, 0.16 * (1 - y / 20)])
    c.save(path("gui", "window.png"))


def shadow():
    # A soft drop shadow for a rounded rectangle; drawn behind windows and menus.
    s, m = 128, 40
    c = Canvas(s, s)
    c.shape(lambda x, y: sd_box(x, y, s / 2, s / 2 + 4, s / 2 - m, s / 2 - m, 14), [0, 0, 0, 1])
    c.blur_alpha(9)
    for p in c.px:
        p[3] = min(1.0, p[3] * 1.15)
    c.save(path("gui", "shadow.png"))


def plain(name, size, sdf):
    c = Canvas(size, size)
    c.shape(sdf, WHITE)
    c.save(path("gui", name))


def chrome():
    window()
    shadow()
    plain("round.png", 32, lambda x, y: sd_box(x, y, 16, 16, 16, 16, 8))
    plain("round_big.png", 64, lambda x, y: sd_box(x, y, 32, 32, 32, 32, 18))
    plain("pill.png", 32, lambda x, y: sd_box(x, y, 16, 16, 16, 16, 16))
    plain("circle.png", 64, lambda x, y: sd_circle(x, y, 32, 32, 32))
    plain("bubble_in.png", 48, lambda x, y: sd_box4(x, y, 24, 24, 24, 24, (18, 18, 18, 5)))
    plain("bubble_out.png", 48, lambda x, y: sd_box4(x, y, 24, 24, 24, 24, (18, 18, 5, 18)))
    # Taskbar: translucent, with a light hairline on top.
    c = Canvas(16, 16)
    c.paint(lambda x, y: hexc("#0c0e16", 0.78) if y > 1 else [1, 1, 1, 0.10])
    c.save(path("gui", "taskbar.png"))


# ---- glyphs (48 px, white, drawn with strokes) ----------------------------------------------------

W = 3.4  # stroke width


def stroke(d):
    return d - W / 2


GLYPHS = {
    "close": lambda x, y: stroke(min(sd_segment(x, y, 15, 15, 33, 33), sd_segment(x, y, 33, 15, 15, 33))),
    "minimize": lambda x, y: stroke(sd_segment(x, y, 14, 30, 34, 30)),
    "maximize": lambda x, y: stroke(abs(sd_box(x, y, 24, 24, 10, 10, 2.5))),
    "restore": lambda x, y: stroke(min(abs(sd_box(x, y, 21, 27, 8, 8, 2)),
                                       max(abs(sd_box(x, y, 27, 21, 8, 8, 2)), -sd_box(x, y, 21, 27, 10, 10, 2)))),
    "back": lambda x, y: stroke(min(sd_segment(x, y, 28, 13, 18, 24), sd_segment(x, y, 18, 24, 28, 35))),
    "forward": lambda x, y: stroke(min(sd_segment(x, y, 20, 13, 30, 24), sd_segment(x, y, 30, 24, 20, 35))),
    "reload": lambda x, y: min(stroke(sd_arc(x, y, 24, 24, 11, -1.1, 4.2)),
                               sd_triangle(x, y, (29.5, 7.5), (36.5, 16.5), (25.5, 17.5))),
    "search": lambda x, y: stroke(min(abs(sd_circle(x, y, 21, 21, 9)), sd_segment(x, y, 28, 28, 36, 36))),
    "send": lambda x, y: max(sd_triangle(x, y, (10, 11), (39, 24), (10, 37)), -sd_triangle(x, y, (8, 24), (22, 24), (8, 31))),
    "bell": lambda x, y: min(
        min(sd_circle(x, y, 24, 22, 10), sd_box(x, y, 24, 28, 10, 6, 0), sd_box(x, y, 24, 33.5, 14, 1.8, 1.8)),
        sd_circle(x, y, 24, 38.5, 3.2), sd_circle(x, y, 24, 11, 2.2)),
    "wifi": lambda x, y: min(stroke(sd_arc(x, y, 24, 34, 18, -2.4, -0.74)), stroke(sd_arc(x, y, 24, 34, 11.5, -2.4, -0.74)),
                             stroke(sd_arc(x, y, 24, 34, 5, -2.4, -0.74)) + 0.4, sd_circle(x, y, 24, 34.5, 2.4)),
    "volume": lambda x, y: min(
        min(sd_box(x, y, 12, 24, 4, 5, 1), sd_triangle(x, y, (11, 24), (24, 11), (24, 37))),
        stroke(sd_arc(x, y, 25, 24, 7, -0.9, 0.9)), stroke(sd_arc(x, y, 25, 24, 13, -0.9, 0.9))),
    "battery": lambda x, y: min(stroke(abs(sd_box(x, y, 22, 24, 13, 7.5, 2.5))), sd_box(x, y, 18, 24, 7, 4, 1),
                                sd_box(x, y, 38, 24, 1.8, 3.5, 1)),
    "power": lambda x, y: min(stroke(sd_arc(x, y, 24, 26, 11, -1.0, 4.14)), stroke(sd_segment(x, y, 24, 10, 24, 24))),
    "save": lambda x, y: min(stroke(abs(sd_box(x, y, 24, 24, 12, 12, 2.5))), sd_box(x, y, 24, 17, 6, 3.5, 1),
                             stroke(abs(sd_box(x, y, 24, 30.5, 6, 4.5, 1)))),
    "image": lambda x, y: min(stroke(abs(sd_box(x, y, 24, 24, 13, 10, 2.5))), sd_circle(x, y, 18.5, 19.5, 2.5),
                              max(sd_triangle(x, y, (12, 33), (21, 22), (30, 33)), sd_box(x, y, 24, 24, 12, 9, 1.5)),
                              max(sd_triangle(x, y, (22, 33), (29, 26), (37, 33)), sd_box(x, y, 24, 24, 12, 9, 1.5))),
    "home": lambda x, y: min(stroke(min(sd_segment(x, y, 10, 24, 24, 11), sd_segment(x, y, 24, 11, 38, 24))),
                             stroke(max(abs(sd_box(x, y, 24, 29, 9.5, 8, 1.5)), 22 - y))),
    "lock": lambda x, y: min(sd_box(x, y, 24, 29, 10, 8, 2.5), stroke(max(abs(sd_circle(x, y, 24, 20, 6.5)), y - 22)),
                             stroke(sd_segment(x, y, 17.5, 20, 17.5, 23)), stroke(sd_segment(x, y, 30.5, 20, 30.5, 23))),
}


def glyph(name):
    c = Canvas(48, 48)
    c.shape(GLYPHS[name], WHITE)
    c.save(path("icons", name + ".png"))


# ---- app icons (128 px squircles) -----------------------------------------------------------------

APPS = {
    "chat": ("#43e0a8", "#139a8a"),
    "browser": ("#6ab7ff", "#3655e0"),
    "notes": ("#ffd66b", "#f29a2e"),
    "settings": ("#a8b0c4", "#545d74"),
    "start": ("#9a8cff", "#4b3fd6"),
}


def app_glyph(name, x, y):
    if name == "chat":
        a = min(sd_box(x, y, 58, 56, 30, 22, 14), sd_triangle(x, y, (40, 70), (34, 90), (58, 72)))
        b = min(sd_box(x, y, 78, 70, 22, 17, 12), sd_triangle(x, y, (88, 82), (96, 98), (78, 85)))
        return min(a, max(b, -(a - 5)))
    if name == "browser":
        ring = abs(sd_circle(x, y, 64, 64, 30)) - 3.2
        mer = abs(sd_ellipse(x, y, 64, 64, 13, 30)) - 2.6
        eq = max(abs(y - 64) - 2.6, sd_circle(x, y, 64, 64, 30))
        lat = max(min(abs(y - 49) - 2.2, abs(y - 79) - 2.2), sd_circle(x, y, 64, 64, 28))
        return min(ring, max(mer, sd_circle(x, y, 64, 64, 31)), eq, lat)
    if name == "notes":
        sheet = sd_box(x, y, 64, 66, 30, 36, 6)
        fold = sd_triangle(x, y, (76, 30), (94, 30), (94, 48))
        lines = min(abs(y - 52) - 2.5, abs(y - 66) - 2.5, max(abs(y - 80) - 2.5, x - 74))
        body = max(sheet, -fold)
        return body, max(max(lines, abs(x - 63) - 18), sheet)
    if name == "settings":
        a = math.atan2(y - 64, x - 64)
        r = math.hypot(x - 64, y - 64)
        teeth = 34 + 6 * clamp(math.cos(a * 8) * 2.2 + 0.4, -1, 1)
        return max(r - teeth, -(r - 12)), None
    if name == "start":
        return min(sd_box(x, y, 50, 50, 12, 12, 4), sd_box(x, y, 78, 50, 12, 12, 4), sd_box(x, y, 50, 78, 12, 12, 4),
                   sd_box(x, y, 78, 78, 12, 12, 4)), None
    return 1e9


def app_icon(name):
    top, bottom = hexc(APPS[name][0]), hexc(APPS[name][1])
    c = Canvas(128, 128)
    # Drop shadow.
    sh = Canvas(128, 128)
    sh.shape(lambda x, y: sd_superellipse(x, y, 64, 68, 54), [0, 0, 0, 0.55])
    sh.blur_alpha(4)
    c.over(sh)
    body = lambda x, y: sd_superellipse(x, y, 64, 64, 54)
    c.shape(body, lambda x, y: mix(top, bottom, smooth(10, 118, y + (x - 64) * 0.25)))
    # A gloss band near the top and a rim light.
    c.shape(lambda x, y: max(body(x, y) + 2, y - 52), lambda x, y: [1, 1, 1, 0.16 * (1 - y / 52) ** 1.5])
    c.shape(lambda x, y: abs(body(x, y) + 1.2) - 0.7, lambda x, y: [1, 1, 1, 0.22 * (1 - y / 128)])
    g = app_glyph(name, 0, 0)
    second = None
    if isinstance(g, tuple):
        f = lambda x, y: app_glyph(name, x, y)[0]
        if app_glyph(name, 64, 64)[1] is not None:
            second = lambda x, y: app_glyph(name, x, y)[1]
    else:
        f = lambda x, y: app_glyph(name, x, y)
    # Glyph shadow, glyph, details.
    gs = Canvas(128, 128)
    gs.shape(lambda x, y: f(x, y - 3), [0, 0, 0, 0.28])
    gs.blur_alpha(2)
    c.over(gs)
    c.shape(f, lambda x, y: mix([1, 1, 1, 1], hexc("#eef2ff"), y / 128))
    if second:
        c.shape(second, mix(bottom, [0.2, 0.2, 0.25, 1], 0.35))
    if name == "settings":
        c.shape(lambda x, y: sd_circle(x, y, 64, 64, 12), mix(bottom, [0, 0, 0, 1], 0.2))
    c.save(path("apps", name + ".png"))


# ---- avatars --------------------------------------------------------------------------------------

AVATARS = {
    # bg top, bg bottom, skin, hair, shirt, hair style
    "juno": ("#ff9a8b", "#ff6a88", "#f2c4a8", "#2b1d2f", "#3c4e8f", "bob"),
    "gull": ("#8fd3f4", "#4f7fd8", "#d9a07c", "#6b4a2e", "#2f3b3f", "cap"),
    "me": ("#b8a6ff", "#6f5cd6", "#e8b896", "#3a2a22", "#1f2433", "short"),
    "unknown": ("#3a3f4f", "#16181f", "#2a2d38", "#20232c", "#16181f", "none"),
}


def avatar(name):
    bg0, bg1, skin, hair, shirt, style = AVATARS[name]
    bg0, bg1, skin, hair, shirt = map(hexc, (bg0, bg1, skin, hair, shirt))
    s = 128
    c = Canvas(s, s)
    disk = lambda x, y: sd_circle(x, y, 64, 64, 64)
    c.shape(disk, lambda x, y: mix(bg0, bg1, y / s))
    inside = lambda d: lambda x, y: max(d(x, y), disk(x, y))
    shade = lambda col, k: [col[0] * k, col[1] * k, col[2] * k, 1]
    if style == "bob":
        c.shape(inside(lambda x, y: min(sd_circle(x, y, 64, 54, 33), sd_box(x, y, 64, 70, 33, 18, 10))), shade(hair, 1))
    # Shoulders, neck, head.
    c.shape(inside(lambda x, y: sd_ellipse(x, y, 64, 132, 50, 36)), lambda x, y: mix(shirt, shade(shirt, 0.7), (y - 96) / 32))
    c.shape(inside(lambda x, y: sd_box(x, y, 64, 88, 9, 14, 4)), shade(skin, 0.82))
    c.shape(inside(lambda x, y: sd_ellipse(x, y, 64, 60, 22, 26)), lambda x, y: mix(skin, shade(skin, 0.86), clamp((x - 50) / 40)))
    if style == "bob":
        c.shape(inside(lambda x, y: max(sd_ellipse(x, y, 64, 44, 26, 16), -sd_ellipse(x, y, 74, 56, 16, 9))), hair)
    elif style == "cap":
        c.shape(inside(lambda x, y: max(sd_ellipse(x, y, 64, 46, 25, 18), y - 46)), hexc("#e8b23a"))
        c.shape(inside(lambda x, y: sd_box(x, y, 76, 46, 20, 3.5, 3)), hexc("#c8901f"))
        c.shape(inside(lambda x, y: max(sd_ellipse(x, y, 64, 74, 21, 14), 72 - y)), shade(hair, 1))
    elif style == "short":
        c.shape(inside(lambda x, y: max(sd_ellipse(x, y, 64, 46, 24, 17), -sd_ellipse(x, y, 58, 58, 20, 10))), hair)
    if style != "none":
        for ex in (55, 73):
            c.shape(inside(lambda x, y, ex=ex: sd_ellipse(x, y, ex, 62, 2.6, 3.4)), [0.12, 0.1, 0.12, 1])
        c.shape(inside(lambda x, y: max(abs(sd_circle(x, y, 64, 64, 10)) - 1.2, 70 - y)), shade(skin, 0.6))
    else:
        # The unknown contact: a silhouette in static.
        for p_y in range(s):
            for p_x in range(s):
                d = disk(p_x + 0.5, p_y + 0.5)
                if d < 0:
                    n = hash2(p_x, p_y, 7)
                    c.blend(p_x, p_y, [n, n, n, 1], 0.12)
    c.save(path("avatars", name + ".png"))


# ---- web photos -----------------------------------------------------------------------------------

def lighthouse_scene(lit, w=640, h=360):
    rng_seed = 3
    c = Canvas(w, h)
    horizon = h * 0.62
    tx, tbase, ttop = w * 0.68, horizon - 8, h * 0.22
    lantern = (tx, ttop - 10)
    def px(x, y):
        u, v = x / w, y / h
        if y < horizon:
            # Night sky with clouds lit faintly by a hidden moon.
            sky = grad([(0, hexc("#05070f")), (0.55, hexc("#0d1426")), (1, hexc("#1b2a3f"))], y / horizon)
            n = fbm(u * 5 + 3, v * 9, 5, rng_seed)
            cl = smooth(0.45, 0.8, n)
            moon = math.exp(-((x - w * 0.22) ** 2 + (y - h * 0.18) ** 2) / (2 * (w * 0.12) ** 2))
            sky = mix(sky, hexc("#3c4a63"), cl * (0.35 + 0.6 * moon))
            sky = [sky[i] + moon * 0.10 * (1 - cl) for i in range(3)] + [1]
            st = hash2(int(x), int(y), 11)
            if st > 0.9965 and cl < 0.3:
                sky = mix(sky, [0.9, 0.92, 1, 1], (st - 0.9965) / 0.0035 * (1 - cl))
            col = sky
        else:
            # Sea: dark swell with a moon path.
            d = (y - horizon) / (h - horizon)
            sea = grad([(0, hexc("#0e1a2b")), (1, hexc("#04070d"))], d)
            wave = fbm(x / (18 + 60 * d), y / (3 + 10 * d), 3, 9)
            path_ = math.exp(-((x - w * 0.22) / (w * (0.05 + 0.12 * d))) ** 2) * smooth(0.5, 0.75, wave)
            col = [sea[i] + 0.25 * path_ * (0.7, 0.75, 0.85)[i] + 0.04 * (wave - 0.5) for i in range(3)] + [1]
        # The beam (lit): a wide cone sweeping left, with haze.
        if lit:
            dx, dy = x - lantern[0], y - lantern[1]
            ang = math.atan2(dy, -dx)
            cone = math.exp(-((ang - 0.06) / 0.07) ** 2) * (1 if dx < 0 else 0) * math.exp(-abs(dx) / (w * 0.9))
            glow = math.exp(-(dx * dx + dy * dy) / 900) + 0.35 * math.exp(-(dx * dx + dy * dy) / 9000)
            add = cone * 0.45 + glow * 0.9
            col = [col[0] + add * 1.0, col[1] + add * 0.93, col[2] + add * 0.7, 1]
        return col
    c.paint(px)
    # Headland: a rocky silhouette rising to the right.
    def land(x):
        return horizon - 6 - 48 * smooth(w * 0.45, w * 0.95, x) - 22 * fbm(x / 60, 0.3, 4, 21)
    c.shape(lambda x, y: land(x) - y, lambda x, y: mix(hexc("#0a0d14"), hexc("#06080c"), (y - land(x)) / 60), (w * 0.38, horizon - 90, w, horizon + 4))
    # Tower: tapered, striped, with a gallery and a lantern room.
    def tower(x, y):
        t = clamp((y - ttop) / (tbase - ttop))
        half = 10 + 9 * t
        return max(abs(x - tx) - half, ttop - y, y - tbase - 2)
    stripe = lambda y: 1 if int((y - ttop) / 22) % 2 == 0 else 0
    c.shape(tower, lambda x, y: mix(hexc("#2a2f3a") if stripe(y) else hexc("#4c3030"), hexc("#0c0e14"), clamp((x - tx + 8) / 22)))
    c.shape(lambda x, y: sd_box(x, y, tx, ttop - 1, 15, 2.5, 1), hexc("#1a1d24"))
    lamp = hexc("#fff4d0") if lit else hexc("#151922")
    c.shape(lambda x, y: sd_box(x, y, tx, ttop - 10, 8, 7, 1), lamp)
    c.shape(lambda x, y: max(sd_triangle(x, y, (tx - 10, ttop - 17), (tx + 10, ttop - 17), (tx, ttop - 27)), 0), hexc("#11141b"))
    # Keeper's cottage with one window (lit only with the lamp).
    c.shape(lambda x, y: sd_box(x, y, tx - 52, tbase - 10, 22, 12, 1), hexc("#0d1016"))
    c.shape(lambda x, y: sd_triangle(x, y, (tx - 78, tbase - 22), (tx - 26, tbase - 22), (tx - 52, tbase - 40)), hexc("#0b0d12"))
    c.shape(lambda x, y: sd_box(x, y, tx - 58, tbase - 10, 3.5, 4, 0.5), hexc("#ffcf7a") if lit else hexc("#1a1e27"))
    grain(c, 0.03, 41 if lit else 42)
    return c


def coast(w=640, h=360):
    c = Canvas(w, h)
    horizon = h * 0.58
    sun = (w * 0.36, horizon - 18)
    def px(x, y):
        u, v = x / w, y / h
        if y < horizon:
            col = grad([(0, hexc("#2b2a5c")), (0.45, hexc("#a84f78")), (0.8, hexc("#f2905c")), (1, hexc("#ffd08a"))], y / horizon)
            n = fbm(u * 4, v * 14 + 2, 5, 5)
            band = smooth(0.5, 0.75, n) * smooth(0.0, 0.7, y / horizon)
            col = mix(col, hexc("#6a3a62"), band * 0.6)
            g = math.exp(-((x - sun[0]) ** 2 + (y - sun[1]) ** 2) / 2600)
            col = [col[i] + g * (1.0, 0.75, 0.45)[i] * 0.9 for i in range(3)] + [1]
            if math.hypot(x - sun[0], y - sun[1]) < 15:
                col = mix(col, hexc("#fff3d6"), 0.9)
        else:
            d = (y - horizon) / (h - horizon)
            col = grad([(0, hexc("#c9737a")), (0.4, hexc("#5c3f6c")), (1, hexc("#1d1b33"))], d)
            wv = fbm(x / (10 + 50 * d), y / (2 + 8 * d), 3, 2)
            glint = math.exp(-((x - sun[0]) / (20 + 140 * d)) ** 2) * smooth(0.55, 0.8, wv)
            col = [col[i] + glint * (1, 0.8, 0.55)[i] * 0.8 + (wv - 0.5) * 0.05 for i in range(3)] + [1]
        return col
    c.paint(px)
    # Rocks in the foreground and a far headland.
    c.shape(lambda x, y: horizon - 2 - 26 * smooth(w * 0.7, w, x) - 10 * fbm(x / 40, 1, 3, 4) - y,
            hexc("#2a1f33"), (w * 0.6, horizon - 50, w, horizon + 2))
    for i, (rx, ry, rs) in enumerate([(w * 0.1, h * 0.95, 90), (w * 0.28, h * 1.02, 70), (w * 0.86, h * 0.98, 110)]):
        c.shape(lambda x, y, rx=rx, ry=ry, rs=rs, i=i: sd_ellipse(x, y, rx, ry, rs * 1.3, rs * 0.7) + 14 * (fbm(x / 25, y / 25, 3, i) - 0.5),
                lambda x, y, rs=rs: mix(hexc("#2a1d2c"), hexc("#120d18"), clamp((y - h * 0.8) / 80)), (0, h * 0.6, w, h))
    grain(c, 0.02, 7)
    return c


def pier(w=640, h=360):
    c = Canvas(w, h)
    vp = (w * 0.5, h * 0.52)
    def px(x, y):
        col = grad([(0, hexc("#5a6470")), (0.5, hexc("#8a949c")), (1, hexc("#3b434c"))], y / h)
        fog = fbm(x / 140, y / 60, 4, 3)
        return mix(col, hexc("#b8c0c6"), 0.35 * fog)
    c.paint(px)
    # The pier converging to the vanishing point, posts and lamps fading into fog.
    c.shape(lambda x, y: max(y - h, vp[1] + 6 - y, abs(x - vp[0]) - (y - vp[1]) * 1.6 - 3),
            lambda x, y: mix(hexc("#2b2f35"), hexc("#7d868e"), 1 - clamp((y - vp[1]) / (h * 0.45))))
    for k in range(1, 9):
        t = 1 / (1 + k * 0.45)
        y0 = vp[1] + (h - vp[1]) * t
        hh = 140 * t
        fogk = 1 - t
        for side in (-1, 1):
            x0 = vp[0] + side * (y0 - vp[1]) * 1.6
            col = mix(hexc("#1c1f24"), hexc("#a2aab1"), fogk ** 0.8)
            c.shape(lambda x, y, x0=x0, y0=y0, hh=hh, t=t: sd_box(x, y, x0, y0 - hh / 2, 2.5 * t + 0.6, hh / 2, 0), col)
            if k % 2 == 1:
                lx, ly = x0, y0 - hh
                for yy in range(max(0, int(ly - 50 * t)), min(h, int(ly + 50 * t))):
                    for xx in range(max(0, int(lx - 50 * t)), min(w, int(lx + 50 * t))):
                        g = math.exp(-((xx - lx) ** 2 + (yy - ly) ** 2) / (300 * t * t + 4))
                        c.add(xx, yy, [1.0, 0.85, 0.55], g * 0.8 * (1 - fogk * 0.6))
    # Police tape across the near end.
    c.shape(lambda x, y: abs(y - (h * 0.83 + (x - w * 0.5) * 0.05)) - 5 if abs(x - w * 0.5) < w * 0.32 else 1e9,
            lambda x, y: hexc("#f2c21b") if int(x / 26) % 2 else hexc("#1d1d1d"))
    grain(c, 0.025, 13)
    return c


def grain(c, k, seed):
    for y in range(c.h):
        for x in range(c.w):
            n = (hash2(x, y, seed) - 0.5) * k
            p = c.px[y * c.w + x]
            p[0] += n
            p[1] += n
            p[2] += n


PHOTOS = {"cam_off": lambda: lighthouse_scene(False), "cam_on": lambda: lighthouse_scene(True),
          "coast": coast, "pier": pier}


def photo_job(name):
    PHOTOS[name]().save(path("web", "img", name + ".png"))
    return name


# ---- sounds -----------------------------------------------------------------------------------------

SR = 22050


def write_wav(name, samples, peak=0.8):
    m = max(1e-9, max(abs(s) for s in samples))
    data = b"".join(struct.pack("<h", int(clamp(s * peak / m, -1, 1) * 32767)) for s in samples)
    with wave.open(path("audio", name + ".wav"), "wb") as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(SR)
        f.writeframes(data)


def tone(freqs, dur, decay, attack=0.004):
    n = int(dur * SR)
    out = []
    for i in range(n):
        t = i / SR
        env = min(1, t / attack) * math.exp(-t / decay)
        s = 0.0
        for f, a in freqs:
            s += a * math.sin(2 * math.pi * f * t)
        out.append(s * env)
    return out


def sounds():
    # Notification: two soft bell partials, a fifth apart.
    a = tone([(880, 1), (1760, 0.25), (2640, 0.08)], 0.5, 0.16)
    b = tone([(1318.5, 1), (2637, 0.2)], 0.7, 0.22)
    gap = int(0.09 * SR)
    write_wav("notify", [x + (b[i - gap] if i >= gap else 0) for i, x in enumerate(a + [0] * gap)])
    # Incoming message: a rounded "pop" (pitch drops quickly).
    n = int(0.16 * SR)
    pop = []
    ph = 0.0
    for i in range(n):
        t = i / SR
        f = 520 + 900 * math.exp(-t / 0.018)
        ph += 2 * math.pi * f / SR
        pop.append(math.sin(ph) * min(1, t / 0.002) * math.exp(-t / 0.045))
    write_wav("pop", pop, 0.7)
    # Sent: a short rising swish.
    rng = random.Random(4)
    n = int(0.22 * SR)
    sw, y, ph = [], 0.0, 0.0
    for i in range(n):
        t = i / SR
        f = 600 + 1600 * (t / 0.22)
        ph += 2 * math.pi * f / SR
        y += 0.2 * (rng.uniform(-1, 1) - y)
        env = math.sin(math.pi * t / 0.22) ** 2
        sw.append((0.5 * math.sin(ph) + 0.6 * y) * env)
    write_wav("send", sw, 0.55)
    # Startup chime: a soft major chord swell.
    ch = tone([(392, 0.8), (493.9, 0.6), (587.3, 0.6), (784, 0.35)], 1.8, 0.7, 0.12)
    write_wav("startup", ch, 0.6)


if __name__ == "__main__":
    chrome()
    for g in GLYPHS:
        glyph(g)
    for a in APPS:
        app_icon(a)
    for a in AVATARS:
        avatar(a)
    sounds()
    with Pool(4) as pool:
        for n in pool.imap_unordered(photo_job, list(PHOTOS)):
            print("painted", n)
    print("done")
