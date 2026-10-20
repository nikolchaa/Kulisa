-- The case is drawn at three quarters: its front glass faces left, its side glass right, and the
-- top is seen from above. Each visible face has a frame: an origin on the stage, the screen
-- directions of its axes `a` and `b` (per face unit) and of its normal `z` (per unit of lift),
-- plus the same three axes in world space. Films live in face coordinates, so a peel is the same
-- math on any face; the normals the shader gets are in world space, so a film that wraps from one
-- face onto the next is lit consistently.
--
-- World axes: x to the right, y up, z towards the viewer. The camera is turned 30 degrees (yaw)
-- and looks down by 12 degrees (pitch), orthographically.

local Geom = {}

export type Face = {
    name: string,
    ox: number, oy: number, -- stage position of a = b = 0
    ux: number, uy: number, -- screen direction of one unit along a
    vx: number, vy: number, -- ... along b
    zx: number, zy: number, -- ... along the normal (lifting a point off the face)
    width: number, height: number, -- extent of the face in a and b
    a3: { number }, b3: { number }, n3: { number }, -- the face axes in world space
}

Geom.STAGE_W, Geom.STAGE_H = 1280, 800

local YAW, PITCH = math.rad(30), math.rad(12)
local sy, cy, sp, cp = math.sin(YAW), math.cos(YAW), math.sin(PITCH), math.cos(PITCH)

-- Screen position per unit of a world vector (orthographic, pitched down).
local function project(x: number, y: number, z: number): (number, number)
    return x, -y * cp + z * sp
end

-- Case size: front glass (length), side glass (depth) and height, in face units.
Geom.LEN, Geom.DEPTH, Geom.HEIGHT = 740, 480, 480

-- The top corner nearest the viewer, where the two glass panels meet.
Geom.KX, Geom.KY = 830, 235

-- World directions: along the front glass towards the corner, and along the side glass away.
local AL = { cy, 0, sy } -- front glass a-axis (to the right, towards the viewer)
local AR = { sy, 0, -cy } -- side glass a-axis (to the right, away from the viewer)
local NL = { -sy, 0, cy } -- front glass normal
local NR = { cy, 0, sy } -- side glass normal
local DOWN = { 0, -1, 0 }
local UP = { 0, 1, 0 }

local function vec(w: { number }): (number, number)
    return project(w[1], w[2], w[3])
end

local ulx, uly = vec(AL)
local urx, ury = vec(AR)
local vx, vy = vec(DOWN)
local zlx, zly = vec(NL)
local zrx, zry = vec(NR)
local ztx, zty = vec(UP)

local LCX, LCY = Geom.KX - Geom.LEN * ulx, Geom.KY - Geom.LEN * uly -- top-left of the front glass

-- The front glass (left): its a-axis runs towards the corner.
Geom.left = {
    name = "left", ox = LCX, oy = LCY, ux = ulx, uy = uly, vx = vx, vy = vy, zx = zlx, zy = zly,
    width = Geom.LEN, height = Geom.HEIGHT, a3 = AL, b3 = DOWN, n3 = NL,
} :: Face

-- The side glass (right), starting at the corner and running away.
Geom.right = {
    name = "right", ox = Geom.KX, oy = Geom.KY, ux = urx, uy = ury, vx = vx, vy = vy, zx = zrx, zy = zry,
    width = Geom.DEPTH, height = Geom.HEIGHT, a3 = AR, b3 = DOWN, n3 = NR,
} :: Face

-- The top: a runs along the front edge, b away from the viewer; lifting is straight up.
Geom.top = {
    name = "top", ox = LCX, oy = LCY, ux = ulx, uy = uly, vx = urx, vy = ury, zx = ztx, zy = zty,
    width = Geom.LEN, height = Geom.DEPTH, a3 = AL, b3 = AR, n3 = UP,
} :: Face

-- Light and viewer, in world space.
local function unit(x: number, y: number, z: number): { number }
    local l = math.sqrt(x * x + y * y + z * z)
    return { x / l, y / l, z / l }
end
Geom.light = unit(-0.35, 0.8, 0.5)
Geom.view = unit(0, sp, cp)

-- Screen position of the point (a, b) lifted by z off the face.
function Geom.to_screen(f: Face, a: number, b: number, z: number): (number, number)
    return f.ox + a * f.ux + b * f.vx + z * f.zx, f.oy + a * f.uy + b * f.vy + z * f.zy
end

-- Face coordinates of a screen point, assuming it lies on the face (z = 0).
function Geom.to_face(f: Face, sx: number, sy: number): (number, number)
    local dx, dy = sx - f.ox, sy - f.oy
    local det = f.ux * f.vy - f.uy * f.vx
    return (dx * f.vy - dy * f.vx) / det, (f.ux * dy - f.uy * dx) / det
end

-- A normal given in a face's (a, b, up) axes, in world space.
function Geom.world_normal(f: Face, na: number, nb: number, nz: number): (number, number, number)
    local a, b, n = f.a3, f.b3, f.n3
    return na * a[1] + nb * b[1] + nz * n[1], na * a[2] + nb * b[2] + nz * n[2], na * a[3] + nb * b[3] + nz * n[3]
end

Geom.PITCH_SIN, Geom.PITCH_COS = sp, cp
-- The case's bounding box on the stage (feet included).
Geom.OX, Geom.OY = LCX, LCY
Geom.CASE_LEFT = LCX
Geom.CASE_RIGHT = Geom.KX + Geom.DEPTH * urx
Geom.CASE_TOP = LCY + Geom.DEPTH * ury
Geom.CASE_BOTTOM = Geom.KY + Geom.HEIGHT * vy + 16

return Geom
