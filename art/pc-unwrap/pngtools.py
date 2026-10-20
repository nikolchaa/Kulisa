# Tiny PNG reader/writer (8-bit RGB/RGBA, no dependencies) and the post-processing the Blockbench
# renders need: remove the ground shadow, crop to a common rectangle, optionally scale down.
import struct, sys, zlib


def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    pos, chunks = 8, []
    while pos < len(data):
        n = struct.unpack(">I", data[pos:pos + 4])[0]
        t = data[pos + 4:pos + 8]
        chunks.append((t, data[pos + 8:pos + 8 + n]))
        pos += 12 + n
    ihdr = dict(chunks)[b"IHDR"]
    w, h, depth, ctype = struct.unpack(">IIBB", ihdr[:10])
    assert depth == 8 and ctype in (2, 6), (depth, ctype)
    bpp = 4 if ctype == 6 else 3
    raw = zlib.decompress(b"".join(d for t, d in chunks if t == b"IDAT"))
    stride = w * bpp
    out = bytearray(h * stride)
    prev = bytearray(stride)
    p = 0
    for y in range(h):
        f = raw[p]
        row = bytearray(raw[p + 1:p + 1 + stride])
        p += 1 + stride
        if f == 1:
            for i in range(bpp, stride):
                row[i] = (row[i] + row[i - bpp]) & 255
        elif f == 2:
            for i in range(stride):
                row[i] = (row[i] + prev[i]) & 255
        elif f == 3:
            for i in range(stride):
                a = row[i - bpp] if i >= bpp else 0
                row[i] = (row[i] + ((a + prev[i]) >> 1)) & 255
        elif f == 4:
            for i in range(stride):
                a = row[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                row[i] = (row[i] + pr) & 255
        out[y * stride:(y + 1) * stride] = row
        prev = row
    if bpp == 3:
        rgba = bytearray(w * h * 4)
        for i in range(w * h):
            rgba[i * 4:i * 4 + 3] = out[i * 3:i * 3 + 3]
            rgba[i * 4 + 3] = 255
        out = rgba
    return w, h, out


def write_png(path, w, h, rgba):
    rows = b"".join(b"\x00" + bytes(rgba[y * w * 4:(y + 1) * w * 4]) for y in range(h))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
                           + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def remove_shadow(w, h, px):
    """The ground shadow is black with partial alpha; the model's own pixels are opaque or carry their
    colour at the antialiased edge. Clear the black translucent pixels."""
    for i in range(0, w * h * 4, 4):
        if px[i] == 0 and px[i + 1] == 0 and px[i + 2] == 0 and px[i + 3] < 200:
            px[i + 3] = 0
    return px


def alpha_bbox(w, h, px):
    xs, ys = [], []
    x0, y0, x1, y1 = w, h, 0, 0
    for y in range(h):
        row = px[y * w * 4:(y + 1) * w * 4]
        for x in range(w):
            if row[x * 4 + 3] > 8:
                x0, x1 = min(x0, x), max(x1, x)
                y0, y1 = min(y0, y), max(y1, y)
    return x0, y0, x1 + 1, y1 + 1


def crop(w, h, px, box):
    x0, y0, x1, y1 = box
    out = bytearray((x1 - x0) * (y1 - y0) * 4)
    for y in range(y0, y1):
        out[(y - y0) * (x1 - x0) * 4:(y - y0 + 1) * (x1 - x0) * 4] = px[(y * w + x0) * 4:(y * w + x1) * 4]
    return x1 - x0, y1 - y0, out


def scale_down(w, h, px, f):
    """Area-average resize by factor f < 1 (premultiplied-aware)."""
    nw, nh = int(w * f), int(h * f)
    out = bytearray(nw * nh * 4)
    for y in range(nh):
        y0, y1 = y / f, (y + 1) / f
        for x in range(nw):
            x0, x1 = x / f, (x + 1) / f
            r = g = b = a = 0.0
            wsum = 0.0
            for yy in range(int(y0), min(h, int(y1) + 1)):
                wy = min(yy + 1, y1) - max(yy, y0)
                if wy <= 0:
                    continue
                for xx in range(int(x0), min(w, int(x1) + 1)):
                    wx = min(xx + 1, x1) - max(xx, x0)
                    if wx <= 0:
                        continue
                    k = wx * wy
                    i = (yy * w + xx) * 4
                    al = px[i + 3] / 255.0
                    r += px[i] * al * k
                    g += px[i + 1] * al * k
                    b += px[i + 2] * al * k
                    a += al * k
                    wsum += k
            o = (y * nw + x) * 4
            if a > 0:
                out[o], out[o + 1], out[o + 2] = int(r / a), int(g / a), int(b / a)
            out[o + 3] = int(255 * a / wsum) if wsum else 0
    return nw, nh, out


if __name__ == "__main__":
    # python pngtools.py IN_DIR OUT_DIR : cleans every *_lit.png in IN_DIR into OUT_DIR/name.png,
    # cropped to the union of the alpha boxes (so every frame keeps the same framing).
    import glob, os
    indir, outdir = sys.argv[1], sys.argv[2]
    os.makedirs(outdir, exist_ok=True)
    imgs = []
    for p in sorted(glob.glob(os.path.join(indir, "*_lit.png"))):
        w, h, px = read_png(p)
        px = remove_shadow(w, h, px)
        imgs.append((os.path.basename(p).replace("_lit.png", ""), w, h, px))
    boxes = [alpha_bbox(w, h, px) for _, w, h, px in imgs]
    box = (min(b[0] for b in boxes) - 2, min(b[1] for b in boxes) - 2, max(b[2] for b in boxes) + 2, max(b[3] for b in boxes) + 2)
    print("crop", box)
    for name, w, h, px in imgs:
        cw, ch, cp = crop(w, h, px, box)
        if len(sys.argv) > 3:
            cw, ch, cp = scale_down(cw, ch, cp, float(sys.argv[3]))
        write_png(os.path.join(outdir, name + ".png"), cw, ch, cp)
        print("wrote", name, cw, ch)
