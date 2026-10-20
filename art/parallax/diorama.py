# Ray-marches the "diorama" of demos/parallax: a stone arch on a lake shore at golden hour, with
# rocks, pines and far mountains, and writes the picture and its exact depth map (white = near).
# The demo's depth shader turns the pair into a 2.5D scene. Pure Python, one process per core.
# Usage: python art/parallax/diorama.py demos/parallax [width height]
import math, os, sys
from multiprocessing import Pool

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "common"))
from canvas import write_png, clamp, mix, hexc  # noqa: E402

OUT = sys.argv[1] if len(sys.argv) > 1 else "demos/parallax"
W = int(sys.argv[2]) if len(sys.argv) > 2 else 960
H = int(sys.argv[3]) if len(sys.argv) > 3 else 540

NEAR, FAR = 1.0, 60.0
CAM = (0.0, 1.35, -1.0)
SUN = (0.12, 0.16, 1.0)
_l = math.sqrt(sum(c * c for c in SUN))
SUN = tuple(c / _l for c in SUN)


def hash2(ix, iy):
    n = (ix * 374761393 + iy * 668265263) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 65535.0


def noise(x, y):
    ix, iy = math.floor(x), math.floor(y)
    fx, fy = x - ix, y - iy
    fx = fx * fx * (3 - 2 * fx)
    fy = fy * fy * (3 - 2 * fy)
    a = hash2(ix, iy)
    b = hash2(ix + 1, iy)
    c = hash2(ix, iy + 1)
    d = hash2(ix + 1, iy + 1)
    return a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy


def fbm(x, y, o=4):
    s, a = 0.0, 0.5
    for _ in range(o):
        s += a * noise(x, y)
        x, y = x * 2.03 + 1.7, y * 2.03 - 0.9
        a *= 0.5
    return s


def ground_h(x, z):
    # Rolling shore rising to hills on both sides and towards the back; the lake is below 0.
    base = 0.55 * fbm(x * 0.18, z * 0.18) + 0.06 * fbm(x * 1.3, z * 1.3, 2)
    s = clamp((abs(x) - 4.0) / 10.0)
    hills = 2.2 * s * s * (3 - 2 * s) * (0.7 + 0.6 * fbm(x * 0.1, z * 0.1, 2)) + 1.2 * clamp((z - 18.0) / 20.0)
    lake = -0.9 * math.exp(-((x - 0.4) ** 2) / 9.0 - ((z - 8.0) ** 2) / 16.0)
    return base + hills + lake - 0.15


def smin(a, b, k):
    h = clamp(0.5 + 0.5 * (b - a) / k)
    return b + (a - b) * h - k * h * (1 - h)


ROCKS = [(-1.6, 0.15, 3.2, 0.55), (1.9, 0.1, 4.0, 0.7), (-0.6, -0.1, 2.4, 0.35), (2.6, 0.3, 6.5, 0.9),
         (-2.8, 0.4, 7.5, 1.1)]
PINES = [(-3.4, 9.0, 2.6), (-4.4, 11.0, 3.2), (3.8, 10.0, 2.8), (4.8, 12.5, 3.6), (-2.6, 13.0, 2.4),
         (5.6, 8.5, 2.2), (-5.4, 8.0, 2.6), (2.2, 15.0, 3.0), (-1.2, 16.5, 2.6)]
ARCH = (0.3, 0.0, 9.0)


def scene(x, y, z):
    """(distance, material): 1 ground, 2 rock, 3 arch, 4 pine, 5 water."""
    d = (y - ground_h(x, z)) * 0.55
    m = 1
    water = y + 0.05
    if water < d:
        d, m = water, 5
    for rx, ry, rz, rr in ROCKS:
        q = math.sqrt((x - rx) ** 2 + ((y - ry) * 1.25) ** 2 + (z - rz) ** 2) - rr
        if q < rr + 0.6:
            q += 0.12 * (noise(x * 3.1 + rz, y * 3.1 + z * 1.7) - 0.5) * rr * 2
        if q < d:
            d, m = q, 2
    # The arch: a torus segment standing on the shore.
    ax, ay, az = x - ARCH[0], y - ARCH[1], z - ARCH[2]
    qx = math.sqrt(ax * ax + ay * ay) - 2.2
    t = math.sqrt(qx * qx + (az * 0.8) ** 2) - 0.5
    t = max(t, -ay - 0.3)
    if t < 0.8:
        t += 0.1 * (noise(ax * 2.3 + az, ay * 2.3) - 0.5)
    if t < d:
        d, m = t, 3
    for px, pz, ph in PINES:
        gy = ground_h(px, pz)
        cx, cz = x - px, z - pz
        r = math.sqrt(cx * cx + cz * cz)
        hy = y - gy
        if r > 1.6 or hy < -0.5 or hy > ph + 0.5:
            continue
        # A stack of three cones with ragged edges.
        best = 1e9
        for k in range(3):
            top = ph * (0.55 + 0.22 * k)
            bot = top - ph * 0.45
            rad = ph * (0.26 - 0.06 * k)
            frac = clamp((top - hy) / (top - bot))
            c = max(r - rad * frac - 0.05 * noise(cx * 6, hy * 6), hy - top, bot - hy)
            best = min(best, c)
        trunk = max(r - 0.08, hy - ph * 0.4, -hy)
        best = min(best * 0.7, trunk)
        if best < d:
            d, m = best, 4
    return d, m


