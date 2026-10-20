# Paints the two characters of templates/classic-vn (Mila and Ren, two expressions each) as
# layered, shaded 2D shapes: hair with highlights, a face with big eyes, clothes with soft folds.
# Pure Python, no dependencies. Usage: python paint_chars.py OUTDIR [scale]
import math, os, struct, sys, zlib

OUT = sys.argv[1]
SCALE = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
os.makedirs(OUT, exist_ok=True)
W, H = 600, 900
SS = int(os.environ.get("SS", "2"))


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


def lin(c, f):
    return [clamp(v * f) for v in c]


# ---- Signed distances (negative inside) -----------------------------------------------------------
def sd_ellipse(x, y, cx, cy, rx, ry):
    dx, dy = (x - cx) / rx, (y - cy) / ry
    return (math.hypot(dx, dy) - 1.0) * min(rx, ry)


def sd_capsule(x, y, ax, ay, bx, by, r):
    pax, pay = x - ax, y - ay
    bax, bay = bx - ax, by - ay
    h = clamp((pax * bax + pay * bay) / (bax * bax + bay * bay))
    return math.hypot(pax - bax * h, pay - bay * h) - r


def sd_box(x, y, cx, cy, hw, hh, r=0.0):
    qx, qy = abs(x - cx) - (hw - r), abs(y - cy) - (hh - r)
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r


def smin(a, b, k):
    h = clamp(0.5 + 0.5 * (b - a) / k)
    return b + (a - b) * h - k * h * (1 - h)


def cov(d):
    return clamp(0.5 - d)


# ---- A character painter ----------------------------------------------------------------------------
class Look:
    def __init__(self, **kw):
        self.__dict__.update(kw)


def over(dst, src, a):
    """dst, src: [r, g, b]; a: coverage. Returns the composited color (dst carries alpha separately)."""
    return [dst[i] * (1 - a) + src[i] * a for i in range(3)]


def hair_shade(base, x, y, cx, cy, spread, strand=1.0):
    """Hair color with a vertical gradient, strand lines and a curved highlight band."""
    t = clamp((y - (cy - spread)) / (2 * spread))
    c = mix(lin(base, 1.25), lin(base, 0.55), smooth(0.0, 1.0, t))
    s = 0.5 + 0.5 * math.sin((x - cx) * 0.22 + math.sin(y * 0.02) * 2.0)
    c = lin(c, 0.9 + 0.1 * s * strand)
    # Highlight: a soft arc across the top of the head.
    d = abs(math.hypot((x - cx) / (spread * 1.0), (y - (cy + spread * 0.15)) / (spread * 0.9)) - 0.78)
    hl = (1 - smooth(0.02, 0.1, d)) * smooth(cy + spread * 0.1, cy - spread * 0.5, y)
    return mix(c, [min(1, c[i] + 0.35) for i in range(3)], hl * 0.55)


