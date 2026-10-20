# Builds the Blockbench models for templates/classic-vn: two characters with two faces each, and
# two backgrounds (a tea shop at dusk, a street at night). Pure Python, no dependencies; the
# textures are painted here (faces, hair, wood, sky). Usage: python build_vn.py OUTDIR
import base64, json, math, os, random, struct, sys, uuid, zlib

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)


def uid():
    return str(uuid.uuid4())


def png_bytes(w, h, px):
    rows = b"".join(b"\x00" + b"".join(bytes(p) for p in px[y * w:(y + 1) * w]) for y in range(h))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def clamp(v, a=0.0, b=1.0):
    return a if v < a else (b if v > b else v)


def smooth(a, b, x):
    t = clamp((x - a) / (b - a))
    return t * t * (3 - 2 * t)


def to8(c):
    return [int(clamp(v) * 255 + 0.5) for v in c]


# ---- Textures -----------------------------------------------------------------------------------
class Tex:
    def __init__(self, name, w, h, px, emissive=False):
        self.name, self.w, self.h, self.px, self.emissive = name, w, h, px, emissive


def solid(name, color, w=8, h=8, noise=0.0, seed=1, emissive=False):
    r = random.Random(seed)
    px = [to8([c + (r.random() - 0.5) * noise for c in color]) + [255] for _ in range(w * h)]
    return Tex(name, w, h, px, emissive)


def painted(name, w, h, fn, emissive=False, ss=2):
    px = []
    for y in range(h):
        for x in range(w):
            acc, a = [0.0, 0.0, 0.0], 0.0
            for sy in range(ss):
                for sx in range(ss):
                    c = fn((x + (sx + 0.5) / ss) / w, (y + (sy + 0.5) / ss) / h)
                    al = c[3] if len(c) > 3 else 1.0
                    acc = [acc[i] + c[i] * al for i in range(3)]
                    a += al
            n = ss * ss
            px.append(to8([acc[i] / a if a else 0 for i in range(3)]) + [int(clamp(a / n) * 255 + 0.5)])
    return Tex(name, w, h, px, emissive)


def mixc(a, b, t):
    return [a[i] + (b[i] - a[i]) * t for i in range(3)]


def ellipse(u, v, cx, cy, rx, ry):
    return math.hypot((u - cx) / rx, (v - cy) / ry)


