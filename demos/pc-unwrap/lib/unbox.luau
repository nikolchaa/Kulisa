-- The unboxing: a shipping box taped shut, the PC in its bag standing on two foam end caps with two
-- more on top. Peel the tape, pull the flaps open, take the top caps out, lift the PC up and out.
--
-- The box is a set of Blockbench renders (images/pc) made with the same camera as the
-- PC, so they stack with the live PC between them: the back of the box and the bottom caps behind
-- it, the two near walls in front, the flaps in front of everything while they are closed. The
-- flaps are 12 frames of their opening animation. While the box is on the desk the whole view is
-- zoomed out so it fits; when the PC is out, the box goes and the view zooms in on the PC.

local Film = require("lib/film")
local Geom = require("lib/geom")
local Sfx = require("lib/sfx")

local Unbox = {}
Unbox.__index = Unbox

local FRAMES = 12
local SPLIT = 4 -- frames before this keep the front short flap closed (in its own layer)
local PC_LIFT = 3.8 -- the PC stands this high (cm) on the bottom foam caps
local BOX_H = 56.8 -- the box's walls (cm)
local OUT = 58 -- lifted this far (cm), the PC is clear of the box

-- Draw order: the box's back, the PC, the near walls, the flaps.
local Z = { back = 10, far_open = 12, pc = 15, foam = 18, near_wall = 20, front_flap = 21, far = 22, near = 24, held = 26, tape = 30 }

local function ease(t: number): number
    t = math.clamp(t, 0, 1)
    return t * t * (3 - 2 * t)
end

-- A layer from the renders, placed by lib/art.luau.
local function layer(parent, name: string, z: number)
    local path, x, y, s = Geom.image(name)
    local img = kulisa.load_image(path)
    return kulisa.sprite({ parent = parent, image = img, x = x, y = y, width = img.width * s, height = img.height * s, z = z })
end

-- Puts a sprite on another render (keeping the sprite).
local function show(sprite, name: string)
    local path, x, y, s = Geom.image(name)
    local img = kulisa.load_image(path)
    sprite.image = img
    sprite.x, sprite.y, sprite.width, sprite.height = x, y, img.width * s, img.height * s
end

function Unbox.new(view, pc, bag, case_art, to_design, films)
    local self = setmetatable({}, Unbox)
    self.view, self.pc, self.bag, self.case_art, self.to_design, self.films = view, pc, bag, case_art, to_design, films
    for i = 0, FRAMES - 1 do
        kulisa.load_image(Geom.image(`flaps_far{i}`))
        kulisa.load_image(Geom.image(`flaps_near{i}`))
    end
    -- The layers are siblings of the PC (not in a group of their own), so they interleave with it.
    self.back = layer(view, "box_back", Z.back)
    self.near_wall = layer(view, "box_near", Z.near_wall)
    self.front_flap = layer(view, "flap_front", Z.front_flap)
    self.far = layer(view, "flaps_far0", Z.far)
    self.near = layer(view, "flaps_near_long0", Z.near)
    self.layers = { self.back, self.near_wall, self.front_flap, self.far, self.near }
    self.foams = {
        rear = { sprite = layer(view, "foam_rear", Z.foam), dir = -1 },
        front = { sprite = layer(view, "foam_front", Z.foam), dir = 1 },
    }
    for _, f in self.foams do
        f.x0, f.y0 = f.sprite.x, f.sprite.y
    end

    -- The tape along the seam where the long flaps meet, on top of the closed box; pulled from the
    -- back end (left on screen) towards the front.
    local top = BOX_H + 0.8
    local face = Geom.face("tape", { 28.6, top, -2.4 }, { -1, 0, 0 }, { 0, 0, 1 }, { 0, 1, 0 }, 57.2, 4.8)
    self.tape = Film.new({
        parent = view, spans = { { face = face, len = face.width } }, b0 = 0, height = face.height, cols = 48, rows = 4,
        z = Z.tape, kind = "clear", tab = { 60, face.height + 20 }, rscale = 0.4, wedge = 0.2,
        tint = { 0.7, 0.5, 0.26 }, alpha = 0.72, gloss = 1.1, wrinkle = 0.08,
    })
    self.tape.on_slip = function(strength)
        Sfx.slip(strength, 640)
    end
    self.tape.on_detach = function()
        Sfx.tear(640)
        self:_tape_off()
    end
    -- The box's top, for grabbing the flaps (stage points of its corners).
    self.top = {}
    for _, c in { { -27.8, -15.8 }, { 27.8, -15.8 }, { 27.8, 15.8 }, { -27.8, 15.8 } } do
        local x, y = Geom.project(c[1], top, c[2])
        table.insert(self.top, { x, y })
    end
    self:reset()
    return self
