-- The case is drawn in an oblique projection. Each visible face has a frame: an origin on the
-- screen (stage coordinates), the screen directions of its two axes `a` and `b` (per face unit)
-- and of its normal `z` (per unit of lift). Films live in face coordinates, so a peel is the same
-- math on any face; only the projection (and the light, given in the face's own axes) differs.

local Geom = {}

export type Face = {
    name: string,
    ox: number, oy: number, -- screen position of a = b = 0
    ux: number, uy: number, -- screen direction of one unit along a
    vx: number, vy: number, -- ... along b
    zx: number, zy: number, -- ... along the normal (lifting a point off the face)
    width: number, height: number, -- extent of the face in a and b
    light: { number }, -- direction to the light in (a, b, normal) axes
    view: { number }, -- direction to the viewer in the same axes
}

local function unit(x: number, y: number, z: number): { number }
    local l = math.sqrt(x * x + y * y + z * z)
    return { x / l, y / l, z / l }
end

-- Stage layout (stage units; the stage is scaled to fit the window).
Geom.STAGE_W, Geom.STAGE_H = 1280, 800
local OX, OY = 185, 282
local DEPTH_X, DEPTH_Y = 150, -125 -- where the back of the case lands for the full depth

-- The glass side panel faces the viewer; lifting a point off it shifts it down and to the left.
Geom.glass = {
    name = "glass",
    ox = OX, oy = OY, ux = 1, uy = 0, vx = 0, vy = 1, zx = -0.34, zy = 0.28,
    width = 760, height = 440,
    light = unit(-0.45, -0.65, 0.62), -- the light is up, left and in front of the case
    view = unit(0, 0, 1),
} :: Face

-- The top panel lies back and to the right of the glass's top edge; lifting is straight up.
Geom.top = {
    name = "top",
    ox = OX, oy = OY, ux = 1, uy = 0, vx = DEPTH_X / 380, vy = DEPTH_Y / 380, zx = 0, zy = -1,
    width = 760, height = 380,
    light = unit(-0.45, -0.62, 0.65),
    view = unit(0, -0.83, 0.55),
} :: Face

Geom.OX, Geom.OY, Geom.DEPTH_X, Geom.DEPTH_Y = OX, OY, DEPTH_X, DEPTH_Y

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

return Geom