def face_texture(name, skin, iris, style, lash=(0.12, 0.07, 0.1), blush=(1.0, 0.55, 0.6)):
    """style: 'neutral' or 'happy' (closed, smiling eyes and an open smile)."""
    def fn(u, v):
        c = mixc(skin, [skin[0] * 0.92, skin[1] * 0.88, skin[2] * 0.86], smooth(0.55, 1.0, v))
        # Cheek blush.
        for cx in (0.26, 0.74):
            d = ellipse(u, v, cx, 0.68, 0.11, 0.065)
            c = mixc(c, blush, 0.45 * (1 - smooth(0.4, 1.0, d)))
        for cx in (0.3, 0.7):
            if style == "happy":
                # A smiling arc (closed eye): thick upward-curved line.
                ex = (u - cx) / 0.115
                if abs(ex) < 1:
                    ey = 0.5 - 0.075 * (1 - ex * ex)
                    d = abs(v - ey)
                    c = mixc(c, lash, 1 - smooth(0.016, 0.026, d))
            else:
                d = ellipse(u, v, cx, 0.5, 0.098, 0.135)
                if d < 1.0:
                    # White, iris, pupil, highlights.
                    iris_d = ellipse(u, v, cx, 0.52, 0.075, 0.1)
                    col = [0.97, 0.97, 1.0]
                    if iris_d < 1.0:
                        t = clamp((v - 0.42) / 0.2)
                        col = mixc([iris[0] * 0.45, iris[1] * 0.45, iris[2] * 0.5], [min(1, iris[0] * 1.35), min(1, iris[1] * 1.35), min(1, iris[2] * 1.3)], t)
                        pup = ellipse(u, v, cx, 0.52, 0.036, 0.05)
                        col = mixc(col, [0.05, 0.03, 0.08], 1 - smooth(0.9, 1.0, pup))
                        # Highlights.
                        h1 = ellipse(u, v, cx - 0.025, 0.47, 0.022, 0.028)
                        h2 = ellipse(u, v, cx + 0.026, 0.56, 0.011, 0.014)
                        col = mixc(col, [1, 1, 1], max(1 - smooth(0.8, 1.0, h1), 1 - smooth(0.8, 1.0, h2)))
                    # Upper lid shadow.
                    col = mixc(col, [col[0] * 0.55, col[1] * 0.55, col[2] * 0.6], smooth(0.5, 0.0, (v - (0.38 + 0.0))) * 0.0)
                    c = mixc(c, col, 1 - smooth(0.93, 1.0, d))
                # Thick upper lash line.
                top = ellipse(u, v, cx, 0.52, 0.108, 0.14)
                if 0.0 < v < 0.5 and abs(top - 1.0) < 0.07 and v < 0.5 + 0.02:
                    c = mixc(c, lash, 1 - smooth(0.04, 0.07, abs(top - 1.0)))
        # Eyebrows.
        for cx, s in ((0.3, 1), (0.7, -1)):
            ex = (u - cx) / 0.13
            if abs(ex) < 1:
                ey = 0.3 - 0.015 * (1 - ex * ex) * (1.0 if style == "happy" else 0.4) + 0.02 * ex * s * (0.0 if style == "happy" else 0.3)
                c = mixc(c, [lash[0] * 1.5, lash[1] * 1.3, lash[2] * 1.3], 0.8 * (1 - smooth(0.006, 0.014, abs(v - ey))) * (1 - smooth(0.7, 1.0, abs(ex))))
        # Mouth.
        if style == "happy":
            mx, my = (u - 0.5) / 0.15, (v - 0.78) / 0.085
            if my > -0.1 and mx * mx + my * my < 1.0 and my > 0.0:
                c = mixc(c, [0.45, 0.1, 0.16], 1.0)
                c = mixc(c, [0.95, 0.5, 0.55], smooth(0.4, 0.9, my) * 0.7)
            d = abs(math.hypot(mx, my) - 1.0)
            if my > -0.1:
                c = mixc(c, lash, 0.9 * (1 - smooth(0.04, 0.1, d)))
        else:
            ex = (u - 0.5) / 0.07
            if abs(ex) < 1:
                ey = 0.795 + 0.012 * (ex * ex - 1) * -1
                c = mixc(c, [0.55, 0.22, 0.28], 1 - smooth(0.006, 0.012, abs(v - ey)))
        # Tiny nose.
        c = mixc(c, [skin[0] * 0.8, skin[1] * 0.72, skin[2] * 0.7], 0.5 * (1 - smooth(0.0, 0.012, math.hypot(u - 0.5, v - 0.65))))
        return c
    return painted(name, 256, 256, fn)


def hair_texture(name, color, shine=0.35):
    def fn(u, v):
        stripe = 0.5 + 0.5 * math.sin(u * 40 + math.sin(v * 7) * 2)
        c = [color[i] * (0.82 + 0.18 * stripe) for i in range(3)]
        sh = smooth(0.0, 0.5, 0.5 - abs(v - 0.35)) * 0.5 * (0.6 + 0.4 * math.sin(u * 5 + 1))
        c = [min(1, c[i] + shine * sh * (0.8 + 0.2 * (1 - color[i]))) for i in range(3)]
        return c
    return painted(name, 64, 64, fn)


def cloth_texture(name, color, accent=None, pattern="plain"):
    def fn(u, v):
        c = list(color)
        if pattern == "stripes":
            if int(v * 10) % 2 == 0:
                c = [color[i] * 0.92 for i in range(3)]
        if pattern == "pleats":
            c = [color[i] * (0.88 + 0.12 * (0.5 + 0.5 * math.sin(u * 60))) for i in range(3)]
        if pattern == "hem" and accent:
            if v > 0.86:
                c = list(accent)
        if pattern == "collar" and accent:
            if v < 0.14:
                c = list(accent)
        return c
    return painted(name, 32, 32, fn, ss=1)


