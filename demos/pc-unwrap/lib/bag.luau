-- The bag: a sleeve of milky plastic pulled over the case from the top, its open end tucked under
-- the case. It comes off one way only: up and over the top, while the case stays on the desk (like
-- a sock pulled off a foot). Sideways pulls just make it lean.
--
-- Where you hold it matters. Held high, the whole bag slides up as one piece. Held low, the hand
-- lifts the hem and everything below the hand, while the plastic above it (pinned by friction and
-- the closed top) bunches up into folds; once it is bunched tight, pulling further slides the
-- whole bunch off.
--
-- The state is three numbers: `s`, how far the whole bag has slid up (cm); `c`, how far the part
-- below the fold point has been raised into the bunch; `g`, the fold point (cm of plastic from the
-- top rim). A point of plastic has fixed material coordinates (u around the case, v down from the
-- top rim) and sits at height
--   top - v * k + s           above the fold (v <= g), squeezed by k = (g - c) / g
--   top - v + c + s           below it
-- Squeezed plastic folds out in ridges; plastic pushed above the case gathers into a neck below the
-- closed end. The wall is one mesh running along the glass side, round the corner and along the
-- front, and over the top edge, so there are no seams. When the hem clears the top (s + c past
-- the wall's length) the bag is off: it hangs from the hand, then drops.

local Geom = require("lib/geom")

local sin, sqrt, min, max, exp, abs, PI = math.sin, math.sqrt, math.min, math.max, math.exp, math.abs, math.pi

local Bag = {}
Bag.__index = Bag

-- The bag around the case (cm): a little bigger than the case, the hem tucked under it.
local X0, X1, Z0, Z1 = -23.8, 23.8, -12.3, 12.3
local TOP, HEM = 49.9, -0.6
local LEN = TOP - HEM -- wall length, top rim to hem
local RC = 1.4 -- radius of the plastic round the vertical corner
local OVER = 3 -- how far the wall mesh runs over the top edge, inwards
-- The path round the case: the glass side (from the back to the corner), the corner, the front.
local SL = X1 - (X0 + RC)
local ARC = RC * PI / 2
local FL = Z1 - (Z0 + RC)
local U = SL + ARC + FL
local COLS, ROWS = 30, 36
local TOP_COLS, TOP_ROWS = 16, 8
local MAX_SQUEEZE = 0.85 -- the bunch can take up this much of the plastic above the hand

local function smooth(a: number, b: number, x: number): number
    local t = math.clamp((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)
end

-- How far the plastic `e` cm above the case top has gathered towards the middle (0..0.8).
local function gather(e: number): number
    if e <= 0 then
        return 0
    end
    return 0.8 * e / (e + 9)
end

-- The wall path at u: base point (x, z) and outward normal (nx, nz).
local function path(u: number): (number, number, number, number)
    if u <= SL then
        return X1 - u, Z0, 0, -1
    elseif u <= SL + ARC then
        local a = -PI / 2 - (u - SL) / RC
        local nx, nz = math.cos(a), math.sin(a)
        return X0 + RC + nx * RC, Z0 + RC + nz * RC, nx, nz
    end
    return X0, Z0 + RC + (u - SL - ARC), -1, 0
end

-- The prints' sizes in face units.
function Bag.sizes()
    local fu = Geom.FU
    return {
        wall = { U * fu, (LEN + OVER) * fu },
        top = { (X1 - X0) * fu, (Z1 - Z0) * fu },
    }
end

function Bag.new(parent, prints, z: number)
    local self = setmetatable({}, Bag)
    local shader = kulisa.load_shader("shaders/plastic.wgsl")
    local light, view = Geom.light, Geom.view
    local sizes = Bag.sizes()
    self.pieces = {}
    for name, p in { wall = { cols = COLS, rows = ROWS }, top = { cols = TOP_COLS, rows = TOP_ROWS } } do
        local n = (p.cols + 1) * (p.rows + 1)
        local w, h = sizes[name][1], sizes[name][2]
        local image = prints[name]
        local params = {
            light[1], light[2], light[3], 0.35,
            view[1], view[2], view[3], 0.09,
            0.86, 0.9, 1.0, 0.09,
            w, h, 1 + (if image then 8 else 0), 1,
        }
        self.pieces[name] = {
            cols = p.cols, rows = p.rows, n = n, params = params,
            mesh = kulisa.mesh({
                parent = parent, image = image, cols = p.cols, rows = p.rows, width = w, height = h,
                shader = shader, params = params, z = z + (if name == "top" then 0 else 1),
            }),
            pos = buffer.create(n * 8), dat = buffer.create(n * 16),
            X = table.create(n, 0), Y = table.create(n, 0), Z = table.create(n, 0),
            -- Which way is out at each vertex (normals are flipped to face it).
            OX = table.create(n, 0), OY = table.create(n, 0), OZ = table.create(n, 0),
        }
    end
    -- The wall's path, per column.
    self.bx, self.bz, self.nx, self.nz = {}, {}, {}, {}
    for col = 0, COLS do
        self.bx[col], self.bz[col], self.nx[col], self.nz[col] = path(U * col / COLS)
    end
    self:reset()
    return self
end

function Bag:reset()
    self.s, self.target = 0, 0
    self.c, self.c_target, self.g = 0, 0, LEN
    self.mode = nil
    self.held = false
    self.fling = false
    self.gone = false
    self.free = nil
    self.lean = 0
    self.speed = 0
    self.moved = 0
    self.asleep = false
    for _, p in self.pieces do
        p.mesh.visible = true
        p.mesh.x, p.mesh.y = 0, 0
        p.params[16] = 1
        p.mesh.params = p.params
    end
    self:_place()
end

-- Redraw now (after the PC moved or a reset).
function Bag:wake()
    self.asleep = false
    self:_place()
end

function Bag:contains(sx: number, sy: number): boolean
    if self.gone or self.free then
        return false
    end
    local b = self.bounds
    return b ~= nil and sx >= b[1] and sx <= b[2] and sy >= b[3] and sy <= b[4]
end

-- Stage units per centimetre straight up, for turning a drag into a pull.
local _, UP_Y = Geom.vec(0, 1, 0)
local UP = -UP_Y

-- The height (cm) of a stage point on the bag, read off the glass side's plane.
local function height_at(sx: number, sy: number): number
    local f = Geom.side
    local _, b = Geom.to_face(f, sx, sy)
    return 47.3 - b / Geom.FU
end

function Bag:begin_grab(sx: number, sy: number)
    self.held = true
    self.asleep = false
    self.gx, self.gy = sx, sy
    self.lx, self.ly = sx, sy
    self.g_s, self.g_c = self.target, self.c_target
    -- Below the bunch (or low on a smooth bag): lift the hem and bunch the plastic above the hand.
    -- High up, or on the bunch itself: slide the whole bag.
    local y = height_at(sx, sy)
    local s, c, g = self.s, self.c, self.g
    if c > 0.5 then
        local fold_y = TOP - (g - c) + s
        self.mode = if y < fold_y - 1 then "bunch" else "slide"
    elseif y < TOP + s - LEN * 0.4 then
        self.mode = "bunch"
        self.g = math.clamp(TOP + s - y, 8, LEN)
    else
        self.mode = "slide"
    end
end

function Bag:drag(sx: number, sy: number)
    -- Only the upward part of the drag pulls the bag; it cannot be pushed further down than it
    -- started, nor sideways.
    local d = (self.gy - sy) / UP
    if self.mode == "bunch" then
        local cmax = self.g * MAX_SQUEEZE
        local c = self.g_c + d
        self.c_target = math.clamp(c, 0, cmax)
        -- Bunched tight: the hand drags the whole bunch on up.
        self.target = self.g_s + max(0, c - cmax)
    else
        self.target = max(0, self.g_s + d)
        self.lean = math.clamp((sx - self.gx) / 140, -1, 1)
    end
    self.lx, self.ly = sx, sy
end

function Bag:release()
    self.held = false
    if self.free then
        self.free.held = false
        return
    end
    -- Flicked upwards or pulled most of the way: it keeps going and comes off.
    if self.speed > 500 or self.s + self.c > LEN * 0.6 then
        self.fling = true
    end
end

-- Pulled off: it hangs from the hand, then drops away.
function Bag:_come_off()
    self.free = { x = 0, y = 0, vx = 0, vy = -260, held = self.held, base_x = self.lx or 0, base_y = self.ly or 0 }
    if self.on_slide_off then
        self.on_slide_off(self.lean)
    end
end

function Bag:update(dt: number)
    if self.gone then
        return
    end
    if self.free then
        self:_free_step(dt)
        return
    end
    if self.asleep and not self.held then
        return
    end
    local before = self.s + self.c
    if self.fling then
        self.target = self.target + dt * (60 + self.s * 2)
    elseif not self.held then
        -- Let go part way: the bag slides back down over the case. A bunch stays bunched.
        self.target = max(0, self.target - dt * 22)
    end
    -- Friction against the case: the bag follows the hand with a little lag.
    local k = min(1, dt * 11)
    self.s += (self.target - self.s) * k
    self.c += (self.c_target - self.c) * k
    if not self.held then
        self.lean += (0 - self.lean) * min(1, dt * 6)
    end
    local now = self.s + self.c
    self.speed = (now - before) / max(dt, 1e-3) * UP
    self.moved += abs(now - before)
    if now >= LEN then
        self.fling = false
        self:_place()
        self:_come_off()
        return
    end
    self:_place()
    if not self.held and not self.fling and abs(self.s - self.target) < 0.01 and abs(self.c - self.c_target) < 0.01
        and abs(self.lean) < 0.01 then
        self.asleep = true
    end
end

function Bag:_free_step(dt: number)
    local f = self.free
    if f.held then
        local nx, ny = self.lx, self.ly
        f.vx, f.vy = (nx - (f.px or nx)) / max(dt, 1e-3), (ny - (f.py or ny)) / max(dt, 1e-3)
        f.px, f.py = nx, ny
        f.x, f.y = nx - f.base_x, ny - f.base_y
    else
        -- Falls: gravity and a little air drag; fades as it drops out of view.
        f.vy += 1500 * dt
        f.vx *= exp(-1.5 * dt)
        f.x += f.vx * dt
        f.y += f.vy * dt
        f.fall = (f.fall or 0) + dt
    end
    local fade = 1 - smooth(0.5, 1.1, f.fall or 0)
    for _, p in self.pieces do
        p.params[16] = fade
        p.mesh.params = p.params
        p.mesh.x, p.mesh.y = f.x, f.y
    end
    if fade <= 0 then
        self.gone = true
        for _, p in self.pieces do
            p.mesh.visible = false
        end
        if self.on_gone then
            self.on_gone()
        end
    end
end

-- Material height field (cm): loose plastic puffing off the case, creases that run mostly down
-- the bag, the hem's flare, folds where the neck gathers it. u, v in cm.
local function height(u: number, v: number, e: number): number
    local h = 0.25 + 0.22 * sin(u * 0.55 + sin(v * 0.21) * 1.7) * sin(v * 0.17 + u * 0.13)
        + 0.14 * sin(u * 1.7 + v * 0.35 + 1.3) + 0.08 * sin(u * 3.1 - v * 0.6)
    h += smooth(LEN - 5, LEN, v) * (0.9 + 0.5 * sin(u * 0.8))
    if e > 0 then
        h += gather(e) * 2.4 * (0.5 + 0.5 * sin(u * 1.3 + e * 0.2))
    end
    return h
end

function Bag:_place()
    local s, c, g, lean = self.s, self.c, self.g, self.lean
    local k = (g - c) / g
    -- Squeezed plastic folds out: ridges as high as the squeeze allows (an accordion).
    local fold = 1.6 * sqrt(max(0, 1 - k * k))
    local minx, maxx, miny, maxy = 1e9, -1e9, 1e9, -1e9
    local project = Geom.project
    local bxs, bzs, nxs, nzs = self.bx, self.bz, self.nx, self.nz
    local wall = self.pieces.wall
    local X, Y, Z, OX, OY, OZ = wall.X, wall.Y, wall.Z, wall.OX, wall.OY, wall.OZ
    local pos = wall.pos
    local rim_g = gather(s)
    for r = 0, ROWS do
        local v = (LEN + OVER) * r / ROWS - OVER
        for col = 0, COLS do
            local i = r * (COLS + 1) + col + 1
            local u = U * col / COLS
            local bx, bz, nx, nz = bxs[col], bzs[col], nxs[col], nzs[col]
            local x, y, z, e
            if v < 0 then
                -- Over the top edge: lying on the top, moving in from the rim.
                local h = 0.3 + 0.15 * sin(u * 0.9)
                x, z = bx - nx * (-v), bz - nz * (-v)
                x, z = x * (1 - rim_g), z * (1 - rim_g)
                y = TOP + h + s * 0.93
                e = s
                OX[i], OY[i], OZ[i] = 0, 1, 0
            else
                y = if v <= g then TOP - v * k + s else TOP - v + c + s
                e = y - TOP
                local h = height(u, v, e)
                if v <= g and fold > 0 then
                    -- Ridges every 3 cm of plastic, a little skewed round the bag.
                    local ph = v * PI / 3 + 0.35 * sin(u * 0.4)
                    h += fold * (0.35 + abs(sin(ph))) * (0.65 + 0.35 * sin(u * 0.23 + v * 0.7))
                end
                x, z = bx + nx * h, bz + nz * h
                if e > 0 then
                    local ga = gather(e)
                    x, z = x * (1 - ga), z * (1 - ga)
                    y = TOP + e * 0.93
                end
                OX[i], OY[i], OZ[i] = nx, 0, nz
            end
            X[i], Y[i], Z[i] = x, y, z
            local sx, sy = project(x, y, z)
            if e > 0 then
                -- A sideways pull leans the part above the case towards the hand.
                sx += lean * 26 * e / (e + 8)
            end
            buffer.writef32(pos, (i - 1) * 8, sx)
            buffer.writef32(pos, (i - 1) * 8 + 4, sy)
            minx, maxx = min(minx, sx), max(maxx, sx)
            miny, maxy = min(miny, sy), max(maxy, sy)
        end
    end
    -- The closed end: lies on the top, gathered and lifted with the neck.
    local top = self.pieces.top
    local TX, TY, TZ = top.X, top.Y, top.Z
    for r = 0, TOP_ROWS do
        for col = 0, TOP_COLS do
            local i = r * (TOP_COLS + 1) + col + 1
            local bx = X1 - (X1 - X0) * col / TOP_COLS
            local bz = Z1 - (Z1 - Z0) * r / TOP_ROWS
            local h = 0.35 + 0.2 * sin(bx * 0.5 + bz * 0.7) + rim_g * 1.5
            local x, z = bx * (1 - rim_g), bz * (1 - rim_g)
            local y = TOP + h + s * 0.93
            TX[i], TY[i], TZ[i] = x, y, z
            top.OX[i], top.OY[i], top.OZ[i] = 0, 1, 0
            local sx, sy = project(x, y, z)
            sx += lean * 26 * s / (s + 8)
            buffer.writef32(top.pos, (i - 1) * 8, sx)
            buffer.writef32(top.pos, (i - 1) * 8 + 4, sy)
        end
    end
    for _, p in self.pieces do
        self:_normals(p)
        p.mesh:set_positions(p.pos)
        p.mesh:set_data(p.dat)
    end
    self.bounds = { minx, maxx, miny, maxy }
end

-- Normals from neighbouring points (in case space, then to camera space for the shader).
function Bag:_normals(p)
    local cols, rows = p.cols, p.rows
    local X, Y, Z, OX, OY, OZ = p.X, p.Y, p.Z, p.OX, p.OY, p.OZ
    local w1 = cols + 1
    local X3, Y3, Z3 = Geom.X3, Geom.Y3, Geom.Z3
    for r = 0, rows do
        for c = 0, cols do
            local i = r * w1 + c + 1
            local il = if c > 0 then i - 1 else i
            local ir = if c < cols then i + 1 else i
            local iu = if r > 0 then i - w1 else i
            local id = if r < rows then i + w1 else i
            local ax, ay, az = X[ir] - X[il], Y[ir] - Y[il], Z[ir] - Z[il]
            local bx, by, bz = X[id] - X[iu], Y[id] - Y[iu], Z[id] - Z[iu]
            local nx, ny, nz = ay * bz - az * by, az * bx - ax * bz, ax * by - ay * bx
            local l = sqrt(nx * nx + ny * ny + nz * nz)
            local ox, oy, oz = OX[i], OY[i], OZ[i]
            if l < 1e-6 then
                nx, ny, nz, l = ox, oy, oz, 1
            end
            nx, ny, nz = nx / l, ny / l, nz / l
            if nx * ox + ny * oy + nz * oz < 0 then
                nx, ny, nz = -nx, -ny, -nz
            end
            local o16 = (i - 1) * 16
            buffer.writef32(p.dat, o16, nx * X3[1] + ny * Y3[1] + nz * Z3[1])
            buffer.writef32(p.dat, o16 + 4, nx * X3[2] + ny * Y3[2] + nz * Z3[2])
            buffer.writef32(p.dat, o16 + 8, nx * X3[3] + ny * Y3[3] + nz * Z3[3])
            buffer.writef32(p.dat, o16 + 12, 0)
        end
    end
end

-- How far the bag is off, 0..1.
function Bag:progress(): number
    if self.gone or self.free then
        return 1
    end
    return (self.s + self.c) / LEN
end

Bag.LENGTH = LEN

return Bag
