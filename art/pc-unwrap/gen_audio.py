import math, random, struct, wave, os, sys
SR = 22050
OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)

def write(name, samples, peak=0.85):
    m = max(1e-9, max(abs(s) for s in samples))
    g = peak / m
    data = b"".join(struct.pack("<h", int(max(-1, min(1, s * g)) * 32767)) for s in samples)
    with wave.open(os.path.join(OUT, name + ".wav"), "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR); w.writeframes(data)

def noise(n, rng): return [rng.uniform(-1, 1) for _ in range(n)]

def lp(x, fc):
    a = 1 - math.exp(-2 * math.pi * fc / SR); y = 0; out = []
    for v in x:
        y += a * (v - y); out.append(y)
    return out

def hp(x, fc):
    l = lp(x, fc); return [a - b for a, b in zip(x, l)]

def bp(x, lo, hi): return lp(hp(x, lo), hi)

def fade(x, a=0.004, b=0.01):
    n = len(x); na = int(a * SR); nb = int(b * SR)
    return [v * min(1, i / max(1, na), (n - i) / max(1, nb)) for i, v in enumerate(x)]

def crinkle(seed, dur, density, lo=900, hi=7500, bright=1.0):
    rng = random.Random(seed); n = int(dur * SR); out = [0.0] * n
    t = 0.0
    while t < dur:
        t += rng.expovariate(density)
        i0 = int(t * SR)
        if i0 >= n: break
        amp = rng.random() ** 2.2
        tau = rng.uniform(0.0008, 0.004)
        f = rng.uniform(lo, hi) * bright
        ph = rng.uniform(0, 6.28)
        for k in range(int(tau * 6 * SR)):
            if i0 + k >= n: break
            tt = k / SR
            out[i0 + k] += amp * math.exp(-tt / tau) * (0.55 * math.sin(2 * math.pi * f * tt + ph) + 0.45 * rng.uniform(-1, 1))
    env = [math.sin(math.pi * min(1, i / n)) ** 0.5 for i in range(n)]
    out = [o * e for o, e in zip(out, env)]
    return fade(hp(out, 500))

def rustle(seed, dur):
    rng = random.Random(seed); n = int(dur * SR)
    bed = bp(noise(n, rng), 300, 3800)
    # slow bumpy envelope
    knots = [rng.uniform(0.2, 1) for _ in range(int(dur * 14) + 3)]
    env = []
    for i in range(n):
        p = i / SR * 14; a = int(p); fr = p - a
        env.append((knots[a] * (1 - fr) + knots[a + 1] * fr) ** 2)
    pops = crinkle(seed + 99, dur, 90, 1200, 5000)
    out = [b * e * 0.6 + c * 0.9 for b, e, c in zip(bed, env, pops)]
    return fade(out, 0.01, 0.06)

def slip(seed, dur=0.09):
    rng = random.Random(seed); n = int(dur * SR); out = []
    for i in range(n):
        t = i / SR
        env = math.exp(-t / 0.018)
        out.append(env * rng.uniform(-1, 1))
    out = hp(out, 2200)
    # tiny click in front
    for i in range(12): out[i] += (1 - i / 12) * 0.8 * (1 if i % 2 else -1)
    return fade(out, 0.0005, 0.02)

def tear(seed, dur=0.42):
    rng = random.Random(seed); n = int(dur * SR); out = []
    for i in range(n):
        t = i / SR; p = t / dur
        rough = 0.5 + 0.5 * math.sin(2 * math.pi * (70 + 40 * p) * t + 3 * math.sin(2 * math.pi * 13 * t))
        env = (min(1, t / 0.02)) * (1 - p) ** 0.7
        out.append(env * rough * rng.uniform(-1, 1))
    out = bp(out, 1200, 9000)
    return fade(out, 0.003, 0.05)

def whoosh(seed, dur=0.55):
    rng = random.Random(seed); n = int(dur * SR); x = noise(n, rng); out = []
    # swept band-pass by mixing two low-passes
    lo = lp(x, 500); hi = lp(x, 3200)
    for i in range(n):
        p = i / n; env = math.sin(math.pi * p) ** 2
        mix = 0.5 - 0.5 * math.cos(math.pi * p)
        out.append(env * (lo[i] * (1 - mix) + (hi[i] - lo[i]) * mix * 1.3))
    return fade(out, 0.02, 0.1)

def squeak(seed, dur=0.17):
    rng = random.Random(seed); n = int(dur * SR); out = []; ph = 0
    base = rng.uniform(1100, 1500)
    for i in range(n):
        t = i / SR; p = t / dur
        f = base * (1 + 0.12 * math.sin(2 * math.pi * 9 * t) + 0.25 * p)
        ph += 2 * math.pi * f / SR
        env = math.sin(math.pi * p) ** 1.5
        tone = math.sin(ph) + 0.35 * math.sin(2 * ph)
        out.append(env * (0.5 * tone + 0.5 * rng.uniform(-1, 1) * 0.6))
    return fade(bp(out, 700, 6500), 0.01, 0.04)

def rub(seed, dur=0.26):
    rng = random.Random(seed); n = int(dur * SR)
    x = bp(noise(n, rng), 200, 2600)
    out = [v * (math.sin(math.pi * i / n) ** 1.2) * (0.6 + 0.4 * math.sin(2 * math.pi * 31 * i / SR)) for i, v in enumerate(x)]
    sq = squeak(seed + 5, dur)
    return fade([a + 0.35 * b for a, b in zip(out, sq)], 0.02, 0.07)

def pop(seed, dur=0.07):
    rng = random.Random(seed); n = int(dur * SR); out = []
    for i in range(n):
        t = i / SR
        f = 900 * math.exp(-t / 0.012) + 200
        out.append(math.exp(-t / 0.014) * math.sin(2 * math.pi * f * t) + 0.3 * math.exp(-t / 0.002) * rng.uniform(-1, 1))
    return fade(out, 0.0003, 0.02)

def tap(seed, dur=0.08):
    rng = random.Random(seed); n = int(dur * SR); out = []
    for i in range(n):
        t = i / SR
        out.append(math.exp(-t / 0.02) * math.sin(2 * math.pi * 150 * t) + 0.5 * math.exp(-t / 0.003) * rng.uniform(-1, 1))
    return fade(lp(out, 5000), 0.0004, 0.03)

def chime(dur=1.1):
    n = int(dur * SR); out = []
    for i in range(n):
        t = i / SR
        a = math.exp(-t / 0.35) * math.sin(2 * math.pi * 880 * t)
        b = math.exp(-(t - 0.12) / 0.45) * math.sin(2 * math.pi * 1318.5 * t) if t > 0.12 else 0
        c = 0.4 * math.exp(-(t - 0.24) / 0.55) * math.sin(2 * math.pi * 1760 * t) if t > 0.24 else 0
        out.append(0.6 * a + 0.5 * b + 0.3 * c)
    return fade(out, 0.002, 0.15)

for i in range(6):
    write(f"crinkle{i+1}", crinkle(10 + i, 0.10 + 0.035 * i, 380 + 70 * i), 0.8)
for i in range(3):
    write(f"rustle{i+1}", rustle(40 + i, 0.30 + 0.05 * i), 0.8)
for i in range(3):
    write(f"slip{i+1}", slip(70 + i, 0.07 + 0.015 * i), 0.8)
write("tear", tear(5), 0.85)
write("whoosh", whoosh(6), 0.7)
for i in range(2):
    write(f"squeak{i+1}", squeak(80 + i), 0.55)
for i in range(2):
    write(f"rub{i+1}", rub(90 + i), 0.7)
for i in range(3):
    write(f"pop{i+1}", pop(100 + i, 0.06 + 0.01 * i), 0.8)
write("tap", tap(1), 0.8)
write("chime", chime(), 0.6)
