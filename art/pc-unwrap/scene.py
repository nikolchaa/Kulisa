# The camera every model of demos/pc-unwrap is rendered with, and the invisible anchors that give
# every render the same framing (the renderer fits the frame to the model's bounds). With one
# camera and one frame, a point in centimetres lands on the same pixel in every render, so the
# demo can stack the renders as layers and put its live meshes (bag, films, tape) on top.
import contextlib, struct, zlib

PITCH, YAW = -12, -30  # the renderer's front view adds some elevation of its own; calibrate.py measures it
# Everything that is ever rendered fits in here (the box with its flaps standing up).
BOUNDS = (-36.0, -2.0, -25.0, 36.0, 75.0, 25.0)
RENDER = 2048  # square renders, orthographic, "front" view


def solid_png(rgba, w=4, h=4):
    raw = b"".join(b"\x00" + bytes(rgba) * w for _ in range(h))
    ch = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + ch(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + ch(b"IDAT", zlib.compress(raw)) + ch(b"IEND", b""))


@contextlib.contextmanager
def world(m):
    """Opens the camera groups and places the framing anchors; build the model inside."""
    m.texture("clear", solid_png((0, 0, 0, 0)))
    with m.group("pitch", (0, 24.5, 0), (PITCH, 0, 0)), m.group("yaw", (0, 24.5, 0), (0, YAW, 0)):
        x0, y0, z0, x1, y1, z1 = BOUNDS
        for x in (x0, x1):
            for y in (y0, y1):
                for z in (z0, z1):
                    m.cube("anchor", (x, y, z), (x + 0.02, y + 0.02, z + 0.02), "clear")
        yield


# Calibration markers: coloured cubes at known points, rendered alone (calibrate.py finds them).
MARKERS = [((x, y, z), c) for (x, y, z), c in zip(
    [(-23, 0, -11.5), (-23, 0, 11.5), (-23, 49, -11.5), (-23, 49, 11.5), (23, 0, -11.5), (23, 0, 11.5),
     (23, 49, -11.5), (23, 49, 11.5)],
    [(255, 0, 0, 255), (0, 255, 0, 255), (0, 0, 255, 255), (255, 255, 0, 255), (255, 0, 255, 255),
     (0, 255, 255, 255), (255, 128, 0, 255), (255, 255, 255, 255)])]


def markers(m):
    for i, ((x, y, z), c) in enumerate(MARKERS):
        m.texture("m%d" % i, solid_png(c), emissive=True)
        m.cube("mk%d" % i, (x - 0.4, y - 0.4, z - 0.4), (x + 0.4, y + 0.4, z + 0.4), "m%d" % i)