end

-- The view: zoomed out on the box, or in on the PC. `k` blends between them (0 box, 1 PC).
function Unbox:_frame_view(k: number)
    local zoom = 0.8 + 0.2 * k
    local fx, fy = Geom.project(0, 26 * (1 - k) + 24.5 * k, 0)
    -- The focus point goes a little below the middle of the stage (the stage is 1280 x 800).
    self.view.scale = zoom
    self.view.x, self.view.y = 640 - fx * zoom, 440 - fy * zoom - 15 * (1 - k)
end

function Unbox:reset()
    self.phase = "tape" -- tape | flaps | foam | lift | leaving | done
    self.open, self.open_target = 0, 0
    self.drag_open = false
    self.lift, self.lift_target = 0, nil
    self.exit_t = nil
    self.box_dy = 0
    for _, l in self.layers do
        l.visible, l.opacity = true, 1
    end
    show(self.back, "box_back")
    show(self.near_wall, "box_near")
    show(self.front_flap, "flap_front")
    for _, f in self.foams do
        f.removed = false
        f.sprite.x, f.sprite.y, f.sprite.rotation, f.sprite.opacity, f.sprite.z = f.x0, f.y0, 0, 1, Z.foam
        f.sprite.visible = true
    end
    self.far.z = Z.far
    self:_show_frame(0)
    self.tape:reset()
    self.tape.mesh.visible, self.tape.shadow.visible = true, true
    self.bag:reset()
    self:_place_pc()
    self.pc.visible = true
    self.films.visible = true
    self:_frame_view(0)
end

function Unbox:active(): boolean
    return self.phase ~= "done"
end

function Unbox:_show_frame(i: number)
    self.frame = i
    show(self.far, `flaps_far{i}`)
    if i < SPLIT then
        show(self.near, `flaps_near_long{i}`)
    else
        show(self.near, `flaps_near{i}`)
    end
    self.front_flap.opacity = if i < SPLIT then 1 else 0
end

-- The PC stands on the bottom caps, raised by `lift` cm.
function Unbox:_place_pc()
    local _, dy = Geom.vec(0, PC_LIFT + self.lift, 0)
    self.pc.x, self.pc.y = 0, dy
end

function Unbox:_tape_off()
    self.phase = "flaps"
    self.open_target = 0.08
    self.tape.mesh.visible, self.tape.shadow.visible = false, false
    if self.on_task then
        self.on_task("tape")
    end
end

-- Inside the polygon of the box's top (stage points), with a margin around it.
local function on_top(pts, sx: number, sy: number, margin: number): boolean
    local minx, maxx, miny, maxy = 1e9, -1e9, 1e9, -1e9
    for _, p in pts do
        minx, maxx = math.min(minx, p[1]), math.max(maxx, p[1])
        miny, maxy = math.min(miny, p[2]), math.max(maxy, p[2])
    end
    return sx > minx - margin and sx < maxx + margin and sy > miny - margin * 2 and sy < maxy + margin
end

-- What a press at a view point grabs, or nil.
function Unbox:target(sx: number, sy: number)
    local ph = self.phase
    if ph == "tape" then
        if self.tape:can_grab(sx, sy) then
            return self.tape
        end
    elseif ph == "flaps" then
        if on_top(self.top, sx, sy, 80) then
            return self:_flap_grab()
        end
    elseif ph == "foam" then
        for _, f in self.foams do
            if not f.removed then
                local x, y = self.to_design(sx, sy)
                if f.sprite:contains(x, y, true) then
                    return self:_foam_grab(f)
                end
            end
        end
    elseif ph == "lift" then
        local x, y = self.to_design(sx, sy)
        if self.case_art:contains(x, y, true) or self.bag:contains(sx, sy) then
            return self:_lift_grab()
        end
    end
    return nil
end

function Unbox:_flap_grab()
    local u = self
    local g = {}
    function g:begin_grab(sx, sy)
        g.sx, g.sy = sx, sy
        g.base = u.open
        u.drag_open = true
    end
    function g:drag(sx, sy)
        local d = math.sqrt((sx - g.sx) ^ 2 + (sy - g.sy) ^ 2)
        u.open_target = math.clamp(g.base + d / 300, 0, 1)
        u.open = math.max(u.open, u.open_target)
    end
    function g:release()
        u.drag_open = false
        u.open_target = if u.open > 0.45 then 1 else u.open_target
    end
    return g
end

