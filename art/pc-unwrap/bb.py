# A small writer for Blockbench .bbmodel files (free format): textures embedded as PNG data, cubes
# with per-face textures and uvs, element and group rotations, nested groups and rotation
# keyframes. Used by build_models.py and pc.py.
import base64, json, os, uuid


def uid():
    return str(uuid.uuid4())


SIDES = ("north", "east", "south", "west", "up", "down")


class Model:
    def __init__(self, name):
        self.name = name
        self.textures = []  # (name, png bytes, emissive)
        self.tex_id = {}
        self.elements = []
        self.groups = []
        self.root = []  # outliner entries at the top level
        self.stack = [self.root]
        self.animators = {}

    def texture(self, name, data, emissive=False):
        """Adds a texture (PNG bytes, or a path) once; returns its index."""
        if name in self.tex_id:
            return self.tex_id[name]
        if isinstance(data, str):
            data = open(data, "rb").read()
        self.tex_id[name] = len(self.textures)
        self.textures.append((name, data, emissive))
        return self.tex_id[name]

    def full_uv(self, name):
        """The uv rectangle of a whole texture. The renderer measures uvs in units of the texture's
        width / 16 both ways, so a texture that is not square is 16 wide and 16 * h / w tall."""
        data = self.textures[self.tex_id[name]][1]
        w, h = int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")
        return [0, 0, 16, 16 * h / w]

    def cube(self, name, a, b, faces, rotation=None, origin=None, inflate=0):
        """faces: a texture name for every side, or {side or 'all': name or (name, uv)}; a side
        mapped to None is left out. uv is [u0, v0, u1, v1] (see full_uv)."""
        fs = {}
        for side in SIDES:
            if isinstance(faces, dict):
                t = faces.get(side, faces.get("all"))
            else:
                t = faces
            if t is None:
                continue
            tex, uv = (t, self.full_uv(t)) if isinstance(t, str) else t
            fs[side] = {"uv": list(uv), "texture": self.tex_id[tex]}
        el = {"name": name, "box_uv": False, "rescale": False, "locked": False, "light_emission": 0,
              "render_order": "default", "allow_mirror_modeling": True, "from": [float(v) for v in a],
              "to": [float(v) for v in b], "autouv": 0, "color": 0, "inflate": inflate,
              "origin": list(origin or [(a[i] + b[i]) / 2 for i in range(3)]), "faces": fs, "type": "cube",
              "uuid": uid()}
        if rotation:
            el["rotation"] = list(rotation)
        self.elements.append(el)
        self.stack[-1].append(el["uuid"])
        return el

    def group(self, name, origin=(0, 0, 0), rotation=(0, 0, 0)):
        model = self

        class G:
            def __enter__(self):
                gid = uid()
                model.groups.append({"name": name, "uuid": gid, "origin": list(origin), "rotation": list(rotation),
                                     "export": True, "visibility": True, "mirror_uv": False, "reset": False,
                                     "color": 0, "autouv": 0, "shade": True, "locked": False})
                entry = {"uuid": gid, "isOpen": True, "children": []}
                model.stack[-1].append(entry)
                model.stack.append(entry["children"])
                self.uuid = gid
                return self

            def __exit__(self, *exc):
                model.stack.pop()

        return G()

    def keyframes(self, group, name, keys):
        """keys: [(time, (rx, ry, rz))] for a group's rotation, linear."""
        self.animators[group.uuid] = {"name": name, "type": "bone", "keyframes": [
            {"channel": "rotation", "data_points": [{"x": "%.4f" % r[0], "y": "%.4f" % r[1], "z": "%.4f" % r[2]}],
             "uuid": uid(), "time": round(t, 4), "color": -1, "interpolation": "linear"} for t, r in keys]}

    def save(self, path, anim_length=1.0):
        textures = []
        for i, (name, data, emissive) in enumerate(self.textures):
            w, h = int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")
            textures.append({
                "path": "", "name": name + ".png", "folder": "", "namespace": "", "id": str(i), "width": w,
                "height": h, "uv_width": 16, "uv_height": 16 * h / w, "particle": False, "use_as_default": False,
                "layers_enabled": False, "sync_to_project": "", "render_mode": "emissive" if emissive else "default",
                "render_sides": "auto", "frame_time": 1, "frame_order_type": "loop", "frame_order": "",
                "frame_interpolate": False, "visible": True, "internal": True, "saved": False, "uuid": uid(),
                "source": "data:image/png;base64," + base64.b64encode(data).decode()})
        model = {
            "meta": {"format_version": "5.0", "model_format": "free", "box_uv": False},
            "name": self.name, "model_identifier": "", "visible_box": [1, 1, 0], "variable_placeholders": "",
            "variable_placeholder_buttons": [], "timeline_setups": [], "unhandled_root_fields": {},
            "groups": self.groups, "resolution": {"width": 16, "height": 16}, "elements": self.elements,
            "outliner": self.root, "textures": textures,
            "animations": [{"uuid": uid(), "name": "open", "loop": "once", "override": False, "length": anim_length,
                            "snapping": 24, "selected": False, "anim_time_update": "", "blend_weight": "",
                            "start_delay": "", "loop_delay": "", "animators": self.animators}] if self.animators else [],
        }
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        json.dump(model, open(path, "w"))
        return path