# ---- Model builder ---------------------------------------------------------------------------------
class Model:
    def __init__(self, name):
        self.name = name
        self.textures = []
        self.tex_id = {}
        self.cubes = []
        self.groups = []

    def tex(self, t):
        if t.name not in self.tex_id:
            self.tex_id[t.name] = len(self.textures)
            self.textures.append(t)
        return self.tex_id[t.name]

    def box(self, name, a, b, tex, rot=None, origin=None, uv=None):
        """tex: a Tex for all faces, or a dict side -> Tex with 'all'. uv: dict side -> [u0,v0,u1,v1] in 0..16."""
        faces = {}
        for side in ("north", "east", "south", "west", "up", "down"):
            t = tex.get(side, tex["all"]) if isinstance(tex, dict) else tex
            faces[side] = {"uv": (uv or {}).get(side, [0, 0, 16, 16]), "texture": self.tex(t)}
        c = {"name": name, "box_uv": False, "rescale": False, "locked": False, "light_emission": 0, "render_order": "default",
             "allow_mirror_modeling": True, "from": list(a), "to": list(b), "autouv": 0, "color": 0,
             "origin": list(origin or [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2]),
             "rotation": list(rot or [0, 0, 0]), "faces": faces, "type": "cube", "uuid": uid()}
        self.cubes.append(c)
        return c

    def write(self, extra_anchors=None):
        anchors = []
        clear = Tex("clear", 4, 4, [[0, 0, 0, 0]] * 16)
        for i, (x, y, z) in enumerate(extra_anchors or []):
            anchors.append(self.box("anchor%d" % i, (x, y, z), (x + 0.05, y + 0.05, z + 0.05), clear))
        textures = []
        for i, t in enumerate(self.textures):
            textures.append({"path": "", "name": t.name + ".png", "folder": "", "namespace": "", "id": str(i), "width": t.w,
                             "height": t.h, "uv_width": 16, "uv_height": 16, "particle": False, "use_as_default": False,
                             "layers_enabled": False, "sync_to_project": "",
                             "render_mode": "emissive" if t.emissive else "default", "render_sides": "auto", "frame_time": 1,
                             "frame_order_type": "loop", "frame_order": "", "frame_interpolate": False, "visible": True,
                             "internal": True, "saved": False, "uuid": uid(),
                             "source": "data:image/png;base64," + base64.b64encode(png_bytes(t.w, t.h, t.px)).decode()})
        gid = uid()
        model = {"meta": {"format_version": "5.0", "model_format": "free", "box_uv": False}, "name": self.name,
                 "model_identifier": "", "visible_box": [1, 1, 0], "variable_placeholders": "",
                 "variable_placeholder_buttons": [], "timeline_setups": [], "unhandled_root_fields": {},
                 "resolution": {"width": 16, "height": 16}, "elements": self.cubes,
                 "groups": [{"name": "scene", "uuid": gid, "origin": [0, 0, 0], "rotation": [0, 0, 0], "export": True,
                             "visibility": True, "mirror_uv": False, "reset": False, "color": 0, "autouv": 0, "shade": True,
                             "locked": False}],
                 "outliner": [{"uuid": gid, "isOpen": True, "children": [c["uuid"] for c in self.cubes]}],
                 "textures": textures}
        path = os.path.join(OUT, self.name + ".bbmodel")
        json.dump(model, open(path, "w"))
        print("wrote", path, len(self.cubes), "cubes", len(self.textures), "textures")


