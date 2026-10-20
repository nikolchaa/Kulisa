# Repaints the nine-slice images of templates/classic-vn (text box, name box, frame, buttons) with
# soft gradients, an inner glow and a lit border. Pure Python. Usage: python paint_gui.py OUTDIR
import math, os, struct, sys, zlib

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)


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


def sd_box(x, y, w, h, r):
    qx, qy = abs(x - w / 2) - (w / 2 - r), abs(y - h / 2) - (h / 2 - r)
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r


def panel(path, w, h, r, top, bottom, border, border_a, glow, glow_a, ss=3):
    px = bytearray(w * h * 4)
    for y in range(h):
        for x in range(w):
            acc = [0.0] * 4
            for sy in range(ss):
                for sx in range(ss):
                    fx, fy = x + (sx + 0.5) / ss, y + (sy + 0.5) / ss
                    d = sd_box(fx, fy, w, h, r)
                    a = clamp(0.5 - d)
                    if a <= 0:
                        continue
                    t = fy / h
                    c = [top[i] + (bottom[i] - top[i]) * t for i in range(3)]
                    al = top[3] + (bottom[3] - top[3]) * t
                    # Inner glow near the edge.
                    g = math.exp(-(max(-d, 0) / 5.0) ** 2) * glow_a
                    c = [c[i] * (1 - g) + glow[i] * g for i in range(3)]
                    # A thin lit border, brighter at the top.
                    b = (1 - smooth(0.6, 1.6, abs(d + 1.0))) * border_a * (0.55 + 0.45 * (1 - t))
                    c = [c[i] * (1 - b) + border[i] * b for i in range(3)]
                    al = al + (1 - al) * (b * 0.85 + g * 0.2)
                    acc[0] += c[0] * a * al
                    acc[1] += c[1] * a * al
                    acc[2] += c[2] * a * al
                    acc[3] += a * al
            o = (y * w + x) * 4
            if acc[3] > 0:
                px[o:o + 3] = bytes(int(clamp(acc[i] / acc[3]) * 255 + 0.5) for i in range(3))
                px[o + 3] = int(clamp(acc[3] / (ss * ss)) * 255 + 0.5)
    write_png(os.path.join(OUT, path), w, h, px)
    print("wrote", path)


# The text box: deep indigo glass, a violet-to-coral lit edge.
panel("textbox.png", 64, 64, 20, [0.06, 0.06, 0.13, 0.86], [0.04, 0.04, 0.09, 0.92], [1.0, 0.62, 0.5], 0.8, [0.6, 0.4, 0.9], 0.18)
panel("namebox.png", 48, 48, 14, [0.16, 0.1, 0.24, 0.95], [0.1, 0.07, 0.18, 0.97], [1.0, 0.7, 0.55], 0.85, [0.9, 0.5, 0.6], 0.2)
panel("frame.png", 64, 64, 16, [0.09, 0.09, 0.18, 0.78], [0.06, 0.06, 0.12, 0.82], [0.7, 0.62, 1.0], 0.55, [0.45, 0.4, 0.9], 0.12)
