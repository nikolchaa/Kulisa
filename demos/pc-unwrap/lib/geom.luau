-- Where things are. Everything in the scene was rendered by one orthographic camera
-- (art/pc-unwrap), and lib/art.luau holds what that camera does: the render pixel of any point
-- given in centimetres. The stage (design units) is the render scaled by K and moved so the PC
-- sits in the middle, so a point in centimetres has one stage position for the renders and the
-- live meshes alike.
--
-- Case coordinates (cm): x from the front (-23) to the back (23), y up from the floor (0..49), z
-- from the glass side (-11.5) to the motherboard side (11.5). The camera sees the glass side on the
-- left, the front glass on the right and the top from above.
--
-- Faces: films and the bag live in face coordinates (a along, b across, lifted by z off the
-- face), in face units (FU per centimetre, about a stage unit at the desk's zoom). Each face has
-- the stage direction of one unit along a, b and its normal, and the same three axes in camera
-- space (x right, y up, z towards the viewer), which the plastic shader lights with.

local Art = require("lib/art")

local sqrt = math.sqrt

local Geom = {}

export type Face = {
    name: string,
    ox: number, oy: number, -- stage position of a = b = 0
    ux: number, uy: number, -- stage direction of one unit along a
    vx: number, vy: number, -- ... along b
    zx: number, zy: number, -- ... along the normal (lifting a point off the face)
    width: number, height: number, -- extent of the face in a and b (face units)
    a3: { number }, b3: { number }, n3: { number }, -- the face axes in camera space
}

Geom.STAGE_W, Geom.STAGE_H = 1280, 800

-- Render pixels -> stage: the PC is centred at (640, 390) and about 640 units tall.
Geom.K = 0.56
local RCX, RCY = 1024, 1232 -- the PC's centre in render pixels
local SCX, SCY = 640, 390

local O, EX, EY, EZ = Art.origin, Art.ex, Art.ey, Art.ez

-- Stage position of a render pixel.
function Geom.from_render(rx: number, ry: number): (number, number)
    return (rx - RCX) * Geom.K + SCX, (ry - RCY) * Geom.K + SCY
end

-- Stage position of a point in centimetres.
function Geom.project(x: number, y: number, z: number): (number, number)
    local rx = O[1] + x * EX[1] + y * EY[1] + z * EZ[1]
    local ry = O[2] + x * EX[2] + y * EY[2] + z * EZ[2]
    return Geom.from_render(rx, ry)
end

-- Stage offset of a vector in centimetres.
function Geom.vec(x: number, y: number, z: number): (number, number)
    return (x * EX[1] + y * EY[1] + z * EZ[1]) * Geom.K, (x * EX[2] + y * EY[2] + z * EZ[2]) * Geom.K
end

-- Where a rendered image goes: its stage position and scale (stage units per image pixel).
function Geom.image(name: string): (string, number, number, number)
    local im = Art.images[name]
    local x, y = Geom.from_render(im.x, im.y)
    return im.path, x, y, im.scale * Geom.K
end

-- The orthographic scale (render pixels per centimetre across the view) and the case's axes in
-- camera space, recovered from the projection: screen x and y are the camera's x and -y, the
-- depth component completes a unit vector (its sign from which faces the camera sees).
local SCALE = sqrt(EX[1] * EX[1] + EZ[1] * EZ[1])
local function axis(e: { number }, toward: number): { number }
    local x, y = e[1] / SCALE, -e[2] / SCALE
    return { x, y, toward * sqrt(math.max(0, 1 - x * x - y * y)) }
end
Geom.X3 = axis(EX, -1) -- the front (-x) faces the camera
Geom.Y3 = axis(EY, 1) -- the top (+y) faces it
Geom.Z3 = axis(EZ, -1) -- the glass side (-z) faces it

-- Face units per centimetre: one face unit is about one stage unit.
Geom.FU = SCALE * Geom.K

local function scaled(v: { number }, k: number): { number }
    return { v[1] * k, v[2] * k, v[3] * k }
end

-- A face: origin (cm), axes a, b and normal as case directions (unit vectors in cm), size in cm.
local function face(name: string, origin: { number }, a: { number }, b: { number }, n: { number }, wa: number, wb: number): Face
    local ox, oy = Geom.project(origin[1], origin[2], origin[3])
    local fu = Geom.FU
    local ux, uy = Geom.vec(a[1] / fu, a[2] / fu, a[3] / fu)
    local vx, vy = Geom.vec(b[1] / fu, b[2] / fu, b[3] / fu)
    local zx, zy = Geom.vec(n[1] / fu, n[2] / fu, n[3] / fu)
    local function cam(v)
        local r = { 0, 0, 0 }
        for i, ax in { Geom.X3, Geom.Y3, Geom.Z3 } do
            for k = 1, 3 do
                r[k] += ax[k] * v[i]
            end
        end
        return r
    end
    return {
        name = name, ox = ox, oy = oy, ux = ux, uy = uy, vx = vx, vy = vy, zx = zx, zy = zy,
        width = wa * fu, height = wb * fu, a3 = cam(a), b3 = cam(b), n3 = cam(n),
    }
end
Geom.face = face

-- The glass side (left on screen): from the back edge to the front corner, top to bottom.
Geom.SIDE = { x0 = 22.0, x1 = -22.3, y0 = 47.3, y1 = 2.5, z = -11.5 }
Geom.side = face("side", { 22.0, 47.3, -11.5 }, { -1, 0, 0 }, { 0, -1, 0 }, { 0, 0, -1 }, 44.3, 44.8)
-- The front glass (right): from the corner to the far edge.
Geom.front = face("front", { -23, 47.3, -10.7 }, { 0, 0, 1 }, { 0, -1, 0 }, { -1, 0, 0 }, 21.4, 44.8)
-- The top, for the I/O sticker: along the front edge (away from the glass side), towards the back.
Geom.top = face("top", { -22.6, 49.1, -6.9 }, { 0, 0, 1 }, { 1, 0, 0 }, { 0, 1, 0 }, 13.8, 3.4)

-- Light and viewer in camera space.
local function unit(x: number, y: number, z: number): { number }
    local l = sqrt(x * x + y * y + z * z)
    return { x / l, y / l, z / l }
end
Geom.light = unit(-0.35, 0.8, 0.5)
Geom.view = { 0, 0, 1 }

-- Stage position of the point (a, b) lifted by z off the face.
function Geom.to_screen(f: Face, a: number, b: number, z: number): (number, number)
    return f.ox + a * f.ux + b * f.vx + z * f.zx, f.oy + a * f.uy + b * f.vy + z * f.zy
end

-- Face coordinates of a stage point, assuming it lies on the face (z = 0).
function Geom.to_face(f: Face, sx: number, sy: number): (number, number)
    local dx, dy = sx - f.ox, sy - f.oy
    local det = f.ux * f.vy - f.uy * f.vx
    return (dx * f.vy - dy * f.vx) / det, (f.ux * dy - f.uy * dx) / det
end

-- A normal given in a face's (a, b, up) axes, in camera space.
function Geom.world_normal(f: Face, na: number, nb: number, nz: number): (number, number, number)
    local a, b, n = f.a3, f.b3, f.n3
    return na * a[1] + nb * b[1] + nz * n[1], na * a[2] + nb * b[2] + nz * n[2], na * a[3] + nb * b[3] + nz * n[3]
end

-- The case's outline on the stage (for the room's shadow and glow).
do
    local xs, ys = {}, {}
    for _, x in { -23, 23 } do
        for _, y in { 0, 49 } do
            for _, z in { -11.5, 11.5 } do
                local sx, sy = Geom.project(x, y, z)
                table.insert(xs, sx)
                table.insert(ys, sy)
            end
        end
    end
    Geom.CASE_LEFT, Geom.CASE_RIGHT = math.min(table.unpack(xs)), math.max(table.unpack(xs))
    Geom.CASE_TOP, Geom.CASE_BOTTOM = math.min(table.unpack(ys)), math.max(table.unpack(ys))
end
-- Where the case stands: the stage point under its middle.
Geom.FLOOR_X, Geom.FLOOR_Y = Geom.project(0, 0, 0)

return Geom