def march(ox, oy, oz, dx, dy, dz, tmax=FAR, steps=110):
    t = 0.05
    for _ in range(steps):
        x, y, z = ox + dx * t, oy + dy * t, oz + dz * t
        d, m = scene(x, y, z)
        if d < 0.002 * t:
            return t, m
        t += max(d, 0.01 * t)
        if t > tmax:
            break
    return None, 0


def normal(x, y, z):
    e = 0.01
    dx = scene(x + e, y, z)[0] - scene(x - e, y, z)[0]
    dy = scene(x, y + e, z)[0] - scene(x, y - e, z)[0]
    dz = scene(x, y, z + e)[0] - scene(x, y, z - e)[0]
    l = math.sqrt(dx * dx + dy * dy + dz * dz) or 1
    return dx / l, dy / l, dz / l


def shadow(x, y, z):
    t, res = 0.05, 1.0
    for _ in range(28):
        d, _m = scene(x + SUN[0] * t, y + SUN[1] * t, z + SUN[2] * t)
        if d < 0.001:
            return 0.0
        res = min(res, 9 * d / t)
        t += max(d, 0.05)
        if t > 12:
            break
    return clamp(res)


def sky(dx, dy, dz):
    up = clamp(dy * 2.2 + 0.2)
    col = mix(hexc("#f2a978"), hexc("#6b8cc7"), up ** 0.7)
    col = mix(col, hexc("#2c4580"), clamp(dy * 1.5) ** 1.5)
    col = [c * 0.8 for c in col[:3]]
    sd = dx * SUN[0] + dy * SUN[1] + dz * SUN[2]
    glow = max(0.0, sd) ** 16 * 1.2 + max(0.0, sd) ** 400 * 6
    col = [col[0] + glow * 1.0, col[1] + glow * 0.75, col[2] + glow * 0.45]
    # Clouds: streaks in the sky dome.
    if dy > 0:
        u, v = dx / (dy + 0.15), dz / (dy + 0.15)
        c = clamp((noise(u * 1.6, v * 0.6) * 0.6 + noise(u * 4.1, v * 1.7) * 0.4 - 0.52) * 3.0)
        col = mix(col, [1.0, 0.82, 0.72], c * 0.7 * clamp(dy * 4))
    # Far mountains along the horizon.
    ang = math.atan2(dx, dz)
    ridge = 0.04 + 0.11 * noise(ang * 4 + 3, 0.3) + 0.035 * noise(ang * 15, 1.7)
    if dy < ridge and dy > -0.2:
        k = clamp((ridge - dy) * 60)
        mtn = mix(hexc("#6d5f8f"), hexc("#a0889f"), clamp(1 - (ridge - dy) * 8))
        col = mix(col, [c * 0.9 for c in mtn[:3]], 0.9 * k)
    return col


MAT = {1: hexc("#7a8c4a"), 2: hexc("#8f857a"), 3: hexc("#a89580"), 4: hexc("#2f5a3c"), 5: hexc("#3e6b8e")}


