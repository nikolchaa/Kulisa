-- The bag: a loose sheet of translucent plastic lying over the case, simulated as cloth. You grab
-- it anywhere and drag; the cloth follows with some lag, bunches where it is pushed together and
-- lifts into a tent around your hand. Slide it far enough and it slips off by itself.
--
-- The simulation is position-based (Verlet): positions only, distance constraints between grid
-- neighbours, and heavy damping for the friction against the case. Heights come from how much
-- the cloth is compressed, and go to the plastic shader as normals through the mesh's `data`.

local Geom = require("lib/geom")

local exp, sqrt, abs, min, max = math.exp, math.sqrt, math.abs, math.min, math.max

local Bag = {}
Bag.__index = Bag

local COLS, ROWS = 16, 10
local X0, Y0, W, H = 140, 100, 1000, 660

function Bag.new(parent, image, z: number)
    local self = setmetatable({}, Bag)
    local n = (COLS + 1) * (ROWS + 1)
    self.n = n
    self.hx, self.hy = W / COLS, H / ROWS
    local face = Geom.glass
    local light, view = face.light, face.view
    self.params = {
        light[1], light[2], light[3], 0.3,
        view[1], view[2], view[3], 0.2,
        0.85, 0.9, 1.0, 0.1,
        W, H, 1 + 8, 1,
    }
    self.mesh = kulisa.mesh({
        parent = parent, image = image, cols = COLS, rows = ROWS, width = W, height = H,
        shader = kulisa.load_shader("shaders/plastic.wgsl"), params = self.params, z = z,
    })
    self.X, self.Y, self.PX, self.PY = table.create(n), table.create(n), table.create(n), table.create(n)
    self.Hh, self.comp, self.gw = table.create(n, 0), table.create(n, 0), table.create(n, 0)
    self.pos = buffer.create(n * 8)
    self.dat = buffer.create(n * 16)
    -- Constraint lists: horizontal and vertical neighbours.
    self.ca, self.cb, self.cl, self.ck = {}, {}, {}, {}
    for r = 0, ROWS do
        for c = 0, COLS do
            local i = r * (COLS + 1) + c + 1
            if c < COLS then
                table.insert(self.ca, i)
                table.insert(self.cb, i + 1)
                table.insert(self.cl, self.hx)
                table.insert(self.ck, 1)
            end
            if r < ROWS then
                table.insert(self.ca, i)
                table.insert(self.cb, i + COLS + 1)
                table.insert(self.cl, self.hy)
                table.insert(self.ck, 1)
            end
            -- Diagonals keep cells from collapsing (and the outline from zigzagging).
            if c < COLS and r < ROWS then
                local diag = sqrt(self.hx * self.hx + self.hy * self.hy)
                table.insert(self.ca, i)
                table.insert(self.cb, i + COLS + 2)
                table.insert(self.cl, diag)
                table.insert(self.ck, 0.3)
                table.insert(self.ca, i + 1)
                table.insert(self.cb, i + COLS + 1)
                table.insert(self.cl, diag)
                table.insert(self.ck, 0.3)
            end
        end
    end
    self:reset()
    return self
end

function Bag:reset()
    for r = 0, ROWS do
        for c = 0, COLS do
            local i = r * (COLS + 1) + c + 1
            local x, y = X0 + W * c / COLS, Y0 + H * r / ROWS
            self.X[i], self.Y[i], self.PX[i], self.PY[i] = x, y, x, y
            self.Hh[i], self.comp[i], self.gw[i] = 0, 0, 0
        end
    end
    self.held = false
    self.gone = false
    self.glide = 0
    self.gdx, self.gdy = 0, 0
    self.mesh.visible = true
    self.params[16] = 1
    self.mesh.params = self.params
    self.moved = 0
    self.nx, self.ny = nil, nil
    self.asleep = false
    self:_write()
end