def face_color(look, x, y, expression):
    """Skin and facial features at a point of the face rectangle."""
    u, v = (x - 190) / 220.0, (y - 270) / 200.0
    skin = look.skin
    c = mix(skin, lin(skin, 0.9), smooth(0.4, 1.0, v))
    # Soft shadow under the hair line.
    c = mix(c, lin(skin, 0.82), 0.5 * (1 - smooth(0.0, 0.22, v)))
    # Blush.
    for cx in (0.24, 0.76):
        d = math.hypot((u - cx) / 0.13, (v - 0.74) / 0.075)
        c = mix(c, look.blush, 0.5 * (1 - smooth(0.3, 1.0, d)))
    lash = lin(look.lash, 1.0)
    for ex, sgn in ((0.29, 1), (0.71, -1)):
        if expression == "happy":
            t = (u - ex) / 0.14
            if abs(t) < 1.15:
                yy = 0.58 - 0.1 * (1 - t * t)
                thick = 0.016 + 0.012 * (1 - abs(t))
                c = mix(c, lash, 1 - smooth(thick * 0.6, thick, abs(v - yy)))
        else:
            d = math.hypot((u - ex) / 0.125, (v - 0.56) / 0.17)
            if d < 1.12:
                col = [0.98, 0.98, 1.0]
                idv = math.hypot((u - ex) / 0.104, (v - 0.585) / 0.146)
                if idv < 1.0:
                    t = clamp((v - 0.44) / 0.3)
                    col = mix(lin(look.iris, 0.4), lin(look.iris, 1.35), t)
                    col = mix(col, lin(look.iris, 0.25), 1 - smooth(0.6, 1.0, idv) if idv > 0.78 else 0)
                    pup = math.hypot((u - ex) / 0.052, (v - 0.585) / 0.07)
                    col = mix(col, [0.04, 0.02, 0.06], 1 - smooth(0.85, 1.0, pup))
                    h1 = math.hypot((u - ex - 0.04 * sgn) / 0.03, (v - 0.5) / 0.038)
                    h2 = math.hypot((u - ex + 0.034 * sgn) / 0.016, (v - 0.64) / 0.019)
                    col = mix(col, [1, 1, 1], max(1 - smooth(0.85, 1.0, h1), 1 - smooth(0.8, 1.0, h2)))
                c = mix(c, col, 1 - smooth(0.96, 1.08, d))
            # Heavy upper lash line and a little flick.
            top = math.hypot((u - ex) / 0.136, (v - 0.56) / 0.185)
            if v < 0.57:
                c = mix(c, lash, (1 - smooth(0.05, 0.09, abs(top - 1.0))) * smooth(0.62, 0.5, v))
            fx = (u - (ex + 0.135 * sgn)) / 0.035
            if abs(fx) < 1 and abs(v - (0.5 - 0.025 * abs(fx))) < 0.014:
                c = mix(c, lash, 0.9)
        # Brows.
        bt = (u - ex) / 0.12
        if abs(bt) < 1:
            by = (0.36 - 0.012 * (1 - bt * bt)) + (0.0 if expression == "happy" else -0.012 * bt * sgn)
            c = mix(c, lin(look.brow, 1.0), 0.85 * (1 - smooth(0.007, 0.015, abs(v - by))) * (1 - smooth(0.6, 1.0, abs(bt))))
    # Nose: a tiny shadow.
    c = mix(c, lin(skin, 0.78), 0.45 * (1 - smooth(0.0, 0.02, math.hypot(u - 0.5, v - 0.7))))
    # Mouth.
    if expression == "happy":
        mx, my = (u - 0.5) / 0.14, (v - 0.84) / 0.075
        r = math.hypot(mx, my)
        if my > 0 and r < 1.0:
            c = mix(c, [0.5, 0.12, 0.18], 1.0)
            c = mix(c, [0.98, 0.55, 0.6], 0.8 * smooth(0.2, 0.9, my) * smooth(0.5, 0.2, abs(mx)))
        c = mix(c, lash, 0.85 * (1 - smooth(0.05, 0.12, abs(r - 1.0))) * (1.0 if my > -0.08 else 0.0))
    else:
        mx = (u - 0.5) / 0.075
        if abs(mx) < 1:
            my = 0.845 + 0.012 * (mx * mx - 1) * -1
            c = mix(c, [0.62, 0.26, 0.32], 0.9 * (1 - smooth(0.005, 0.011, abs(v - my))))
    return c


def paint(path, look, expression):
    px = bytearray(W * H * 4)
    head = (300, 335, 126, 156)  # cx, cy, rx, ry (face)
    for y in range(H):
        for x in range(W):
            acc = [0.0, 0.0, 0.0]
            alpha = 0.0
            for sy in range(SS):
                for sx in range(SS):
                    fx, fy = x + (sx + 0.5) / SS, y + (sy + 0.5) / SS
                    col, a = shade_pixel(look, expression, fx, fy, head)
                    if a > 0:
                        acc = [acc[i] + col[i] * a for i in range(3)]
                        alpha += a
            o = (y * W + x) * 4
            if alpha > 0:
                px[o:o + 3] = bytes(int(clamp(acc[i] / alpha) * 255 + 0.5) for i in range(3))
                px[o + 3] = int(255 * alpha / (SS * SS) + 0.5)
    if SCALE != 1.0:
        w2, h2 = int(W * SCALE), int(H * SCALE)
        px, W2, H2 = scale_down(px, SCALE)
        write_png(path, W2, H2, px)
    else:
        write_png(path, W, H, px)
    print("wrote", path)