function Unbox:_foam_grab(f)
    local u = self
    local g = {}
    local s = f.sprite
    function g:begin_grab(sx, sy)
        g.ox, g.oy = s.x - sx, s.y - sy
        s.z = Z.held
    end
    function g:drag(sx, sy)
        -- A cap comes out upwards first (it sits in the box), then can go anywhere.
        local x, y = sx + g.ox, sy + g.oy
        local up = f.y0 - y
        if up < 60 then
            x = f.x0 + (x - f.x0) * math.max(0, up) / 60
        end
        s.x, s.y = x, math.min(y, f.y0)
    end
    function g:release()
        local d = math.sqrt((s.x - f.x0) ^ 2 + (s.y - f.y0) ^ 2)
        if d > 110 then
            f.removed = true
            s:tween({ x = s.x + f.dir * 600, y = s.y - 120, rotation = f.dir * 0.4, opacity = 0 }, { time = 0.6, ease = "in_cubic" })
            Sfx.whoosh(500, 640 + f.dir * 300)
            if u.foams.rear.removed and u.foams.front.removed then
                u.phase = "lift"
                if u.on_task then
                    u.on_task("foam")
                end
            end
        else
            s.z = Z.foam
            s:tween({ x = f.x0, y = f.y0 }, { time = 0.2, ease = "out_back" })
        end
    end
    return g
end

function Unbox:_lift_grab()
    local u = self
    local g = {}
    local _, up = Geom.vec(0, 1, 0)
    function g:begin_grab(sx, sy)
        g.sy = sy
        g.base = u.lift
        u.lift_target = nil
        u.dragging = true
    end
    function g:drag(sx, sy)
        u.lift = math.clamp(g.base + (sy - g.sy) / up, 0, OUT)
        u:_place_pc()
        Sfx.rustle(260, 640)
    end
    function g:release()
        u.dragging = false
        if u.lift > OUT * 0.7 then
            u.lift_target = OUT
        else
            -- Not out yet: it settles back onto the caps.
            u.lift_target = 0
        end
    end
    return g
end

-- The PC is clear of the box: the box goes and the PC comes down to the desk.
function Unbox:_leave()
    self.phase = "leaving"
    self.exit_t = 0
    Sfx.whoosh(700, 640)
    if self.on_task then
        self.on_task("lift")
    end
end

function Unbox:update(dt: number)
    self.tape:update(dt)
    local ph = self.phase
    if ph == "tape" or ph == "flaps" then
        local diff = self.open_target - self.open
        if not self.drag_open and diff ~= 0 then
            self.open += math.clamp(diff, -dt * 1.2, dt * 1.2)
        end
        local frame = math.clamp(math.floor(self.open * (FRAMES - 1) + 0.5), 0, FRAMES - 1)
        if frame ~= self.frame then
            self:_show_frame(frame)
        end
        if ph == "flaps" and self.open >= 0.999 then
            self.open = 1
            self.phase = "foam"
            -- Open, the far flap stands behind the PC.
            self.far.z = Z.far_open
            if self.on_task then
                self.on_task("flaps")
            end
        end
    elseif ph == "lift" then
        if self.lift_target and not self.dragging then
            self.lift += (self.lift_target - self.lift) * math.min(1, dt * 8)
            if math.abs(self.lift - self.lift_target) < 0.05 then
                self.lift = self.lift_target
                self.lift_target = nil
                if self.lift >= OUT then
                    self:_leave()
                end
            end
            self:_place_pc()
        end
    elseif ph == "leaving" then
        self.exit_t += dt
        local t = ease(self.exit_t / 1.2)
        -- The box slides away down and fades; the PC is set down on the desk; the view zooms in.
        local _, dy = Geom.vec(0, -30 * t, 0)
        for _, l in self.layers do
            l.opacity = if l == self.front_flap then 0 else 1 - t
            l.y += dy - self.box_dy
        end
        self.box_dy = dy
        self.lift = OUT * (1 - t) - PC_LIFT * t
        self:_place_pc()
        self:_frame_view(t)
        if self.exit_t >= 1.2 then
            self:_finish()
        end
    end
end

function Unbox:_finish()
    self.phase = "done"
    self.exit_t = nil
    for _, l in self.layers do
        l.visible = false
    end
    for _, f in self.foams do
        f.sprite.visible = false
    end
    self.lift = -PC_LIFT
    self:_place_pc()
    self:_frame_view(1)
    self.bag:wake()
    if self.on_done then
        self.on_done()
    end
end

-- Jumps to the end (a shortcut for testing and for impatient players).
function Unbox:skip()
    for _, key in { "tape", "flaps", "foam", "lift" } do
        if self.on_task then
            self.on_task(key)
        end
    end
    self.tape.mode = "gone"
    self.tape.mesh.visible, self.tape.shadow.visible = false, false
    self:_finish()
end

-- Where the tape's tab is (for tests): a view point.
function Unbox:tape_tab(): (number, number)
    return Geom.to_screen(self.tape.face, 20, self.tape.face.height / 2, 0)
end

return Unbox