function Bag:contains(sx: number, sy: number): boolean
    if self.gone then
        return false
    end
    -- The cloth's bounding box is a good enough hit area (it covers the case anyway).
    local X, Y = self.X, self.Y
    local minx, maxx, miny, maxy = 1e9, -1e9, 1e9, -1e9
    for i = 1, self.n do
        minx, maxx = min(minx, X[i]), max(maxx, X[i])
        miny, maxy = min(miny, Y[i]), max(maxy, Y[i])
    end
    return sx >= minx and sx <= maxx and sy >= miny and sy <= maxy
end

function Bag:begin_grab(sx: number, sy: number)
    self.held = true
    self.asleep = false
    self.lx, self.ly = sx, sy
    self.nx, self.ny = sx, sy
    self.glide = 0
    -- Influence of the hand on each vertex: a soft blob around where it landed.
    local X, Y, gw = self.X, self.Y, self.gw
    local sigma2 = 2 * 120 * 120
    for i = 1, self.n do
        local dx, dy = X[i] - sx, Y[i] - sy
        gw[i] = exp(-(dx * dx + dy * dy) / sigma2)
    end
end

function Bag:drag(sx: number, sy: number)
    self.nx, self.ny = sx, sy
end

function Bag:release()
    self.held = false
end

local function extents(self)
    local X, Y = self.X, self.Y
    local minx, maxx, miny, maxy = 1e9, -1e9, 1e9, -1e9
    for i = 1, self.n do
        minx, maxx = min(minx, X[i]), max(maxx, X[i])
        miny, maxy = min(miny, Y[i]), max(maxy, Y[i])
    end
    return minx, maxx, miny, maxy
end

-- How far the bag has slid towards clearing the case, 0..1 (the best direction).
function Bag:progress(): number
    local minx, maxx, miny, maxy = extents(self)
    local o = 28 -- margin beyond the case
    local ox0, ox1 = Geom.OX - o, Geom.OX + 760 + Geom.DEPTH_X + o
    local oy0, oy1 = Geom.OY + Geom.DEPTH_Y - o, Geom.OY + 440 + o
    local left = (X0 + W - maxx) / ((X0 + W) - ox0)
    local right = (minx - X0) / (ox1 - X0)
    local up = (Y0 + H - maxy) / ((Y0 + H) - oy0)
    local down = (miny - Y0) / (oy1 - Y0)
    return max(max(left, right), max(up, down))
end

