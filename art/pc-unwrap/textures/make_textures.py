# Generates the textures the Blockbench models in art/pc-unwrap use (pure Python PNG writer, no
# dependencies). Run it, then copy the PNGs next to the .bbmodel files.
import zlib, struct, random, os, math

def png(path, w, h, px):
    rows = []
    for y in range(h):
        rows.append(b"\x00" + b"".join(bytes(p) for p in px[y * w:(y + 1) * w]))
    raw = b"".join(rows)
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

here = os.path.dirname(os.path.abspath(__file__))

def clamp(v): return max(0, min(255, int(v)))

def board(path, w, h, base, seed, flute=True, var=7, drawf=None):
    r = random.Random(seed)
    px = []
    for y in range(h):
        for x in range(w):
            n = (r.random() - 0.5) * 2 * var
            fl = (math.sin(y * 1.9) * 3.0) if flute else 0
            blotch = math.sin(x * 0.021 + math.sin(y * 0.03) * 2) * 3 + math.sin((x + y) * 0.013) * 2
            shade = 1.0 - 0.1 * (y / h)
            px.append([clamp((base[i] + n + fl + blotch) * shade) for i in range(3)] + [255])
    if drawf:
        drawf(px, w, h)
    png(os.path.join(here, path), w, h, px)

def fill(px, w, x0, y0, x1, y1, col):
    for y in range(int(y0), int(y1)):
        for x in range(int(x0), int(x1)):
            px[y * w + x] = list(col)[:3] + [255]

def blend(px, w, x0, y0, x1, y1, col, a):
    for y in range(int(y0), int(y1)):
        for x in range(int(x0), int(x1)):
            p = px[y * w + x]
            px[y * w + x] = [clamp(p[i] * (1 - a) + col[i] * a) for i in range(3)] + [255]

def tri_up(px, w, cx, top, hw, h, col):
    for k in range(int(h)):
        half = hw * k / h
        fill(px, w, cx - half, top + k, cx + half, top + k + 1, col)

def front_print(px, w, h):
    ink = (92, 62, 38)
    # Arrows pointing up, left and right.
    for cx in (w * 0.12, w * 0.88):
        tri_up(px, w, cx, h * 0.18, 26, 34, ink)
        fill(px, w, cx - 9, h * 0.18 + 34, cx + 9, h * 0.18 + 78, ink)
        fill(px, w, cx - 26, h * 0.18 + 90, cx + 26, h * 0.18 + 96, ink)
    # A white shipping label with a barcode and text lines.
    lx0, ly0, lx1, ly1 = w * 0.62, h * 0.52, w * 0.82, h * 0.82
    fill(px, w, lx0, ly0, lx1, ly1, (238, 236, 228))
    for i in range(34):
        bw = 1 + (i * 7) % 3
        fill(px, w, lx0 + 10 + i * 4.2, ly0 + 12, lx0 + 10 + i * 4.2 + bw, ly0 + 56, (30, 30, 34))
    for k in range(3):
        fill(px, w, lx0 + 10, ly0 + 66 + k * 12, lx0 + 10 + (110 - k * 30), ly0 + 71 + k * 12, (70, 70, 76))
    # A glass icon (a wine-glass-ish "fragile" mark) in red.
    red = (176, 46, 40)
    fill(px, w, w * 0.30, h * 0.5, w * 0.30 + 40, h * 0.5 + 6, red)
    fill(px, w, w * 0.30 + 17, h * 0.5 + 6, w * 0.30 + 23, h * 0.5 + 50, red)
    fill(px, w, w * 0.30, h * 0.5 + 50, w * 0.30 + 40, h * 0.5 + 56, red)
    # Hanging tape residue stripe across the top seam.
    blend(px, w, 0, 0, w, 6, (60, 40, 20), 0.35)

board("cardboard.png", 256, 256, (236, 188, 132), 1)
board("cardboard_inner.png", 256, 256, (204, 152, 100), 2, var=9)
board("cardboard_edge.png", 64, 64, (170, 128, 84), 3, var=10)
board("box_front.png", 512, 256, (232, 184, 128), 4, drawf=front_print)
board("box_side.png", 256, 256, (228, 180, 124), 5, drawf=lambda px, w, h: (tri_up(px, w, w * 0.5, h * 0.25, 22, 30, (92, 62, 38)), fill(px, w, w * 0.5 - 8, h * 0.25 + 30, w * 0.5 + 8, h * 0.25 + 70, (92, 62, 38))))

def foam(path, seed):
    r = random.Random(seed)
    w = h = 256
    px = []
    for y in range(h):
        for x in range(w):
            # Beads: cell noise at ~6 px.
            cx, cy = x // 6, y // 6
            rr = random.Random(cx * 7919 + cy * 104729 + seed)
            b = rr.random()
            v = 226 + b * 22 + (r.random() - 0.5) * 8
            d = math.hypot((x % 6) - 2.5, (y % 6) - 2.5)
            v -= 6 * (d > 2.6)
            px.append([clamp(v), clamp(v), clamp(v + 3), 255])
    png(os.path.join(here, path), w, h, px)

foam("foam.png", 11)

def flat(path, w, h, col, a=255, noise=0, seed=0):
    r = random.Random(seed)
    px = [[clamp(col[0] + (r.random() - 0.5) * noise), clamp(col[1] + (r.random() - 0.5) * noise), clamp(col[2] + (r.random() - 0.5) * noise), a] for _ in range(w * h)]
    png(os.path.join(here, path), w, h, px)

flat("case_dark.png", 64, 64, (20, 22, 30), noise=4, seed=5)
flat("clear.png", 8, 8, (0, 0, 0), a=0)
flat("neon_purple.png", 8, 8, (200, 100, 255))
flat("neon_blue.png", 8, 8, (70, 140, 255))
flat("neon_cyan.png", 8, 8, (60, 230, 255))

def glass(path):
    w, h = 512, 256
    px = []
    for y in range(h):
        for x in range(w):
            g = [14 + 10 * (y / h), 16 + 10 * (y / h), 30 + 14 * (y / h), 235]
            px.append([clamp(v) for v in g])
    # Neon trims and a few fan rings.
    def ring(cx, cy, r0, r1, col):
        for y in range(int(cy - r1 - 2), int(cy + r1 + 2)):
            for x in range(int(cx - r1 - 2), int(cx + r1 + 2)):
                if 0 <= x < w and 0 <= y < h:
                    d = math.hypot(x - cx, y - cy)
                    if r0 <= d <= r1:
                        px[y * w + x] = list(col) + [255]
    fill(px, w, 0, 0, w, 3, (200, 100, 255)); fill(px, w, 0, h - 3, w, h, (70, 140, 255))
    for cx in (110, 256, 402):
        ring(cx, 190, 24, 27, (70, 140, 255)); ring(cx, 190, 4, 8, (50, 60, 100))
    ring(80, 70, 30, 33, (200, 100, 255)); ring(256, 110, 14, 16, (60, 230, 255))
    fill(px, w, 330, 60, 420, 140, (24, 28, 44)); fill(px, w, 330, 60, 420, 63, (200, 100, 255))
    png(os.path.join(here, path), w, h, px)

glass("case_front.png")
print("ok")
