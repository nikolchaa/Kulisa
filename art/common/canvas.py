# Small pure-Python painting kit shared by the art scripts: an RGBA float canvas, signed distance
# shapes with anti-aliasing, gradients, value noise, blur and a PNG writer. No dependencies.
import math, struct, zlib


def clamp(v, a=0.0, b=1.0):
    return a if v < a else (b if v > b else v)


def smooth(a, b, x):
    t = clamp((x - a) / (b - a)) if b != a else (1.0 if x >= a else 0.0)
    return t * t * (3 - 2 * t)


def mix(a, b, t):
    return [a[i] + (b[i] - a[i]) * t for i in range(len(a))]


def hexc(s, a=1.0):
    s = s.lstrip("#")
    c = [int(s[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    if len(s) == 8:
        a = int(s[6:8], 16) / 255
    return c + [a]


def grad(stops, t):
    """Color along `stops` = [(pos, color), ...] (positions ascending)."""
    t = clamp(t)
    for i in range(len(stops) - 1):
        p0, c0 = stops[i]
        p1, c1 = stops[i + 1]
        if t <= p1:
            return mix(c0, c1, (t - p0) / (p1 - p0) if p1 > p0 else 0)
    return list(stops[-1][1])


# ---- noise -----------------------------------------------------------------------------

def hash2(x, y, seed=0):
    n = (x * 374761393 + y * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 65535.0


def vnoise(x, y, seed=0):
    xi, yi = math.floor(x), math.floor(y)
    fx, fy = x - xi, y - yi
    fx = fx * fx * (3 - 2 * fx)
    fy = fy * fy * (3 - 2 * fy)
    a = hash2(xi, yi, seed)
    b = hash2(xi + 1, yi, seed)
    c = hash2(xi, yi + 1, seed)
    d = hash2(xi + 1, yi + 1, seed)
    return a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy


def fbm(x, y, octaves=4, seed=0, lac=2.0, gain=0.5):
    s, amp, norm = 0.0, 1.0, 0.0
    for o in range(octaves):
        s += vnoise(x, y, seed + o * 17) * amp
        norm += amp
        x *= lac
        y *= lac
        amp *= gain
    return s / norm


# ---- distance functions (negative inside) ----------------------------------------------

def sd_circle(x, y, cx, cy, r):
    return math.hypot(x - cx, y - cy) - r


def sd_box(x, y, cx, cy, hw, hh, r=0.0):
    qx, qy = abs(x - cx) - (hw - r), abs(y - cy) - (hh - r)
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r


def sd_box4(x, y, cx, cy, hw, hh, r):
    """Rounded box with per-corner radii r = (tl, tr, br, bl)."""
    if x >= cx:
        rr = r[1] if y < cy else r[2]
    else:
        rr = r[0] if y < cy else r[3]
    return sd_box(x, y, cx, cy, hw, hh, rr)


def sd_segment(x, y, ax, ay, bx, by):
    px, py, dx, dy = x - ax, y - ay, bx - ax, by - ay
    h = clamp((px * dx + py * dy) / (dx * dx + dy * dy + 1e-12))
    return math.hypot(px - dx * h, py - dy * h)


def sd_ellipse(x, y, cx, cy, rx, ry):
    return (math.hypot((x - cx) / rx, (y - cy) / ry) - 1.0) * min(rx, ry)


def sd_superellipse(x, y, cx, cy, r, n=5.0):
    """Approximate distance to a squircle of half size r."""
    ax, ay = abs(x - cx) / r, abs(y - cy) / r
    v = (ax ** n + ay ** n) ** (1.0 / n)
    return (v - 1.0) * r


def sd_arc(x, y, cx, cy, r, a0, a1):
    """Distance to a circular arc from angle a0 to a1 (radians, a0 < a1, y down)."""
    a = math.atan2(y - cy, x - cx)
    while a < a0:
        a += 2 * math.pi
    if a <= a1:
        return abs(math.hypot(x - cx, y - cy) - r)
    e0 = (cx + math.cos(a0) * r, cy + math.sin(a0) * r)
    e1 = (cx + math.cos(a1) * r, cy + math.sin(a1) * r)
    return min(math.hypot(x - e0[0], y - e0[1]), math.hypot(x - e1[0], y - e1[1]))


def sd_triangle(x, y, a, b, c):
    def side(p0, p1):
        return (p1[0] - p0[0]) * (y - p0[1]) - (p1[1] - p0[1]) * (x - p0[0])
    d = min(sd_segment(x, y, *a, *b), sd_segment(x, y, *b, *c), sd_segment(x, y, *c, *a))
    s1, s2, s3 = side(a, b), side(b, c), side(c, a)
    inside = (s1 >= 0 and s2 >= 0 and s3 >= 0) or (s1 <= 0 and s2 <= 0 and s3 <= 0)
    return -d if inside else d


def cov(d, aa=1.0):
    """Coverage of a pixel at signed distance d (pixels)."""
    return clamp(0.5 - d / aa)


# ---- canvas -------------------------------------------------------------------------------

class Canvas:
    def __init__(self, w, h, fill=(0, 0, 0, 0)):
        self.w, self.h = w, h
        self.px = [list(fill) for _ in range(w * h)]

    def get(self, x, y):
        return self.px[y * self.w + x]

    def blend(self, x, y, c, a=None):
        """Composites straight color c (rgba) with extra coverage a over the pixel."""
        if a is None:
            a = 1.0
        sa = c[3] * a
        if sa <= 0:
            return
        d = self.px[y * self.w + x]
        da = d[3]
        oa = sa + da * (1 - sa)
        if oa <= 0:
            return
        for i in range(3):
            d[i] = (c[i] * sa + d[i] * da * (1 - sa)) / oa
        d[3] = oa

    def add(self, x, y, c, k=1.0):
        d = self.px[y * self.w + x]
        for i in range(3):
            d[i] = d[i] + c[i] * k

    def shape(self, sdf, color, bbox=None, aa=1.0):
        """Fills where sdf(x+0.5, y+0.5) < 0; color is rgba or a function (x, y) -> rgba."""
        x0, y0, x1, y1 = bbox if bbox else (0, 0, self.w, self.h)
        x0, y0 = max(0, int(x0)), max(0, int(y0))
        x1, y1 = min(self.w, int(math.ceil(x1))), min(self.h, int(math.ceil(y1)))
        fn = callable(color)
        for y in range(y0, y1):
            for x in range(x0, x1):
                d = sdf(x + 0.5, y + 0.5)
                k = cov(d, aa)
                if k > 0:
                    self.blend(x, y, color(x + 0.5, y + 0.5) if fn else color, k)

    def paint(self, fn):
        """Sets every pixel to fn(x, y) (rgba)."""
        for y in range(self.h):
            for x in range(self.w):
                self.px[y * self.w + x] = list(fn(x + 0.5, y + 0.5))

    def over(self, other, ox=0, oy=0):
        for y in range(other.h):
            ty = y + oy
            if 0 <= ty < self.h:
                for x in range(other.w):
                    tx = x + ox
                    if 0 <= tx < self.w:
                        c = other.px[y * other.w + x]
                        if c[3] > 0:
                            self.blend(tx, ty, c)

    def blur_alpha(self, r, passes=3):
        """Box-blurs the alpha channel (soft shadows)."""
        a = [p[3] for p in self.px]
        w, h = self.w, self.h
        for _ in range(passes):
            out = [0.0] * (w * h)
            for y in range(h):
                row = a[y * w:(y + 1) * w]
                acc = 0.0
                pre = [0.0]
                for v in row:
                    acc += v
                    pre.append(acc)
                for x in range(w):
                    lo, hi = max(0, x - r), min(w, x + r + 1)
                    out[y * w + x] = (pre[hi] - pre[lo]) / (2 * r + 1)
            a = out
            out = [0.0] * (w * h)
            for x in range(w):
                acc = 0.0
                pre = [0.0]
                for y in range(h):
                    acc += a[y * w + x]
                    pre.append(acc)
                for y in range(h):
                    lo, hi = max(0, y - r), min(h, y + r + 1)
                    out[y * w + x] = (pre[hi] - pre[lo]) / (2 * r + 1)
            a = out
        for p, v in zip(self.px, a):
            p[3] = v

    def save(self, path):
        write_png(path, self.w, self.h, self.px)


def write_png(path, w, h, px, alpha=True):
    rows = bytearray()
    for y in range(h):
        rows.append(0)
        for x in range(w):
            p = px[y * w + x]
            rows.append(int(clamp(p[0]) * 255 + 0.5))
            rows.append(int(clamp(p[1]) * 255 + 0.5))
            rows.append(int(clamp(p[2]) * 255 + 0.5))
            if alpha:
                rows.append(int(clamp(p[3]) * 255 + 0.5))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    ctype = 6 if alpha else 2
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, ctype, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b""))
