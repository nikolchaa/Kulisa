-- Applying a fresh protector film: it lands on the glass with air bubbles trapped under it, and a
-- squeegee dragged over it presses the film down and pushes the bubbles ahead of the blade, out
-- to the edges, where they escape.
--
-- It reuses a Film's mesh (a 48x48 grid) and shader. Here the film never peels; its height comes
-- from the landing, from a faint floating wave that the squeegee irons out, and from the bubbles,
-- each a smooth lens h = hb (1 - (d/r)^2)^2 whose slope gives the normals the shader lights.

local Geom = require("lib/geom")

local sin, cos, sqrt, abs, min, max, exp, floor, ceil = math.sin, math.cos, math.sqrt, math.abs, math.min, math.max, math.exp, math.floor, math.ceil

local Protector = {}
Protector.__index = Protector

local BLADE_HALF = 90 -- half length of the squeegee blade, stage px

local function smoothstep(a: number, b: number, x: number): number
    local t = math.clamp((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)
end

function Protector.new(film, parent, z: number)
    local self = setmetatable({}, Protector)
    self.film = film
    film.mode = "apply"
    film.mesh.visible, film.shadow.visible = false, false
    local n = film.n
    self.H, self.AIR, self.press, self.L0 = table.create(n, 0), table.create(n, 0), table.create(n, 0), table.create(n, 0)
    local o = film.o
    for r = 0, film.rows do
        for c = 0, film.cols do
            local i = r * (film.cols + 1) + c + 1
            local a, b = c / film.cols * o.width, r / film.rows * o.height
            -- A gentle floating wave: the film is not yet stuck to the glass.
            self.L0[i] = 5 * (sin(a * 0.021 + 0.7) * cos(b * 0.027) + 0.6 * sin(b * 0.041 + a * 0.013))
        end
    end
    self.hx, self.hy = o.width / film.cols, o.height / film.rows

    -- The squeegee: a card with a rubber blade, drawn by a shader.
    self.tool = kulisa.sprite({
        parent = parent, width = BLADE_HALF * 2 + 20, height = 64, center = true, z = z, visible = false,
        shader = kulisa.load_shader("shaders/squeegee.wgsl"),
        params = { BLADE_HALF * 2 + 20, 64, 0, 0 },
    })
    self.phase = "idle" -- idle | landing | ready | done
    self.bubbles = {}
    self.held = false
    return self
end

function Protector:reset()
    self.phase = "idle"
    self.film.mesh.visible, self.film.shadow.visible = false, false
    self.tool.visible = false
    self.bubbles = {}
    self.held = false
    self.coverage = 0
end

-- The film drops onto the glass; bubbles form as it settles.
function Protector:start()
    local film = self.film
    local o = film.o
    for i = 1, film.n do
        self.press[i] = 0
    end
    self.bubbles = {}
    self.t = 0
    self.phase = "landing"
    self.coverage = 0
    self.lift = 1
    film.params[16] = 1
    film.mesh.params = film.params
    film.mesh.visible, film.shadow.visible = true, true
    -- A scatter of bubbles of different sizes, kept off the very edges.
    local count = 11
    for _ = 1, count do
        local r = 14 + math.random() ^ 1.6 * 40
        local a = r + 18 + math.random() * (o.width - 2 * r - 36)
        local b = r + 18 + math.random() * (o.height - 2 * r - 36)
        table.insert(self.bubbles, { a = a + o.a0, b = b + o.b0, r = r, grow = -0.3 - 0.4 * math.random(), dying = 0 })
    end
    self.dirty = true
    self.moved_total = 0
end

function Protector:active(): boolean
    return self.phase == "ready"
end

-- Is the point over the film (so a press starts the squeegee)?
function Protector:can_grab(sx: number, sy: number): boolean
    if self.phase ~= "ready" then
        return false
    end
    local f = self.film
    local a, b = Geom.to_face(f.face, sx, sy)
    local o = f.o
    return a >= o.a0 - 20 and a <= o.a0 + o.width + 20 and b >= o.b0 - 20 and b <= o.b0 + o.height + 20
end

function Protector:begin_grab(sx: number, sy: number)
    self.held = true
    self.px, self.py = sx, sy
    self.nx, self.ny = sx, sy
    self.dx, self.dy = 1, 0
    self.have_dir = false
    self.tool.visible = true
    self.tool.x, self.tool.y = sx, sy
    self.speed = 0
end

function Protector:drag(sx: number, sy: number)
    self.nx, self.ny = sx, sy
end

function Protector:release()
    self.held = false
    self.tool.visible = false
end

-- The squeegee moved from (x0, y0) to (x1, y1): iron the film flat under the blade and push the
-- bubbles it meets.
function Protector:_sweep(x0: number, y0: number, x1: number, y1: number)
    local mx, my = x1 - x0, y1 - y0
    local m = sqrt(mx * mx + my * my)
    if m < 0.6 then
        return
    end
    local dx, dy = mx / m, my / m
    if self.have_dir then
        dx, dy = self.dx + (dx - self.dx) * 0.3, self.dy + (dy - self.dy) * 0.3
        local l = sqrt(dx * dx + dy * dy)
        dx, dy = dx / l, dy / l
    end
    self.dx, self.dy, self.have_dir = dx, dy, true
    local lx, ly = -dy, dx
    local film = self.film
    local face = film.face
    local o = film.o
    -- Press the film down under the blade.
    local BX, BY, press = film.BX, film.BY, self.press
    local covered = 0
    for i = 1, film.n do
        local rx, ry = BX[i] - x1, BY[i] - y1
        local along = rx * dx + ry * dy
        local across = rx * lx + ry * ly
        if abs(across) <= BLADE_HALF and along <= 10 and along >= -m - 14 then
            press[i] = 1
        end
        covered += press[i]
    end
    self.coverage = covered / film.n
    -- Push bubbles ahead of the blade, a little sideways away from its middle.
    local pushed = 0
    local bubbles = self.bubbles
    for k = #bubbles, 1, -1 do
        local bub = bubbles[k]
        if bub.dying == 0 and bub.grow >= 0 then
            local sx, sy = Geom.to_screen(face, bub.a, bub.b, 0)
            local rx, ry = sx - x1, sy - y1
            local along = rx * dx + ry * dy
            local across = rx * lx + ry * ly
            local reach = bub.r * 0.6
            if abs(across) <= BLADE_HALF + bub.r * 0.4 and along < reach and along > -m - bub.r * 0.9 then
                local want = reach - along
                local push = min(want, m * 1.7 + 3)
                local slide = across / BLADE_HALF * push * 0.35
                local nsx, nsy = sx + dx * push + lx * slide, sy + dy * push + ly * slide
                bub.a, bub.b = Geom.to_face(face, nsx, nsy)
                bub.r = max(6, bub.r * (1 - 0.0016 * push))
                pushed += 1
            end
        end
    end
    -- Bubbles that reach an edge escape.
    for k = #bubbles, 1, -1 do
        local bub = bubbles[k]
        if bub.dying == 0 and bub.grow >= 0 then
            local da, db = bub.a - o.a0, bub.b - o.b0
            local edge = min(min(da, o.width - da), min(db, o.height - db))
            if edge < bub.r * 0.35 then
                bub.dying = 0.001
                if self.on_pop then
                    local sx = Geom.to_screen(face, bub.a, bub.b, 0)
                    self.on_pop(math.clamp(bub.r / 50, 0.1, 1), sx)
                end
            end
        end
    end
    -- Two bubbles that meet merge into one bigger one.
    for i = #bubbles, 1, -1 do
        for j = i - 1, 1, -1 do
            local p, q = bubbles[i], bubbles[j]
            if p and q and p.dying == 0 and q.dying == 0 and p.grow >= 0 and q.grow >= 0 then
                local da, db = p.a - q.a, p.b - q.b
                if sqrt(da * da + db * db) < (p.r + q.r) * 0.5 then
                    local ar, br = p.r * p.r, q.r * q.r
                    q.a, q.b = (p.a * ar + q.a * br) / (ar + br), (p.b * ar + q.b * br) / (ar + br)
                    q.r = min(60, sqrt(ar + br) * 0.96)
                    table.remove(bubbles, i)
                    break
                end
            end
        end
    end
    self.pushed = pushed
    self.dirty = true
end

function Protector:update(dt: number)
    local film = self.film
    if self.phase == "idle" then
        return
    end
    self.t += dt
    local landing_t = 0.75
    if self.phase == "landing" and self.t >= landing_t then
        self.phase = "ready"
        self.dirty = true
    end
    -- Bubbles grow in after the film lands, and shrink away when they escape.
    for k = #self.bubbles, 1, -1 do
        local bub = self.bubbles[k]
        if bub.dying > 0 then
            bub.dying += dt
            self.dirty = true
            if bub.dying > 0.18 then
                table.remove(self.bubbles, k)
            end
        elseif bub.grow < 0 then
            bub.grow = min(0, bub.grow + dt * 1.5)
            if self.t > landing_t - 0.2 then
                bub.grow = max(0, bub.grow)
            end
            self.dirty = true
        end
    end
    if self.held and self.nx then
        local moved_x, moved_y = self.nx - self.px, self.ny - self.py
        local m = sqrt(moved_x * moved_x + moved_y * moved_y)
        self.speed += (m / max(dt, 1e-3) - self.speed) * min(1, dt * 12)
        if m > 0.6 then
            self:_sweep(self.px, self.py, self.nx, self.ny)
            -- The blade turns to face where it is going.
            local ang = math.atan2(self.dx, -self.dy)
            self.tool.rotation = ang
            self.tool.x, self.tool.y = self.nx, self.ny
            self.px, self.py = self.nx, self.ny
            if self.on_move then
                self.on_move(self.speed, self.nx, self.pushed or 0)
            end
        end
    end
    if self.phase == "ready" and not self.finished and #self.bubbles == 0 and self.coverage > 0.9 then
        self.finished = true
        self.phase = "done"
        self.tool.visible = false
        self.held = false
        if self.on_done then
            self.on_done()
        end
    end
    if self.dirty or self.phase == "landing" then
        self:_rebuild()
        self.dirty = false
    end
end

function Protector:_rebuild()
    local film = self.film
    local o = film.o
    local cols, rows = film.cols, film.rows
    local w1 = cols + 1
    local H, AIR, press, L0 = self.H, self.AIR, self.press, self.L0
    local n = film.n
    local face = film.face
    -- The landing: the film comes down from above with a slight tilt.
    local e = smoothstep(0, 1, min(1, self.t / 0.7))
    local z0 = 80 * (1 - e) * (1 - e)
    local tilt = 36 * (1 - e)
    local cx, cy = o.width * 0.5, o.height * 0.5
    local fl = 1 - 0.85 * self.coverage
    for r = 0, rows do
        for c = 0, cols do
            local i = r * w1 + c + 1
            local a, b = c * self.hx, r * self.hy
            H[i] = z0 + tilt * ((a - cx) / o.width + 0.5 * (b - cy) / o.height) + L0[i] * (1 - press[i]) * fl * e
            AIR[i] = 0
        end
    end
    -- Bubbles.
    for _, bub in self.bubbles do
        local size = bub.r * max(0, 1 + bub.grow)
        if bub.dying > 0 then
            size *= max(0, 1 - bub.dying / 0.18)
        end
        if size > 1 then
            local hb = size * 0.3
            local ca, cb = bub.a - o.a0, bub.b - o.b0
            local c0, c1 = max(0, floor((ca - size) / self.hx)), min(cols, ceil((ca + size) / self.hx))
            local r0, r1 = max(0, floor((cb - size) / self.hy)), min(rows, ceil((cb + size) / self.hy))
            local inv = 1 / (size * size)
            for r = r0, r1 do
                for c = c0, c1 do
                    local da, db = c * self.hx - ca, r * self.hy - cb
                    local rho = (da * da + db * db) * inv
                    if rho < 1 then
                        local k = (1 - rho)
                        local i = r * w1 + c + 1
                        H[i] += hb * k * k * e
                        AIR[i] = max(AIR[i], k * k)
                    end
                end
            end
        end
    end
    -- Positions and normals.
    local pos, dat, spos, sdat = film.pos, film.dat, film.spos, film.sdat
    local BX, BY, SHI = film.BX, film.BY, film.SHI
    local Zx, Zy = face.zx, face.zy
    local sdx, sdy = 0.55, 0.42
    local ia, ib = 1 / (2 * self.hx), 1 / (2 * self.hy)
    for r = 0, rows do
        for c = 0, cols do
            local i = r * w1 + c + 1
            local hl = if c > 0 then H[i - 1] else H[i]
            local hr = if c < cols then H[i + 1] else H[i]
            local hu = if r > 0 then H[i - w1] else H[i]
            local hd = if r < rows then H[i + w1] else H[i]
            local nx, ny = -(hr - hl) * ia, -(hd - hu) * ib
            local il = 1 / sqrt(nx * nx + ny * ny + 1)
            local z = H[i]
            local o8, o16 = (i - 1) * 8, (i - 1) * 16
            buffer.writef32(pos, o8, BX[i] + z * Zx)
            buffer.writef32(pos, o8 + 4, BY[i] + z * Zy)
            local wx, wy, wz = Geom.world_normal(face, nx, ny, 1)
            buffer.writef32(dat, o16, wx * il)
            buffer.writef32(dat, o16 + 4, wy * il)
            buffer.writef32(dat, o16 + 8, wz * il)
            buffer.writef32(dat, o16 + 12, AIR[i])
            local shi = SHI[i]
            if shi > 0 then
                local zs = max(0, z) * 0.9
                buffer.writef32(spos, (shi - 1) * 8, BX[i] + zs * sdx)
                buffer.writef32(spos, (shi - 1) * 8 + 4, BY[i] + zs * sdy)
                buffer.writef32(sdat, (shi - 1) * 16, 0.4 * smoothstep(1, 60, z0))
            end
        end
    end
    film:_upload()
end

return Protector
