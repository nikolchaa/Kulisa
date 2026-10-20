# Builds the Blockbench models (.bbmodel JSON) for the unboxing scene of demos/pc-unwrap.
# Usage: python build_models.py OUTDIR  (textures are read from ./textures and embedded)
import base64, json, os, sys, uuid

HERE = os.path.dirname(os.path.abspath(__file__))
TEX_DIR = os.path.join(HERE, "textures")
OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)


def uid():
    return str(uuid.uuid4())


TEXTURES = ["cardboard", "cardboard_inner", "box_front", "box_side", "foam", "case_dark", "case_front", "clear",
            "neon_purple", "neon_blue", "neon_cyan"]
TEX_ID = {n: i for i, n in enumerate(TEXTURES)}


def texture_entries():
    out = []
    for i, name in enumerate(TEXTURES):
        data = open(os.path.join(TEX_DIR, name + ".png"), "rb").read()
        w, h = int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")
        out.append({
            "path": "", "name": name + ".png", "folder": "", "namespace": "", "id": str(i),
            "width": w, "height": h, "uv_width": 16, "uv_height": 16, "particle": False, "use_as_default": False,
            "layers_enabled": False, "sync_to_project": "",
            "render_mode": "emissive" if name.startswith("neon") else "default",
            "render_sides": "auto", "frame_time": 1, "frame_order_type": "loop", "frame_order": "",
            "frame_interpolate": False, "visible": True, "internal": True, "saved": False, "uuid": uid(),
            "source": "data:image/png;base64," + base64.b64encode(data).decode(),
        })
    return out


def cube(name, a, b, tex):
    """tex: a texture name for all faces, or a dict with 'all' and per-side overrides."""
    faces = {}
    for side in ("north", "east", "south", "west", "up", "down"):
        t = tex.get(side, tex.get("all")) if isinstance(tex, dict) else tex
        faces[side] = {"uv": [0, 0, 16, 16], "texture": TEX_ID[t]}
    return {"name": name, "box_uv": False, "rescale": False, "locked": False, "light_emission": 0,
            "render_order": "default", "allow_mirror_modeling": True, "from": list(a), "to": list(b), "autouv": 0,
            "color": 0, "origin": [0, 0, 0], "faces": faces, "type": "cube", "uuid": uid()}


def kf(rot, t):
    return {"channel": "rotation", "data_points": [{"x": "%.4f" % rot[0], "y": "%.4f" % rot[1], "z": "%.4f" % rot[2]}],
            "uuid": uid(), "time": round(t, 4), "color": -1, "interpolation": "linear"}


def build(name, parts, root_rot=(-52, 0, 0)):
    """parts: dicts with 'cubes' and optionally 'group': (name, origin, rot_closed, rot_open, t0, t1)."""
    elements, children, animators, groups = [], [], {}, []
    for part in parts:
        g = part.get("group")
        if g:
            gname, origin, closed, opened, t0, t1 = g
            gid = uid()
            kids = []
            for c in part["cubes"]:
                elements.append(c)
                kids.append(c["uuid"])
            # The group rests unrotated (flaps flat, pointing outward); the animation swings them.
            groups.append({"name": gname, "uuid": gid, "origin": list(origin), "rotation": [0, 0, 0], "export": True,
                           "visibility": True, "mirror_uv": False, "reset": False, "color": 0, "autouv": 0,
                           "shade": True, "locked": False})
            children.append({"uuid": gid, "isOpen": True, "children": kids})
            keys = [kf(closed, 0)]
            steps = 8
            for k in range(steps + 1):
                f = k / steps
                e = f * f * (3 - 2 * f)
                keys.append(kf([closed[i] + (opened[i] - closed[i]) * e for i in range(3)], t0 + (t1 - t0) * f))
            animators[gid] = {"name": gname, "type": "bone", "keyframes": keys}
        else:
            for c in part["cubes"]:
                elements.append(c)
                children.append(c["uuid"])
    # Invisible markers keep the camera framing identical in every layer.
    corners = [(-56, -2, -48), (56, -2, -48), (-56, -2, 48), (56, -2, 48), (-56, 46, -48), (56, 46, -48),
               (-56, 46, 48), (56, 46, 48)]
    for i, (x, y, z) in enumerate(corners):
        c = cube("anchor%d" % i, (x, y, z), (x + 0.05, y + 0.05, z + 0.05), "clear")
        elements.append(c)
        children.append(c["uuid"])
    root_id = uid()
    groups.append({"name": "scene", "uuid": root_id, "origin": [0, 15, 0], "rotation": list(root_rot), "export": True,
                   "visibility": True, "mirror_uv": False, "reset": False, "color": 0, "autouv": 0, "shade": True,
                   "locked": False})
    root = {"uuid": root_id, "isOpen": True, "children": children}
    model = {
        "meta": {"format_version": "5.0", "model_format": "free", "box_uv": False},
        "name": name, "model_identifier": "", "visible_box": [1, 1, 0], "variable_placeholders": "",
        "variable_placeholder_buttons": [], "timeline_setups": [], "unhandled_root_fields": {}, "groups": groups,
        "resolution": {"width": 16, "height": 16}, "elements": elements, "outliner": [root],
        "textures": texture_entries(),
        "animations": [{"uuid": uid(), "name": "open", "loop": "once", "override": False, "length": 1.0,
                        "snapping": 24, "selected": False, "anim_time_update": "", "blend_weight": "",
                        "start_delay": "", "loop_delay": "", "animators": animators}],
    }
    path = os.path.join(OUT, name + ".bbmodel")
    json.dump(model, open(path, "w"))
    print("wrote", path, len(elements), "elements")


