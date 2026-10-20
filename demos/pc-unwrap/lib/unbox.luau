-- The unboxing: a cardboard box taped shut, with the PC in its bag between two blocks of foam.
-- Peel the tape, pull the flaps open, take the foam out, lift the PC up and out. The box is a set of
-- renders from Blockbench (art/pc-unwrap): frames of the flaps opening, then separate layers (the
-- box, the case, each foam block, the front wall) so things can be lifted out between them. The
-- bag (cloth) and the tape (a Film) are the live parts on top.

local Film = require("lib/film")
local Sfx = require("lib/sfx")

local Unbox = {}
Unbox.__index = Unbox

-- The renders are 736 x 589 and drawn at 1.2x with their top-left here (stage units).
local BX, BY, BS = 200, 40, 1.2
local FRAMES = 7 -- frame0 (closed) .. frame6 (open)

-- Image-pixel positions in the renders.
local SEAM_X0, SEAM_X1, SEAM_Y = 116, 620, 307 -- the tape runs along the closed flaps' seam
local CASE = { x = 196, y = 226, w = 345, h = 278 } -- where the case sits in the renders

local function stage_x(ix: number): number
    return BX + ix * BS
end
local function stage_y(iy: number): number
    return BY + iy * BS
end

local function ease(t: number): number
    t = math.clamp(t, 0, 1)
    return t * t * (3 - 2 * t)
end

function Unbox.new(stage, pc, bag, case_art, to_design, films)
    local self = setmetatable({}, Unbox)
    self.stage, self.pc, self.bag, self.case_art, self.to_design, self.films = stage, pc, bag, case_art, to_design, films
    self.frames = {}
    for i = 0, FRAMES - 1 do
        self.frames[i] = kulisa.load_image(`images/unbox/frame{i}.png`)
    end
    local function layer(name: string, z: number)
        return kulisa.sprite({
            parent = stage, image = `images/unbox/{name}.png`, x = BX, y = BY, scale = BS, z = z, visible = false,
        })
    end
    self.base = kulisa.sprite({ parent = stage, image = self.frames[0], x = BX, y = BY, scale = BS, z = 10 })
    self.l_back = layer("back_empty", 10)
    self.l_case = layer("case", 12)
    self.l_foam_l = layer("foam_left", 20)
    self.l_foam_r = layer("foam_right", 20)
    self.l_front = layer("front", 30)

    -- The tape: a film along the seam, lying on the box top.
    local len = (SEAM_X1 - SEAM_X0) * BS
    local face = {
        name = "tape", ox = stage_x(SEAM_X0), oy = stage_y(SEAM_Y) - 9, ux = 1, uy = 0, vx = 0, vy = 1, zx = 0, zy = -0.5,
        width = len, height = 18, a3 = { 1, 0, 0 }, b3 = { 0, -0.79, 0.61 }, n3 = { 0, 0.61, 0.79 },
    }
    self.tape = Film.new({
        parent = stage, spans = { { face = face, len = len } }, b0 = 0, height = 18, cols = 48, rows = 4, z = 40,
        kind = "clear", tab = { 56, 20 }, rscale = 0.4, wedge = 0.2, tint = { 0.75, 0.58, 0.32 }, alpha = 0.42,
        gloss = 1.1, wrinkle = 0.08,
    })
    self.tape.on_slip = function(strength)
        Sfx.slip(strength, 640)
    end
    self.tape.on_detach = function()
        Sfx.tear(640)
        self:_tape_off()
    end
    self:reset()
    return self
end

function Unbox:reset()
    self.phase = "tape" -- tape | flaps | foam | lift | done
    self.open = 0 -- flaps opening, 0..1
    self.open_target = 0
    self.drag = nil
    self.foam = { l = { removed = false }, r = { removed = false } }
    self.lift = 0
    self.exit_t = nil
    self.base.visible = true
    self.base.image = self.frames[0]
    self.base.opacity = 1
    for _, s in { self.l_back, self.l_case, self.l_foam_l, self.l_foam_r, self.l_front } do
        s.visible = false
        s.opacity = 1
        s.x, s.y, s.rotation = BX, BY, 0
    end
    self.tape:reset()
    self.tape.mesh.visible, self.tape.shadow.visible = true, true
    -- The PC (just its bag) sits in the box.
    local bag = self.bag
    bag.crumple = 1
    bag:reset()
    local cx, cy = stage_x(CASE.x + CASE.w / 2), stage_y(CASE.y + CASE.h / 2)
    local s = 0.45
    self.pc.scale = s
    self.pc.x, self.pc.y = cx - 630 * s, cy - 411 * s
    self.pc.opacity = 1
    self.case_art.visible = false
    self.case_art.opacity = 1
    self.films.visible = false
    self.pc_home = { self.pc.x, self.pc.y }
    self.bag.mesh.visible = false -- the bag shows once the box is open
