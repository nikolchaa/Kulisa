-- The static scene: room, case body and its three visible faces. All of it is procedural WGSL,
-- so the demo ships no art.

local Geom = require("lib/geom")

local PcView = {}

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

function PcView.build(stage)
    local shaders = {
        room = kulisa.load_shader("shaders/room.wgsl"),
        glass = kulisa.load_shader("shaders/case_glass.wgsl"),
        top = kulisa.load_shader("shaders/case_top.wgsl"),
        side = kulisa.load_shader("shaders/case_side.wgsl"),
    }
    local g, t = Geom.glass, Geom.top

    -- The room is far bigger than the stage so wide windows never see its edge.
    local RX, RY, RW, RH = -1000, -700, 3280, 2200
    local desk_y = Geom.OY + g.height + 14
    kulisa.sprite({
        parent = stage, x = RX, y = RY, width = RW, height = RH, shader = shaders.room, z = -10,
        params = { desk_y - RY, Geom.OX + g.width * 0.55 + 60 - RX, 560, 46, RW, RH, 0, 0 },
    })

    -- Right side panel (the back of the case is hidden behind it).
    local fx, fy = Geom.to_screen(g, g.width, 0, 0)
    quad_mesh(stage, shaders.side, {
        { fx, fy }, { fx + Geom.DEPTH_X, fy + Geom.DEPTH_Y },
        { fx + Geom.DEPTH_X, fy + Geom.DEPTH_Y + g.height }, { fx, fy + g.height },
    }, { 380, g.height, 0, 0 }, 1)

    -- Top panel.
    local c = {}
    for _, ab in { { 0, 0 }, { t.width, 0 }, { t.width, t.height }, { 0, t.height } } do
        local sx, sy = Geom.to_screen(t, ab[1], ab[2], 0)
        table.insert(c, { sx, sy })
    end
    quad_mesh(stage, shaders.top, c, { t.width, t.height, 0, 0 }, 2)

    -- Glass panel with the machine behind it.
    kulisa.sprite({
        parent = stage, x = g.ox, y = g.oy, width = g.width, height = g.height, shader = shaders.glass,
        params = { g.width, g.height, 1, 0 }, z = 3,
    })
end

return PcView