BW, BD, BH, T = 30.8, 22.8, 30, 0.8


def box_back():
    return [
        {"cubes": [cube("floor", (-BW, 0, -BD), (BW, T, BD), "cardboard_inner")]},
        {"cubes": [cube("wall_back", (-BW, T, BD - T), (BW, BH, BD), {"all": "cardboard", "north": "cardboard_inner", "south": "box_side"})]},
        {"cubes": [cube("wall_left", (-BW, T, -BD + T), (-BW + T, BH, BD - T), {"all": "cardboard", "east": "cardboard_inner", "west": "box_side"})]},
        {"cubes": [cube("wall_right", (BW - T, T, -BD + T), (BW, BH, BD - T), {"all": "cardboard", "west": "cardboard_inner", "east": "box_side"})]},
        {"group": ("flap_back", (0, BH, BD), (-180, 0, 0), (-72, 0, 0), 0.0, 0.5),
         "cubes": [cube("flap_back_c", (-BW, BH, BD), (BW, BH + T, BD + 22), {"all": "cardboard", "up": "cardboard_inner"})]},
        {"group": ("flap_left", (-BW, BH - 0.8, 0), (0, 0, -180), (0, 0, -72), 0.3, 0.85),
         "cubes": [cube("flap_left_c", (-BW - 22, BH - 0.8, -BD + T), (-BW, BH, BD - T), {"all": "cardboard", "up": "cardboard_inner"})]},
        {"group": ("flap_right", (BW, BH - 0.8, 0), (0, 0, 180), (0, 0, 72), 0.3, 0.85),
         "cubes": [cube("flap_right_c", (BW, BH - 0.8, -BD + T), (BW + 22, BH, BD - T), {"all": "cardboard", "up": "cardboard_inner"})]},
    ]


def box_front():
    return [
        {"cubes": [cube("wall_front", (-BW, T, -BD), (BW, BH, -BD + T), {"all": "cardboard", "north": "box_front", "south": "cardboard_inner"})]},
        {"group": ("flap_front", (0, BH, -BD), (180, 0, 0), (72, 0, 0), 0.0, 0.5),
         "cubes": [cube("flap_front_c", (-BW, BH, -BD - 22), (BW, BH + T, -BD), {"all": "cardboard", "up": "cardboard_inner"})]},
    ]


def foam_left():
    return [{"cubes": [cube("foam_l_bar", (-29, 4.5, -21), (-20.5, 29, 21), "foam"),
                       cube("foam_l_arm", (-29, 17, -21), (-10, 29, -12), "foam")]}]


def foam_right():
    return [{"cubes": [cube("foam_r_bar", (20.5, 4.5, -21), (29, 29, 21), "foam"),
                       cube("foam_r_arm", (10, 17, -21), (29, 29, -12), "foam")]}]


def foam_floor():
    return [{"cubes": [cube("foam_floor", (-29, T, -21), (29, 4.5, 21), "foam")]}]


def case():
    return [{"cubes": [cube("case", (-20, 4.5, -13), (20, 27, 14), {"all": "case_dark", "north": "case_front"}),
                       cube("trim_top", (-20, 26.6, -13.1), (20, 27.2, -12.9), "neon_purple"),
                       cube("trim_bottom", (-20, 4.5, -13.1), (20, 5.1, -12.9), "neon_blue")]}]


build("unbox_all", box_back() + box_front() + foam_floor() + foam_left() + foam_right() + case())
build("unbox_back_nofoam", box_back() + case() + foam_floor())
build("unbox_back_empty", box_back() + foam_floor())
build("unbox_front", box_front())
build("unbox_foam_left", foam_left())
build("unbox_foam_right", foam_right())
build("unbox_case", case())