end

function Unbox:active(): boolean
    return self.phase ~= "done"
end

-- The tape came off: the flaps give a little.
function Unbox:_tape_off()
    self.phase = "flaps"
    self.open_target = 0.2
    if self.on_task then
        self.on_task("tape")
    end
end

local function inside(node, sx: number, sy: number, to_design, alpha: boolean?): boolean
    local x, y = to_design(sx, sy)
    return node:contains(x, y, alpha)
end

-- What a press at a stage point grabs, or nil.
function Unbox:target(sx: number, sy: number)
    local ph = self.phase
    if ph == "tape" then
        if self.tape:can_grab(sx, sy) then
            return self.tape
        end
        return nil
    elseif ph == "flaps" then
        if sx > stage_x(0) and sx < stage_x(736) and sy > stage_y(60) and sy < stage_y(540) then
            return self:_flap_grab()
        end
    elseif ph == "foam" then
        for key, foam in { l = self.l_foam_l, r = self.l_foam_r } do
            if not self.foam[key].removed and inside(foam, sx, sy, self.to_design, true) then
                return self:_foam_grab(key, foam)
            end
        end
    elseif ph == "lift" then
        local x0, y0 = stage_x(CASE.x), stage_y(CASE.y)
        if sx > x0 and sx < x0 + CASE.w * BS and sy > y0 and sy < y0 + CASE.h * BS then
            return self:_lift_grab()
        end
    end
    return nil
end

function Unbox:_flap_grab()
    local u = self
    local g = { sx = 0, sy = 0 }
    function g:begin_grab(sx, sy)
        g.sx, g.sy = sx, sy
        g.base = u.open
        u.drag_open = true
    end
    function g:drag(sx, sy)
        local d = math.sqrt((sx - g.sx) ^ 2 + (sy - g.sy) ^ 2)
        u.open_target = math.clamp(g.base + d / 240, 0, 1)
        u.open = math.max(u.open, u.open_target)
    end
    function g:release()
        u.drag_open = false
        u.open_target = if u.open > 0.5 then 1 else u.open_target
    end
    return g
end

function Unbox:_foam_grab(key: string, foam)
    local u = self
    local g = {}
    function g:begin_grab(sx, sy)
        g.ox, g.oy = foam.x - sx, foam.y - sy
        g.x0, g.y0 = foam.x, foam.y
        foam.z = 31
    end
    function g:drag(sx, sy)
        foam.x, foam.y = sx + g.ox, sy + g.oy
    end
    function g:release()
        foam.z = 20
        local d = math.sqrt((foam.x - g.x0) ^ 2 + (foam.y - g.y0) ^ 2)
        if d > 110 then
            u.foam[key].removed = true
            local dir = if key == "l" then -1 else 1
            foam:tween({ x = foam.x + dir * 700, y = foam.y - 150, rotation = dir * 0.5, opacity = 0 }, { time = 0.6, ease = "in_cubic" })
            Sfx.whoosh(500, 640 + dir * 300)
            if u.foam.l.removed and u.foam.r.removed then
                u.phase = "lift"
                if u.on_task then
                    u.on_task("foam")
                end
            end
        else
            foam:tween({ x = BX, y = BY }, { time = 0.2, ease = "out_back" })
        end
    end
    return g
end

function Unbox:_lift_grab()
    local u = self
    local g = {}
    function g:begin_grab(sx, sy)
        g.sy = sy
        g.base = u.lift
    end
    function g:drag(sx, sy)
        u.lift = math.clamp(g.base + (g.sy - sy) / 260, 0, 1.2)
        u:_place_lift()
        Sfx.rustle(300, 640)
    end
    function g:release()
        if u.lift > 0.55 then
            u:_leave()
        else
            u.lift_target = 0
        end
    end
    return g
