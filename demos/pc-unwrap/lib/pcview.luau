-- The static scene: the room (WGSL), the PC (a Blockbench render, art/pc-unwrap/pc.py) and its two
-- glass panels. The render has no glass, so the panels are drawn live (lib/geom.luau puts them
-- exactly on the render): smoked glass with soft reflections, under the films.

local Geom = require("lib/geom")

local PcView = {}

-- A quad mesh through four points given in centimetres (top-left, top-right, bottom-right,
-- bottom-left).
local function quad(parent, shader, corners, params, z)
    local pos = {}
    for _, c in corners do
        local x, y = Geom.project(c[1], c[2], c[3])
        table.insert(pos, x)
        table.insert(pos, y)
    end
    return kulisa.mesh({
        parent = parent, positions = pos, uvs = { 0, 0, 1, 0, 1, 1, 0, 1 }, indices = { 1, 2, 3, 1, 3, 4 },
        shader = shader, params = params, z = z,
    })
end

-- Builds the room on `stage` and the PC in `pc`; returns the group of the case's parts and the
-- sprite of the render (for hit tests).
function PcView.build(stage, pc)
    local room = kulisa.load_shader("shaders/room.wgsl")
    local glass = kulisa.load_shader("shaders/glass.wgsl")

    -- The room is far bigger than the stage so wide windows never see its edge.
    local RX, RY, RW, RH = -3200, -2600, 7700, 5600
    local _, horizon = Geom.project(0, 0, 32) -- the back of the desk, just behind the case
    local cx, cy = Geom.FLOOR_X, Geom.FLOOR_Y
    kulisa.sprite({
        parent = stage, x = RX, y = RY, width = RW, height = RH, shader = room, z = -10,
        params = {
            horizon - RY, cx - RX, (Geom.CASE_RIGHT - Geom.CASE_LEFT) * 0.5, 34,
            RW, RH, 0, 0,
            cy + 6 - RY, 0, 0, 0,
            0, 0, 0, 0,
        },
    })

    local case = kulisa.group({ parent = pc })
    local path, x, y, s = Geom.image("pc")
    local img = kulisa.load_image(path)
    local sprite = kulisa.sprite({ parent = case, image = img, x = x, y = y, width = img.width * s, height = img.height * s, z = 0 })
    -- Smoked glass: the side panel (left) and the front panel (right).
    quad(case, glass, { { 22.2, 47.6, -11.5 }, { -22.5, 47.6, -11.5 }, { -22.5, 2.2, -11.5 }, { 22.2, 2.2, -11.5 } },
        { 0.012, 0.014, 0.02, 0.2, 0.18, 1.0, 1, 0 }, 2)
    quad(case, glass, { { -23, 47.6, -11.0 }, { -23, 47.6, 10.9 }, { -23, 2.2, 10.9 }, { -23, 2.2, -11.0 } },
        { 0.012, 0.014, 0.02, 0.24, 0.3, 0.48, 1, 0 }, 2)
    return case, sprite
end

return PcView
