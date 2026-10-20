# Paints the mountain ranges of demos/parallax as data textures. Each range is a heightfield
# (ridged multifractal noise, periodic in x so the strip tiles) rendered column by column from
# front to back, the way old voxel-terrain renderers did it. Channels: red = lightness under a sun
# on the left, green = under a sun on the right (both with soft cast shadows and the rock's
# albedo), blue = snow, alpha = coverage. shaders/mountains.wgsl colors them for the time of day.
# Usage: python art/parallax/mountains.py demos/parallax [preview.png]
import math, os, sys
from multiprocessing import Pool

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "common"))
from canvas import clamp, smooth, write_png  # noqa: E402

OUT = sys.argv[1] if len(sys.argv) > 1 else "demos/parallax"
W = 2048

# name, image height, front row's screen y, depth rows, world units per row, screen rise per row,
# amplitude, ridge cells across the width, octaves, sharpness, snow line, albedo, tree fuzz, seed
RANGES = [
    dict(name="mountains_far", h=720, y0=700, rows=760, dz=1.1, rise=0.27, amp=440, cells=4, oct=8, sharp=0.7,
         snow=245, albedo=0.62, fuzz=0, seed=5),
    dict(name="mountains_mid", h=560, y0=545, rows=640, dz=1.2, rise=0.25, amp=300, cells=6, oct=8, sharp=0.6,
         snow=255, albedo=0.55, fuzz=0, seed=17),
    dict(name="mountains_near", h=400, y0=390, rows=480, dz=1.3, rise=0.25, amp=170, cells=8, oct=6, sharp=0.4,
         snow=10000, albedo=0.42, fuzz=4, seed=29),
]


def hsh(x, y, s):
    n = (x * 374761393 + y * 668265263 + s * 1442695041) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 65535.0


def pnoise(x, z, period, s):
    """Value noise, periodic in x with `period` cells."""
    xi, zi = math.floor(x), math.floor(z)
    fx, fz = x - xi, z - zi
    fx = fx * fx * (3 - 2 * fx)
    fz = fz * fz * (3 - 2 * fz)
    x0, x1 = xi % period, (xi + 1) % period
    a, b = hsh(x0, zi, s), hsh(x1, zi, s)
    c, d = hsh(x0, zi + 1, s), hsh(x1, zi + 1, s)
    return a + (b - a) * fx + (c - a) * fz + (a - b - c + d) * fx * fz


def ridged(x, z, period, s, octaves, sharp):
    """Musgrave's ridged multifractal: sharp crests, smoother valleys."""
    total, amp, weight = 0.0, 1.0, 1.0
    for o in range(octaves):
        n = pnoise(x, z, period, s + o * 131)
        r = 1.0 - abs(n * 2.0 - 1.0)
        r = r ** (1.0 + sharp)
        r *= weight
        weight = clamp(r * 2.2)
        total += r * amp
        x, z, period, amp = x * 2.0, z * 2.0, period * 2, amp * 0.5
    return total


def lights():
    out = []
    for sx in (-1.0, 1.0):
        v = (0.9 * sx, 0.34, -0.3)
        l = math.sqrt(sum(c * c for c in v))
        out.append((v[0] / l, v[1] / l, v[2] / l))
    return out


