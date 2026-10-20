-- The bag: a sleeve of milky plastic pulled over the case from the top, its open end tucked under
-- the case. It comes off one way only: up and over the top, while the case stays on the desk and
-- leaves through the opening (like a sock pulled off a foot). Dragging moves it along that path
-- only; sideways pulls just make it lean.
--
-- The state is one number, `s`: how far the bag has been pulled up (cm). Every point of the bag
-- has fixed material coordinates (u around, v down from the top rim) and is placed from s each
-- frame: wall material still beside the case lies on the case's wall at height top - v + s; the
-- material above the case gathers into a narrowing neck below the closed end, and the hem (the
-- open end) climbs up the case. Wrinkles come from a height field in material coordinates, so they
-- travel with the plastic; the neck adds folds where the plastic bunches. Past s = the wall's
-- length, the hem clears the top and the bag is off: it hangs from the hand, then drops.

local Geom = require("lib/geom")

local sin, sqrt, min, max, exp, abs = math.sin, math.sqrt, math.min, math.max, math.exp, math.abs

local Bag = {}
Bag.__index = Bag

-- The bag around the case (cm): a little bigger than the case, the hem tucked under it.
local X0, X1, Z0, Z1 = -23.8, 23.8, -12.3, 12.3
local TOP, HEM = 49.9, -0.6
local LEN = TOP - HEM -- wall length, top rim to hem

-- Pieces: the glass-side wall, the front wall and the closed top. Grid sizes and extents.
local PIECES = {
    side = { cols = 20, rows = 22 },
    front = { cols = 10, rows = 22 },
    top = { cols = 20, rows = 10 },
}

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

-- The pieces' sizes in face units (for their prints).
function Bag.sizes()
    local fu = Geom.FU
    return {
        side = { (X1 - X0) * fu, LEN * fu },
        front = { (Z1 - Z0) * fu, LEN * fu },
        top = { (X1 - X0) * fu, (Z1 - Z0) * fu },
    }
end

function Bag.new(parent, prints, z: number)
    local self = setmetatable({}, Bag)
    local shader = kulisa.load_shader("shaders/plastic.wgsl")
    local light, view = Geom.light, Geom.view
    local sizes = Bag.sizes()
    self.pieces = {}
    for name, p in PIECES do
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
            X = table.create(n, 0), Y = table.create(n, 0), Z = table.create(n, 0), H = table.create(n, 0),
        }
    end
    self:reset()
    return self
end

function Bag:reset()
    self.s, self.target = 0, 0
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

function Bag:begin_grab(sx: number, sy: number)
    self.held = true
    self.asleep = false
    self.gx, self.gy = sx, sy
    self.g_s = self.target
    self.lx, self.ly = sx, sy
end

function Bag:drag(sx: number, sy: number)
    -- Only the upward part of the drag pulls the bag; it cannot be pushed further down than it
    -- started, nor sideways.
    self.target = max(0, self.g_s + (self.gy - sy) / UP)
    self.lean = math.clamp((sx - self.gx) / 140, -1, 1)
    self.lx, self.ly = sx, sy
end

function Bag:release()
    self.held = false
    if self.free then
        self.free.held = false
        return
    end
    -- Flicked upwards or pulled most of the way: it keeps going and comes off.
    if self.speed > 500 or self.s > LEN * 0.6 then
        self.fling = true
    end
end

-- Pulled off: it hangs from the hand, then drops away.
function Bag:_come_off()
    -- While held it moves with the hand from where it came off; flung, it rises a little first.
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
    local before = self.s
    if self.fling then
        self.target = self.target + dt * (60 + self.s * 2)
    elseif not self.held then
        -- Let go part way: the plastic above the case has nothing to hold it and slides back down.
        self.target = max(0, self.target - dt * 22)
    end
    -- Friction against the case: the bag follows the hand with a little lag.
    self.s += (self.target - self.s) * min(1, dt * 11)
    if not self.held then
        self.lean += (0 - self.lean) * min(1, dt * 6)
    end
    self.speed = (self.s - before) / max(dt, 1e-3) * UP
    self.moved += abs(self.s - before)
    if self.s >= LEN then
        self.s = LEN
        self.fling = false
        self:_place()
        self:_come_off()
        return
    end
    self:_place()
    if not self.held and not self.fling and abs(self.s - self.target) < 0.01 and abs(self.lean) < 0.01 then
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
-- the bag, and folds where the neck gathers it. u, v in cm.
local function height(u: number, v: number, e: number): number
    local h = 0.25 + 0.22 * sin(u * 0.55 + sin(v * 0.21) * 1.7) * sin(v * 0.17 + u * 0.13)
        + 0.14 * sin(u * 1.7 + v * 0.35 + 1.3) + 0.08 * sin(u * 3.1 - v * 0.6)
    -- The hem flares out and waves.
    local hem = smooth(LEN - 5, LEN, v)
    h += hem * (0.9 + 0.5 * sin(u * 0.8))
    -- Gathered plastic folds up.
    if e > 0 then
        h += gather(e) * 2.4 * (0.5 + 0.5 * sin(u * 1.3 + e * 0.2))
    end
    return h
