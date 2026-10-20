-- The static scene: room, case body and its three visible faces. All of it is procedural WGSL,
-- so the demo ships no art. The case is a dual-chamber showcase: front and side glass meet at a
-- corner with no pillar, neon trim on the edges, a perforated top.

local Geom = require("lib/geom")

local PcView = {}

-- A quad mesh from four stage points with the uvs of a face (top-left, top-right, bottom-right,
-- bottom-left).
local function quad_mesh(parent, shader, corners, params, z)
    local pos = {}
    for _, c in corners do
        table.insert(pos, c[1])
        table.insert(pos, c[2])
    end
    return kulisa.mesh({
        parent = parent,
        positions = pos,
        uvs = { 0, 0, 1, 0, 1, 1, 0, 1 },
        indices = { 1, 2, 3, 1, 3, 4 },
        shader = shader,
        params = params,
        z = z,
    })
end

local function face_corners(f: Geom.Face)
    local c = {}
    for _, ab in { { 0, 0 }, { f.width, 0 }, { f.width, f.height }, { 0, f.height } } do
        local sx, sy = Geom.to_screen(f, ab[1], ab[2], 0)
        table.insert(c, { sx, sy })
    end
    return c
end

-- Builds the room and the case; returns the group the case parts live in (to move or scale).
function PcView.build(stage, pc)
    local shaders = {
        room = kulisa.load_shader("shaders/room.wgsl"),
        front = kulisa.load_shader("shaders/case_front.wgsl"),
        top = kulisa.load_shader("shaders/case_top.wgsl"),
        side = kulisa.load_shader("shaders/case_side.wgsl"),
    }
    local L, R, T = Geom.left, Geom.right, Geom.top

    -- The room is far bigger than the stage so wide windows never see its edge.
    local RX, RY, RW, RH = -3200, -2600, 7700, 5600
    local horizon = 585
    kulisa.sprite({
        parent = stage, x = RX, y = RY, width = RW, height = RH, shader = shaders.room, z = -10,
        params = {
            horizon - RY, Geom.KX - 80 - RX, 440, 52,
            RW, RH, 0, 0,
            Geom.CASE_BOTTOM - 20 - RY, 0, 0, 0,
            0, 0, 0, 0,
        },
    })

    local case = kulisa.group({ parent = pc })

    -- Feet: small dark blocks under the front corner and the ends.
    local function foot(a: number, face: Geom.Face, dir: number)
        local x, y = Geom.to_screen(face, a, face.height, 0)
        local pts = { { x - 24, y }, { x + 24, y }, { x + 18, y + 16 }, { x - 18, y + 16 } }
        local pos = {}
        for _, p in pts do
            table.insert(pos, p[1])
            table.insert(pos, p[2])
        end
        kulisa.mesh({
            parent = case, positions = pos, indices = { 1, 2, 3, 1, 3, 4 },
            tint = "#0a0b10", z = 0,
        })
    end
    foot(40, L, 1)
    foot(L.width - 8, L, 1)
    foot(R.width - 40, R, 1)

    quad_mesh(case, shaders.side, face_corners(R), { R.width, R.height, 1, 0 }, 3)
    quad_mesh(case, shaders.top, face_corners(T), { T.width, T.height, 0, 0 }, 2)
    quad_mesh(case, shaders.front, face_corners(L), { L.width, L.height, 1, 0 }, 3)

    -- The corner post of light where the two glass panels meet: a thin bright line.
    local kx, ky = Geom.to_screen(L, L.width, 0, 0)
    local kx2, ky2 = Geom.to_screen(L, L.width, L.height, 0)
    kulisa.mesh({
        parent = case,
        positions = { kx - 1.2, ky, kx + 1.2, ky, kx + 1.2, ky2, kx - 1.2, ky2 },
        indices = { 1, 2, 3, 1, 3, 4 }, tint = "#8a96c8", opacity = 0.6, z = 4,
    })
    return case
end

return PcView