function Bag:update(dt: number)
    if self.gone then
        return
    end
    if self.asleep and not self.held then
        return
    end
    local n, X, Y, PX, PY, gw, comp = self.n, self.X, self.Y, self.PX, self.PY, self.gw, self.comp
    -- The hand drags the vertices near it (a position change, so the cloth picks up velocity).
    local dx, dy = 0, 0
    if self.held and self.nx then
        dx, dy = self.nx - self.lx, self.ny - self.ly
        self.lx, self.ly = self.nx, self.ny
        self.moved += sqrt(dx * dx + dy * dy)
    end
    self.last_move = sqrt(dx * dx + dy * dy) / max(dt, 1e-3)
    local damp = 0.78 ^ (dt * 60) -- friction against the case, independent of the frame rate
    local held = self.held
    local gl = if self.glide > 0 then self.glide * 24 else 0
    local gx, gy = self.gdx * gl, self.gdy * gl
    for i = 1, n do
        local vx, vy = (X[i] - PX[i]) * damp, (Y[i] - PY[i]) * damp
        local w = if held then 0.5 + gw[i] * 0.48 else 0
        -- The hand moves vertices directly and does not give them momentum.
        local ox, oy = X[i] + dx * w, Y[i] + dy * w
        PX[i], PY[i] = ox, oy
        X[i] = ox + vx + gx
        Y[i] = oy + vy + gy
    end
    -- Distance constraints, a few relaxation passes. Compression is remembered as wrinkles.
    for i = 1, n do
        comp[i] = 0
    end
    local ca, cb, cl, ck = self.ca, self.cb, self.cl, self.ck
    local count = #ca
    for pass = 1, 4 do
        for k = 1, count do
            local a, b = ca[k], cb[k]
            local ex, ey = X[b] - X[a], Y[b] - Y[a]
            local d = sqrt(ex * ex + ey * ey)
            local rest = cl[k]
            if d > 1e-4 then
                -- Stretching is fought harder than compression: cloth bunches up but does not stretch.
                local diff = (d - rest) / d * ck[k] * (if d > rest then 0.5 else 0.22)
                local mx, my = ex * diff, ey * diff
                X[a] += mx
                Y[a] += my
                X[b] -= mx
                Y[b] -= my
                if pass == 4 and d < rest then
                    local c = (rest - d) / rest
                    comp[a] += c
                    comp[b] += c
                end
            end
        end
    end
    -- Slide off on its own once far enough: the bag glides away in the direction it was moved.
    if self.glide == 0 then
        local prog = self:progress()
        if prog > 0.36 and (not held or prog > 0.6) then
            local cx, cy = 0, 0
            for i = 1, n do
                cx += X[i] - (X0 + W * ((i - 1) % (COLS + 1)) / COLS)
                cy += Y[i] - (Y0 + H * ((i - 1) // (COLS + 1)) / ROWS)
            end
            local l = sqrt(cx * cx + cy * cy)
            self.gdx, self.gdy = cx / max(l, 1), cy / max(l, 1)
            self.glide = 0.01
            if self.on_slide_off then
                self.on_slide_off(self.gdx)
            end
        end
    else
        self.glide += dt * 1.6
        local fade = max(0, 1 - max(0, self.glide - 0.35) * 1.5)
        self.params[16] = fade
        self.mesh.params = self.params
        if fade <= 0 then
            self.gone = true
            self.mesh.visible = false
            if self.on_gone then
                self.on_gone()
            end
            return
        end
    end
    -- Settled: stop simulating until it is touched again.
    local moving = 0
    for i = 1, n do
        moving += abs(X[i] - PX[i]) + abs(Y[i] - PY[i])
    end
    self:_write()
    if not held and self.glide == 0 and moving < 0.02 * n then
        self.asleep = true
    end
end

function Bag:_write()
    local n, X, Y, Hh, comp, gw = self.n, self.X, self.Y, self.Hh, self.comp, self.gw
    local face = Geom.glass
    local Zx, Zy = face.zx, face.zy
    local tent = if self.held then 20 else 0
    for i = 1, n do
        local target = min(comp[i] * 70, 26) + tent * gw[i]
        Hh[i] += (target - Hh[i]) * 0.35
    end
    local pos, dat = self.pos, self.dat
    local w1 = COLS + 1
    local inv = 1 / (2 * self.hx)
    local invy = 1 / (2 * self.hy)
    for r = 0, ROWS do
        for c = 0, COLS do
            local i = r * w1 + c + 1
            local l = if c > 0 then Hh[i - 1] else Hh[i]
            local rr = if c < COLS then Hh[i + 1] else Hh[i]
            local u = if r > 0 then Hh[i - w1] else Hh[i]
            local d = if r < ROWS then Hh[i + w1] else Hh[i]
            local nx, ny = -(rr - l) * inv * 6, -(d - u) * invy * 6
            local il = 1 / sqrt(nx * nx + ny * ny + 1)
            local ao = min(0.3, comp[i] * 0.6)
            local k = (1 - ao) * il
            local z = Hh[i]
            local o8, o16 = (i - 1) * 8, (i - 1) * 16
            buffer.writef32(pos, o8, X[i] + z * Zx)
            buffer.writef32(pos, o8 + 4, Y[i] + z * Zy)
            buffer.writef32(dat, o16, nx * k)
            buffer.writef32(dat, o16 + 4, ny * k)
            buffer.writef32(dat, o16 + 8, k)
        end
    end
    self.mesh:set_positions(pos)
    self.mesh:set_data(dat)
end

return Bag