end

-- The case and its bag follow the lift.
function Unbox:_place_lift()
    local dy = -self.lift * 190
    self.l_case.y = BY + dy
    self.pc.y = self.pc_home[2] + dy
end

-- The PC is out: the box drops away and the PC settles on the desk.
function Unbox:_leave()
    self.phase = "done"
    self.exit_t = 0
    self.exit_from = { self.pc.x, self.pc.y, self.pc.scale }
    self.case_art.visible = true
    self.case_art.opacity = 0
    self.films.visible = true
    Sfx.whoosh(700, 640)
    if self.on_task then
        self.on_task("lift")
    end
end

function Unbox:update(dt: number)
    self.tape:update(dt)
    local ph = self.phase
    if ph == "tape" or ph == "flaps" then
        -- Ease the flaps towards their target and show the matching frame.
        local diff = self.open_target - self.open
        if not self.drag_open and diff ~= 0 then
            self.open += math.clamp(diff, -dt * 1.1, dt * 1.1)
        end
        local frame = math.clamp(math.floor(self.open * (FRAMES - 1) + 0.5), 0, FRAMES - 1)
        if self.base.visible then
            self.base.image = self.frames[frame]
        end
        if ph == "flaps" and self.open >= 0.999 then
            self.open = 1
            self.phase = "foam"
            self:_show_layers()
            if self.on_task then
                self.on_task("flaps")
            end
        end
    elseif ph == "lift" then
        if self.lift_target and not self.dragging then
            self.lift += (self.lift_target - self.lift) * math.min(1, dt * 10)
            self:_place_lift()
            if math.abs(self.lift - self.lift_target) < 0.002 then
                self.lift_target = nil
            end
        end
    elseif ph == "done" and self.exit_t then
        self.exit_t += dt
        local t = ease(self.exit_t / 1.0)
        -- The PC settles to its place on the desk; the box and the placeholder fade and sink.
        local from = self.exit_from
        self.pc.x = from[1] * (1 - t)
        self.pc.y = from[2] * (1 - t)
        self.pc.scale = from[3] + (1 - from[3]) * t
        self.case_art.opacity = t
        self.l_case.opacity = 1 - t
        for _, s in { self.l_back, self.l_foam_l, self.l_foam_r, self.l_front } do
            s.opacity = 1 - t
            s.y = BY + 160 * t
        end
        self.base.opacity = 1 - t
        self.bag.crumple = 1 - t
        self.bag:wake()
        if self.exit_t >= 1.0 then
            self.exit_t = nil
            for _, s in { self.l_back, self.l_case, self.l_foam_l, self.l_foam_r, self.l_front, self.base } do
                s.visible = false
            end
            self.bag.crumple = 0
            self.bag:wake()
            if self.on_done then
                self.on_done()
            end
        end
    end
end

-- Jumps to the end (a shortcut for testing and for impatient players).
function Unbox:skip()
    for _, key in { "tape", "flaps", "foam", "lift" } do
        if self.on_task then
            self.on_task(key)
        end
    end
    self.tape.mesh.visible, self.tape.shadow.visible = false, false
    self.tape.mode = "gone"
    self.phase = "done"
    self.exit_t = nil
    self.pc.scale, self.pc.x, self.pc.y = 1, 0, 0
    self.case_art.visible, self.case_art.opacity = true, 1
    self.films.visible = true
    for _, s in { self.l_back, self.l_case, self.l_foam_l, self.l_foam_r, self.l_front, self.base } do
        s.visible = false
    end
    self.bag.mesh.visible = true
    self.bag.crumple = 0
    self.bag:wake()
    if self.on_done then
        self.on_done()
    end
end

-- Flaps are open: swap the one big image for its layers, so foam and case can be lifted out.
function Unbox:_show_layers()
    self.base.visible = false
    for _, s in { self.l_back, self.l_case, self.l_foam_l, self.l_foam_r, self.l_front } do
        s.visible = true
    end
    self.bag.mesh.visible = true
    self.bag:wake()
end

return Unbox