def render(spec):
    rows, dz, amp = spec["rows"], spec["dz"], spec["amp"]
    cells, seed = spec["cells"], spec["seed"]
    cw = W / cells
    # Heights on the grid (row 0 is the front).
    hf = []
    hmax = 0.0
    for r in range(rows):
        zw = r * dz
        zn = r / (rows - 1)
        env = 0.12 + 0.88 * smooth(0.0, 0.8, zn)
        row = []
        for x in range(W):
            big = 0.35 + 0.75 * pnoise(x / (W / 4), 7.3, 4, seed + 999)
            h = ridged(x / cw, zw / cw, cells, seed, spec["oct"], spec["sharp"]) * 0.62
            h = amp * env * big * h
            if spec["fuzz"]:
                h += spec["fuzz"] * pnoise(x / 4.0, zw / 4.0, W // 4, seed + 7) * env
            row.append(h)
            hmax = max(hmax, h)
        hf.append(row)
    lts = lights()

    def shade(x, r):
        row = hf[r]
        h = row[x]
        dhdx = (row[(x + 1) % W] - row[(x - 1) % W]) * 0.5
        r0, r1 = max(r - 1, 0), min(r + 1, rows - 1)
        dhdz = (hf[r1][x] - hf[r0][x]) / ((r1 - r0) * dz)
        nx, ny, nz = -dhdx, 1.0, -dhdz
        nl = 1.0 / math.sqrt(nx * nx + ny * ny + nz * nz)
        nx, ny, nz = nx * nl, ny * nl, nz * nl
        # Albedo: rock with strata, lighter scree on gentle slopes.
        strata = pnoise(x / 64.0, h / 9.0, W // 64, seed + 3)
        alb = spec["albedo"] * (0.8 + 0.3 * strata) * (0.9 + 0.2 * ny)
        out = []
        for lx, ly, lz in lts:
            diff = max(0.0, nx * lx + ny * ly + nz * lz)
            sh = 1.0
            if diff > 0:
                t = 3.0
                zw = r * dz
                while t < 420:
                    px = x + lx * t
                    pz = zw + lz * t
                    ph = h + ly * t
                    if pz < 0 or ph > hmax:
                        break
                    th = hf[min(rows - 1, int(pz / dz))][int(px) % W]
                    sh = min(sh, 6.0 * (ph - th) / t)
                    if sh <= 0:
                        sh = 0.0
                        break
                    t += 2.0 + t * 0.04
            sh = clamp(sh)
            out.append(alb * (0.12 + 0.88 * diff * sh))
        # Snow: high up, on slopes gentle enough to hold it, ragged at the line.
        line = spec["snow"] + 70 * (pnoise(x / 64.0, r * dz / 64.0, W // 64, seed + 11) - 0.5)
        snow = smooth(line - 30, line + 30, h) * smooth(0.3, 0.55, ny)
        out.append(snow)
        return out

    H, y0, rise = spec["h"], spec["y0"], spec["rise"]
    px = [[0.0, 0.0, 0.0, 0.0] for _ in range(W * H)]
    for x in range(W):
        top = float(H)
        last = None
        for r in range(rows):
            y = y0 - r * rise - hf[r][x]
            if r == 0:
                y = min(y, float(y0))
                c = shade(x, 0)
                for py in range(int(math.ceil(y)), H):
                    px[py * W + x] = c + [1.0]
                top, last = y, c
                continue
            if y < top:
                c = shade(x, r)
                # A steep face spreads one step over several pixels: blend from the previous step.
                span = top - y
                for py in range(max(0, int(math.ceil(y))), int(math.ceil(top))):
                    t = clamp((top - py) / span) if span > 3.0 else 1.0
                    px[py * W + x] = [last[i] + (c[i] - last[i]) * t for i in range(3)] + [1.0]
                top, last = y, c
        # The topmost pixel is partly covered.
        fy = math.floor(top)
        if 0 <= fy < H and last is not None and fy < math.ceil(top):
            px[fy * W + x] = last + [math.ceil(top) - top]
    return spec["name"], H, px


def job(spec):
    name, H, px = render(spec)
    path = os.path.join(OUT, "images", name + ".png")
    write_png(path, W, H, px, up=True)
    return name, H, px


if __name__ == "__main__":
    os.makedirs(os.path.join(OUT, "images"), exist_ok=True)
    with Pool(3) as pool:
        done = pool.map(job, RANGES)
    for name, _, _ in done:
        print("painted", name)
    if len(sys.argv) > 2:
        # A preview: the three ranges stacked, colored the way the demo colors them at dusk.
        prev_h = 820
        img = [[0.55, 0.45, 0.6, 1.0] for _ in range(W * prev_h)]
        tints = [((0.95, 0.62, 0.55), (0.32, 0.27, 0.45), 0.35), ((0.75, 0.48, 0.5), (0.22, 0.18, 0.32), 0.2),
                 ((0.42, 0.3, 0.36), (0.13, 0.1, 0.18), 0.05)]
        offs = [0, 200, 420]
        for (name, H, px), (lit, shd, haze), oy in zip(done, tints, offs):
            for y in range(H):
                yy = y + oy
                if yy >= prev_h:
                    break
                for x in range(W):
                    p = px[y * W + x]
                    if p[3] <= 0:
                        continue
                    l = p[0] * 0.3 + p[1] * 0.7
                    c = [shd[i] + (lit[i] - shd[i]) * clamp(l * 1.6) for i in range(3)]
                    s = [0.75 + 0.25 * clamp(l * 1.6)] * 3
                    c = [c[i] + (s[i] - c[i]) * p[2] for i in range(3)]
                    c = [c[i] + (0.55 - c[i]) * haze for i in range(3)]
                    q = img[yy * W + x]
                    for i in range(3):
                        q[i] = q[i] + (c[i] - q[i]) * p[3]
        write_png(sys.argv[2], W, prev_h, img, alpha=False)
        print("preview", sys.argv[2])
