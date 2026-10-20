-- The diorama: one ray-marched picture and its depth map (art/parallax/diorama.py) drawn through
-- a steep-parallax shader, so the still image turns like a small 3D scene as the view moves.
-- Dust motes drift in front of it.

local Diorama = {}

local DEPTH = kulisa.load_shader("shaders/depth.wgsl")
local root, pic, motes
local view_x, view_y = 0, 0

function Diorama.build(parent)
    root = kulisa.layer({ parent = parent, parallax = 0, z = 5 })
    pic = kulisa.sprite({ parent = root, image = "images/diorama.png", shader = DEPTH })
    pic.mask = "images/diorama_depth.png"
    motes = kulisa.particles({
        parent = root, capacity = 260, rate = 18, shape = "rect", life = { 6, 10 }, speed = { 3, 10 },
        direction = -80, spread = 120, size = { 4, 3 }, scale = { 0.5, 1.6 }, wander = { 30, 0.12 },
        colors = { "#fff4d000", "#fff0c8cc", "#ffe8b000" }, fade = { 0.3, 0.4 }, blend = "add", warm_up = 8, z = 2,
    })
    root.visible = false
    Diorama.place()
end

-- The picture covers the screen (cropped, never stretched).
function Diorama.place()
    if not pic then
        return
    end
    local vx, vy, vw, vh = kulisa.screen:visible()
    local s = math.max(vw / 960, vh / 540)
    pic.width, pic.height = 960 * s, 540 * s
    pic.x, pic.y = vx + (vw - pic.width) / 2, vy + (vh - pic.height) / 2
    motes.x, motes.y = vx + vw / 2, vy + vh / 2
    motes:set({ shape = "rect", width = vw, height = vh })
end

-- `dx`, `dy` in -1..1: where the viewer looks from (pointer or tilt).
function Diorama.view(dx: number, dy: number, dt: number)
    if not (root and root.visible) then
        return
    end
    local k = 1 - math.exp(-dt * 6)
    view_x += (dx - view_x) * k
    view_y += (dy - view_y) * k
    -- Near things move most; the arch (depth ~0.5) stays put; zoom hides the pulled-in edges.
    pic.params = { -view_x * 0.035, -view_y * 0.025, 0.42, 1.06 }
    motes.x = select(1, kulisa.screen:visible()) + select(3, kulisa.screen:visible()) / 2 - view_x * 40
end

function Diorama.show(on: boolean)
    if root then
        root.visible = on
    end
end

function Diorama.alive(): number
    return if motes then motes.alive else 0
end

return Diorama