def scale_down(px, f):
    nw, nh = int(W * f), int(H * f)
    out = bytearray(nw * nh * 4)
    for y in range(nh):
        for x in range(nw):
            r = g = b = a = wsum = 0.0
            for yy in range(int(y / f), min(H, int((y + 1) / f) + 1)):
                for xx in range(int(x / f), min(W, int((x + 1) / f) + 1)):
                    i = (yy * W + xx) * 4
                    al = px[i + 3] / 255.0
                    r += px[i] * al
                    g += px[i + 1] * al
                    b += px[i + 2] * al
                    a += al
                    wsum += 1
            o = (y * nw + x) * 4
            if a > 0:
                out[o], out[o + 1], out[o + 2] = int(r / a), int(g / a), int(b / a)
            out[o + 3] = int(255 * a / wsum) if wsum else 0
    return out, nw, nh


def shade_pixel(look, expression, x, y, head):
    """Composites the layers back to front at one point; returns (color, alpha)."""
    col = [0, 0, 0]
    alpha = 0.0

    def put(c, a):
        nonlocal col, alpha
        if a <= 0:
            return
        # "over" with accumulated alpha.
        na = a + alpha * (1 - a)
        col = [(c[i] * a + col[i] * alpha * (1 - a)) / na for i in range(3)]
        alpha = na

    cx, cy, rx, ry = head
    # 1. Back hair (and the twin tails or the short back).
    d_back = sd_ellipse(x, y, cx, cy - 20, rx + 48, ry + 62)
    if look.kind == "girl":
        d_back = smin(d_back, sd_capsule(x, y, 140, 330, 105, 650, 46 - 0.02 * max(0, y - 330)), 24)
        d_back = smin(d_back, sd_capsule(x, y, 460, 330, 495, 650, 46 - 0.02 * max(0, y - 330)), 24)
    else:
        d_back = sd_ellipse(x, y, cx, cy - 30, rx + 34, ry + 40)
    put(hair_shade(look.hair_dark, x, y, cx, cy, 190, 0.6), cov(d_back))
    # 2. Torso and arms.
    sh = sd_box(x, y, 300, 800, 150, 230, 64)
    sh = smin(sh, sd_ellipse(x, y, 300, 620, 118, 62), 50)
    arm_l = sd_capsule(x, y, 165, 650, 140, 900, 48)
    arm_r = sd_capsule(x, y, 435, 650, 460, 900, 48)
    body = smin(sh, smin(arm_l, arm_r, 18), 22)
    if body < 1.0:
        t = clamp((y - 560) / 340)
        c = mix(lin(look.top, 1.15), lin(look.top, 0.78), smooth(0, 1, t))
        # Folds: soft vertical shading, darker toward the sides.
        fold = 0.5 + 0.5 * math.sin((x - 300) * 0.05 + math.sin(y * 0.01) * 1.5)
        c = lin(c, 0.93 + 0.07 * fold)
        c = lin(c, 1.0 - 0.18 * smooth(100, 170, abs(x - 300)))
        # Arm/torso separation line.
        sep = min(abs(sd_capsule(x, y, 195, 640, 190, 900, 0) * 0 + abs(x - 200) - 0), 99)
        c = mix(c, lin(look.top, 0.62), 0.35 * (1 - smooth(1.0, 6.0, abs(abs(x - 300) - 118))) * smooth(640, 700, y))
        if look.kind == "boy":
            # A zip line and pocket flap.
            c = mix(c, lin(look.top, 0.55), 0.7 * (1 - smooth(0.8, 2.2, abs(x - 300))) * smooth(560, 600, y))
        put(c, cov(body))
        # Collar.
        if look.kind == "girl":
            vd = sd_box(x, y, 300, 575, 82, 34, 18)
            vd = max(vd, -sd_ellipse(x, y, 300, 525, 62, 66))
            put(mix(look.collar, lin(look.collar, 0.85), smooth(560, 610, y)), cov(vd))
            bow = min(sd_ellipse(x, y, 268, 625, 34, 22), sd_ellipse(x, y, 332, 625, 34, 22))
            bow = min(bow, sd_ellipse(x, y, 300, 625, 12, 14))
            put(mix(look.accent, lin(look.accent, 0.7), smooth(610, 645, y)), cov(bow))
        else:
            vd = max(sd_box(x, y, 300, 570, 90, 44, 16), -sd_ellipse(x, y, 300, 520, 52, 58))
            put(mix(lin(look.top, 1.3), look.top, smooth(550, 610, y)), cov(vd))
    # 3. Neck.
    neck = sd_box(x, y, 300, 500, 36, 64, 22)
    put(mix(lin(look.skin, 0.9), lin(look.skin, 0.74), smooth(470, 560, y) * 0.6 + 0.2 * smooth(460, 520, y)), cov(neck))
    # 4. Head and face.
    # A slightly pointed chin: the lower half is narrower.
    k = 1.0 if y < cy else 1.0 + 0.0
    d_head = sd_ellipse(x, y, cx, cy, rx, ry * (1.0 if y < cy else 1.02))
    if y > cy:
        d_head = smin(d_head, sd_ellipse(x, y, cx, cy + 20, rx * 0.93, ry * 0.98), 20)
    if d_head < 1.0:
        c = face_color(look, x, y, expression)
        edge = smooth(-1.5, 0.5, d_head)
        put(mix(c, lin(look.skin, 0.8), 0.0 * edge), cov(d_head))
    # 5. Front hair: the fringe, side locks and the crown.
    crown = sd_ellipse(x, y, cx, cy - 16, rx + 30, ry + 34)
    # The hairline: lower at the sides, a gentle parting in the middle.
    xo = (x - cx)
    line_y = (cy - 70) + 62 * smooth(0, 140, abs(xo)) + 7 * math.sin(xo * 0.09) + 10 * smooth(60, 0, abs(xo))
    fringe = max(crown, y - line_y)
    # Strand cuts: a few notches along the fringe's lower edge.
    notch = 9 * max(0.0, math.sin(xo * 0.075 + 0.6)) ** 2
    fringe = max(crown, y - (line_y + notch + 18 * smooth(20, 120, abs(xo)) ))
    locks = min(sd_capsule(x, y, cx - 130, cy - 20, cx - 122, cy + 115, 24 - 0.04 * max(0, y - cy)),
                sd_capsule(x, y, cx + 130, cy - 20, cx + 122, cy + 115, 24 - 0.04 * max(0, y - cy)))
    front = smin(fringe, locks, 14)
    front = max(front, y - (cy + 140))
    if front < 1.0:
        c = hair_shade(look.hair, x, y, cx, cy - 20, 150)
        # Darker where the fringe meets the face (a soft cast shadow on the skin).
        put(c, cov(front))
    # Cast shadow of the fringe on the forehead.
    sh_d = front - 0.0
    if 0 < sh_d < 14 and d_head < 0 and alpha > 0.5:
        a = (1 - smooth(0, 14, sh_d)) * 0.22
        col = mix(col, lin(look.skin, 0.62), a)
    # 6. Accessories: ribbons for Mila.
    if look.kind == "girl":
        for sx_ in (-1, 1):
            bx = cx + sx_ * 128
            b = min(sd_ellipse(x, y, bx + sx_ * 18, 232, 30, 20), sd_ellipse(x, y, bx - sx_ * 18, 232, 30, 20))
            b = min(b, sd_ellipse(x, y, bx, 232, 11, 13))
            put(mix(lin(look.accent, 1.1), lin(look.accent, 0.72), smooth(215, 250, y)), cov(b))
    else:
        # A spiky tuft on top.
        for ox, oy, tip in ((-30, 160, 120), (10, 150, 100), (46, 165, 128)):
            d = sd_capsule(x, y, cx + ox, oy + 60, cx + ox + 12, tip, 18 - 0.0)
            put(hair_shade(look.hair, x, y, cx, cy - 20, 150), cov(d))
    return col, alpha


mila = Look(kind="girl", skin=[1.0, 0.85, 0.76], blush=[1.0, 0.5, 0.58], lash=[0.2, 0.09, 0.14], brow=[0.3, 0.16, 0.2],
            iris=[0.82, 0.32, 0.55], hair=[0.4, 0.17, 0.3], hair_dark=[0.3, 0.12, 0.22], top=[0.93, 0.55, 0.72],
            collar=[1.0, 0.96, 0.97], accent=[1.0, 0.8, 0.35])
ren = Look(kind="boy", skin=[0.98, 0.82, 0.7], blush=[0.98, 0.58, 0.55], lash=[0.12, 0.12, 0.2], brow=[0.15, 0.2, 0.3],
           iris=[0.25, 0.55, 0.8], hair=[0.24, 0.36, 0.52], hair_dark=[0.17, 0.26, 0.4], top=[0.28, 0.62, 0.7],
           collar=[0.9, 0.95, 0.98], accent=[0.95, 0.55, 0.35])

for name, look in (("mila", mila), ("ren", ren)):
    for expression in ("neutral", "happy"):
        if os.environ.get("ONLY", f"{name} {expression}") == f"{name} {expression}":
            paint(os.path.join(OUT, f"{name} {expression}.png"), look, expression)