def shade(t, m, ox, oy, oz, dx, dy, dz):
    x, y, z = ox + dx * t, oy + dy * t, oz + dz * t
    nx, ny, nz = normal(x, y, z)
    base = list(MAT[m][:3])
    if m == 1:
        # Grass on flat ground, earth on slopes, sand by the water.
        slope = 1 - ny
        g = fbm(x * 2.5, z * 2.5, 3)
        base = mix(mix(hexc("#86a14e"), hexc("#5d7a36"), g)[:3], hexc("#8a7358")[:3], clamp(slope * 2.5))
        base = mix(base, hexc("#d6c49a")[:3], clamp(1 - (y + 0.02) * 14))
    elif m in (2, 3):
        g = noise(x * 4 + z, y * 4)
        base = mix(base, hexc("#5d564f")[:3], g * 0.5)
        moss = clamp(ny * 1.6 - 0.6) * clamp(noise(x * 2, z * 2) * 1.6 - 0.3)
        base = mix(base, hexc("#5f7a3a")[:3], moss)
    elif m == 4:
        base = mix(base, hexc("#1e3d2a")[:3], noise(x * 5, y * 5) * 0.6)
    diff = max(0.0, nx * SUN[0] + ny * SUN[1] + nz * SUN[2])
    sh = shadow(x + nx * 0.02, y + ny * 0.02, z + nz * 0.02) if diff > 0 else 0.0
    amb = 0.25 + 0.3 * max(ny, 0.0)
    sun_c = (1.0, 0.82, 0.62)
    sky_c = (0.55, 0.65, 0.85)
    col = [base[i] * (diff * sh * sun_c[i] * 1.05 + amb * sky_c[i] * 0.55) for i in range(3)]
    if m == 5:
        # Water: reflect the sky, darker with depth, a sun glint.
        rdy = -dy
        refl = sky(dx, abs(rdy), dz)
        fres = 0.2 + 0.8 * (1 - abs(dy)) ** 5
        ripple = noise(x * 6, z * 12) - 0.5
        col = mix(hexc("#22465e")[:3], refl, fres)
        sd = dx * SUN[0] + abs(rdy) * SUN[1] + dz * SUN[2]
        g = max(0.0, sd) ** 60 * 3 * (0.6 + ripple)
        col = [col[0] + g, col[1] + g * 0.8, col[2] + g * 0.5]
    # Aerial perspective.
    fog = 1 - math.exp(-t * 0.022)
    haze = mix(hexc("#d9a98c")[:3], hexc("#a99bb8")[:3], clamp(dy * 3 + 0.5))
    return mix(col, [h * 0.8 for h in haze], fog * 0.65)


def tonemap(c):
    out = []
    for v in c[:3]:
        v = max(v, 0.0)
        v = v * (2.51 * v + 0.03) / (v * (2.43 * v + 0.59) + 0.14)
        out.append(clamp(v) ** (1 / 2.2))
    # A gentle S-curve and more saturation: a warmer, richer picture.
    lum = 0.299 * out[0] + 0.587 * out[1] + 0.114 * out[2]
    out = [clamp(lum + (v - lum) * 1.3) for v in out]
    return [v * v * (3 - 2 * v) * 0.35 + v * 0.65 for v in out]


def row(y):
    color, depth = [], []
    fov = 0.62
    for x in range(W):
        u = (2 * (x + 0.5) / W - 1) * fov * W / H
        v = -(2 * (y + 0.5) / H - 1) * fov - 0.03
        dx, dy, dz = u, v, 1.0
        l = math.sqrt(dx * dx + dy * dy + dz * dz)
        dx, dy, dz = dx / l, dy / l, dz / l
        t, m = march(*CAM, dx, dy, dz)
        if t is None:
            c = sky(dx, dy, dz)
            dist = FAR
        else:
            c = shade(t, m, *CAM, dx, dy, dz)
            dist = t * dz
        color.append(tonemap(c) + [1.0])
        # Depth: 1 near, 0 far, on an inverse scale so the foreground gets the most precision.
        dval = clamp((1 / max(dist, NEAR) - 1 / FAR) / (1 / NEAR - 1 / FAR)) ** 0.45
        depth.append(dval)
    return y, color, depth


if __name__ == "__main__":
    os.makedirs(os.path.join(OUT, "images"), exist_ok=True)
    color = [None] * H
    depth = [None] * H
    with Pool() as pool:
        for y, c, d in pool.imap_unordered(row, range(H), chunksize=4):
            color[y], depth[y] = c, d
    px = [p for r in color for p in r]
    write_png(os.path.join(OUT, "images", "diorama.png"), W, H, px, alpha=False)
    # Dilate the depth by two pixels so foreground edges carry their own color when shifted.
    dil = []
    for y in range(H):
        for x in range(W):
            best = 0.0
            for yy in range(max(0, y - 2), min(H, y + 3)):
                r = depth[yy]
                for xx in range(max(0, x - 2), min(W, x + 3)):
                    if r[xx] > best:
                        best = r[xx]
            dil.append([best, best, best, 1.0])
    write_png(os.path.join(OUT, "images", "diorama_depth.png"), W, H, dil, alpha=False)
    print("done")