# ---- Characters ------------------------------------------------------------------------------------
def character(name, expression, skin, iris, hair, hair2, top, top2, bottom, shoes, ribbon, kind):
    m = Model(f"{name}_{expression}")
    face = face_texture(f"{name}_{expression}_face", skin, iris, expression)
    skin_t = solid("skin_" + name, skin, noise=0.0)
    hair_t = hair_texture("hair_" + name, hair)
    hair_d = hair_texture("hair2_" + name, hair2, 0.2)
    top_t = cloth_texture("top_" + name, top, top2, "collar")
    top_s = cloth_texture("topsleeve_" + name, top, top2, "plain")
    bottom_t = cloth_texture("bottom_" + name, bottom, None, "pleats" if kind == "girl" else "plain")
    shoe_t = solid("shoe_" + name, shoes)
    rib_t = solid("rib_" + name, ribbon)
    white = solid("white", (0.96, 0.96, 0.98))
    mouthless = skin_t
    # Head: a big cube with the face on its front.
    m.box("head", (-8, 29, -7), (8, 45, 7), {"all": skin_t, "north": face}, uv={})
    # Hair: back, top and fringe, side locks.
    m.box("hair_back", (-8.8, 27, 3), (8.8, 46, 8.6), hair_t)
    m.box("hair_top", (-8.8, 42.5, -7.8), (8.8, 46.6, 8.2), hair_t)
    m.box("fringe", (-8.2, 38.5, -8.1), (8.2, 43.5, -5.8), {"all": hair_t, "north": hair_d})
    m.box("fringe_l", (-8.8, 31, -8.0), (-6.2, 43, -3.5), hair_t)
    m.box("fringe_r", (6.2, 31, -8.0), (8.8, 43, -3.5), hair_t)
    if kind == "girl":
        m.box("tail_l", (-13.2, 24, -2), (-9, 40, 3), hair_d)
        m.box("tail_r", (9, 24, -2), (13.2, 40, 3), hair_d)
        m.box("ribbon_l", (-11.4, 38, -2.5), (-8.8, 41.5, 3.5), rib_t)
        m.box("ribbon_r", (8.8, 38, -2.5), (11.4, 41.5, 3.5), rib_t)
    else:
        m.box("spike", (-4, 46, -4), (5, 48.6, 4), hair_t, rot=[0, 0, 12])
    # Neck, torso, arms, legs.
    m.box("neck", (-2.6, 26.6, -2.2), (2.6, 29.4, 2.4), skin_t)
    m.box("torso", (-6.6, 14, -3.8), (6.6, 27.4, 3.8), {"all": top_s, "north": top_t})
    if kind == "girl":
        m.box("skirt", (-8.4, 8.5, -5.2), (8.4, 16.5, 5.2), bottom_t)
        m.box("collar_bow", (-2.4, 23.5, -4.4), (2.4, 26.6, -3.7), rib_t)
    else:
        m.box("jacket_open", (-6.9, 12.4, -4.1), (6.9, 17, 4.1), top_s)
    m.box("arm_l", (-10.4, 14.8, -2.6), (-6.4, 27, 2.6), top_s)
    m.box("arm_r", (6.4, 14.8, -2.6), (10.4, 27, 2.6), top_s)
    m.box("hand_l", (-10.1, 11.4, -2.2), (-6.7, 15.2, 2.2), skin_t)
    m.box("hand_r", (6.7, 11.4, -2.2), (10.1, 15.2, 2.2), skin_t)
    if kind == "girl":
        m.box("leg_l", (-5.2, 2.2, -2.8), (-0.6, 9, 2.8), white)  # socks
        m.box("leg_r", (0.6, 2.2, -2.8), (5.2, 9, 2.8), white)
    else:
        m.box("leg_l", (-5.4, 1.5, -3), (-0.4, 12.6, 3), bottom_t)
        m.box("leg_r", (0.4, 1.5, -3), (5.4, 12.6, 3), bottom_t)
    m.box("shoe_l", (-5.8, 0, -4.8), (-0.2, 2.6, 3), shoe_t)
    m.box("shoe_r", (0.2, 0, -4.8), (5.8, 2.6, 3), shoe_t)
    m.write([(-16, -1, -9), (16, 52, 9)])


mila = dict(skin=(1.0, 0.86, 0.78), iris=(0.85, 0.35, 0.55), hair=(0.28, 0.14, 0.2), hair2=(0.36, 0.2, 0.28),
            top=(0.93, 0.55, 0.72), top2=(1.0, 0.95, 0.97), bottom=(0.32, 0.26, 0.45), shoes=(0.15, 0.1, 0.14),
            ribbon=(0.98, 0.78, 0.4), kind="girl")
ren = dict(skin=(0.98, 0.82, 0.7), iris=(0.3, 0.55, 0.75), hair=(0.22, 0.3, 0.42), hair2=(0.3, 0.4, 0.54),
           top=(0.35, 0.62, 0.74), top2=(0.9, 0.95, 0.98), bottom=(0.22, 0.24, 0.32), shoes=(0.12, 0.12, 0.16),
           ribbon=(0.9, 0.5, 0.35), kind="boy")
for expr in ("neutral", "happy"):
    character("mila", expr, **mila)
    character("ren", expr, **ren)