end

-- Places a wall point: material u (cm along the wall), v (cm down from the top rim). `wall`
-- gives the wall's line round the case: base (x, z) at u, and its outward normal (nx, nz).
local function wall_point(s: number, bx: number, bz: number, nx: number, nz: number, u: number, v: number)
    local y = TOP - v + s
    local e = y - TOP
    local h = height(u, v, e)
    local x, z = bx + nx * h, bz + nz * h
    if e > 0 then
        local g = gather(e)
        x, z = x * (1 - g), z * (1 - g)
        y = TOP + e * 0.93
    end
    return x, y, z, e, h
end

function Bag:_place()
    local s = self.s
    local lean = self.lean
    local minx, maxx, miny, maxy = 1e9, -1e9, 1e9, -1e9
    local project = Geom.project
    for name, p in self.pieces do
        local cols, rows = p.cols, p.rows
        local X, Y, Z, H = p.X, p.Y, p.Z, p.H
        for r = 0, rows do
            for c = 0, cols do
                local i = r * (cols + 1) + c + 1
                local x, y, z, e
                if name == "side" then
                    local u = (X1 - X0) * c / cols
                    local v = LEN * r / rows
                    x, y, z, e, H[i] = wall_point(s, X1 - u, Z0, 0, -1, u, v)
                elseif name == "front" then
                    local u = (Z1 - Z0) * c / cols
                    local v = LEN * r / rows
                    x, y, z, e, H[i] = wall_point(s, X0, Z0 + u, -1, 0, u + 60, v)
                else
                    -- The closed end: lies on the top, gathered and lifted with the neck.
                    local bx = X1 - (X1 - X0) * c / cols
                    local bz = Z1 - (Z1 - Z0) * r / rows
                    local g = gather(s)
                    local h = 0.35 + 0.2 * sin(bx * 0.5 + bz * 0.7) + g * 1.5
                    x, z = bx * (1 - g), bz * (1 - g)
                    y = TOP + h + s * 0.93
                    e = s
                    H[i] = h
                end
                X[i], Y[i], Z[i] = x, y, z
                local sx, sy = project(x, y, z)
                -- A sideways pull leans the gathered part towards the hand.
                if e > 0 then
                    sx += lean * 26 * e / (e + 8)
                end
                local o8 = (i - 1) * 8
                buffer.writef32(p.pos, o8, sx)
                buffer.writef32(p.pos, o8 + 4, sy)
                minx, maxx = min(minx, sx), max(maxx, sx)
                miny, maxy = min(miny, sy), max(maxy, sy)
            end
        end
        self:_normals(name, p)
        p.mesh:set_positions(p.pos)
        p.mesh:set_data(p.dat)
    end
    self.bounds = { minx, maxx, miny, maxy }
end

-- Normals from neighbouring points (in case space, then to camera space for the shader).
function Bag:_normals(name: string, p)
    local cols, rows = p.cols, p.rows
    local X, Y, Z = p.X, p.Y, p.Z
    local w1 = cols + 1
    local X3, Y3, Z3 = Geom.X3, Geom.Y3, Geom.Z3
    -- Which way is out: the side faces -z, the front -x, the top +y.
    local ox, oy, oz = 0, 0, -1
    if name == "front" then
        ox, oy, oz = -1, 0, 0
    elseif name == "top" then
        ox, oy, oz = 0, 1, 0
    end
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
    return self.s / LEN
end

Bag.LENGTH = LEN

return Bag
