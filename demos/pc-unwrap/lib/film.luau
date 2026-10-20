-- A peelable film (or sticker) on one face of the case.
--
-- The film is a grid mesh. Its vertices keep their rest position (a, b) on the face; the peel is
-- described by a few numbers and turned into positions and normals in a single loop:
--
--   s  distance of a vertex from the corner you grab, along the peel direction n (angle `dir`)
--   q  distance across, along the fold line
--   f  where the fold is: everything with s >= f is still stuck, s < f has been peeled
--   th how far the free part is bent up before it runs straight to the hand (the "wrap angle")
--   R  the radius of the cylinder the film rolls over at the fold (tunable)
--
-- A peeled vertex at arc length u = f - s from the fold sits on the cylinder while u < R*th and on
-- a straight tail after that:  x = f - R sin t - tail cos th,  z = R (1 - cos t) + tail sin th,
-- with t = min(u / R, th). The hand holds the free corner, which fixes f for a given th:
--   corner x = f (1 - cos th) - R sin th + R th cos th   (so for a full fold-back, f = (x + pi R)/2:
--   the fold moves at half the hand's speed). The fold sticks: it only advances when the pull
--   exceeds an adhesion threshold, then slips forward (a click, a haptic pulse), like real film.
-- A film that is let go keeps its fold (a partial peel persists); a small one sticks back down.

local Geom = require("lib/geom")

local sin, cos, sqrt, abs, min, max, exp, floor = math.sin, math.cos, math.sqrt, math.abs, math.min, math.max, math.exp, math.floor
local PI = math.pi

local Film = {}
Film.__index = Film

local shaders

local function load_shaders()
    if not shaders then
        shaders = {
            plastic = kulisa.load_shader("shaders/plastic.wgsl"),
            shadow = kulisa.load_shader("shaders/shadow.wgsl"),
        }
    end
    return shaders
end

local function smoothstep(a: number, b: number, x: number): number
    local t = math.clamp((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)
end

-- A tail of length L leaving the arc at wrap angle th, curling further by `kap` radians per px
-- (a film that hangs droops and eventually lies flat over itself). Returns the tail's end offset
-- (dx, dz) in the fold's plane and the angle there.
local function tail_end(L: number, th: number, kap: number): (number, number, number)
    if kap < 1e-4 then
        return -L * cos(th), L * sin(th), th
    end
    local te = th + kap * L
    if te > PI then
        local L1 = (PI - th) / kap
        return sin(th) / kap + (L - L1), (cos(th) + 1) / kap, PI
    end
    return -(sin(te) - sin(th)) / kap, (cos(th) - cos(te)) / kap, te
end

-- Tunables shared by every film; the HUD's slider changes `radius`.
Film.radius = 20
-- How far the free part is bent when the hand has pulled as far as it can: about 100 degrees, the
-- way you peel a sticker. Pulling further makes the fold slip forward.
local TH_MAX = 1.75

export type Options = {
    parent: any,
    face: Geom.Face,
    a0: number, b0: number, width: number, height: number,
    cols: number, rows: number,
    z: number,
    kind: string, -- "clear", "sticker"
    image: any?,
    corner: { number }, -- rest position (a, b) of the corner you grab
    dir: number, -- peel direction in the face plane, radians (0 = along +a)
    tab: { number }, -- size of the grab area around the corner, in face px
    rscale: number?, -- roll radius relative to the shared setting (small films roll tighter)
    alpha: number?, gloss: number?, wrinkle: number?, tint: { number }?,
}

local KINDS = { clear = 0, bag = 1, sticker = 2 }

function Film.new(o: Options)
    local sh = load_shaders()
    local self = setmetatable({}, Film)
    local face = o.face
    self.face, self.o = face, o
    self.rscale = o.rscale or 1
    self.Rcur = Film.radius * self.rscale
    self.base_wrinkle = o.wrinkle or 0.05
    self.energy = 0
    self.kind = o.kind
    local cols, rows = o.cols, o.rows
    local n = (cols + 1) * (rows + 1)
    self.n, self.cols, self.rows = n, cols, rows

    local ca, cb = o.corner[1], o.corner[2]
    local cph, sph = cos(o.dir), sin(o.dir)
    self.ca, self.cb, self.cph, self.sph = ca, cb, cph, sph
    self.Cx, self.Cy = Geom.to_screen(face, ca, cb, 0)
    -- Screen directions of the peel direction and of the fold line.
    self.Nx, self.Ny = cph * face.ux + sph * face.vx, cph * face.uy + sph * face.vy
    self.Tx, self.Ty = -sph * face.ux + cph * face.vx, -sph * face.uy + cph * face.vy
    self.Zx, self.Zy = face.zx, face.zy

    -- Rest data per vertex.
    local S, Q, BX, BY, SHI = table.create(n), table.create(n), table.create(n), table.create(n), table.create(n)
    local smax, qmax = 0, 1
    local shadow_n = 0
    local scols, srows = cols // 2, rows // 2
    for r = 0, rows do
        for c = 0, cols do
            local i = r * (cols + 1) + c + 1
            local a = o.a0 + o.width * c / cols
            local b = o.b0 + o.height * r / rows
            local da, db = a - ca, b - cb
            S[i] = da * cph + db * sph
            Q[i] = -da * sph + db * cph
            BX[i], BY[i] = Geom.to_screen(face, a, b, 0)
            smax = max(smax, S[i])
            qmax = max(qmax, abs(Q[i]))
            -- Every second vertex also feeds the (half resolution) shadow mesh.
            if c % 2 == 0 and r % 2 == 0 and c <= scols * 2 and r <= srows * 2 then
                shadow_n += 1
                SHI[i] = shadow_n
            else
                SHI[i] = 0
            end
        end
    end
    self.S, self.Q, self.BX, self.BY, self.SHI, self.smax, self.qmax = S, Q, BX, BY, SHI, smax, qmax

    -- Vertices sorted by s: the peeled part is always a prefix of this order.
    local ord = table.create(n)
    for i = 1, n do
        ord[i] = i
    end
    table.sort(ord, function(x, y)
        return S[x] < S[y]
    end)
    self.ord = ord

    -- Triangles are drawn from the far end of the film towards the corner, so the peeled part
    -- (small s) is painted over the stuck part.
    local cells = {}
    for r = 0, rows - 1 do
        for c = 0, cols - 1 do
            local i = r * (cols + 1) + c + 1
            table.insert(cells, { i = i, s = (S[i] + S[i + 1] + S[i + cols + 1] + S[i + cols + 2]) * 0.25 })
        end
    end
    table.sort(cells, function(x, y)
        return x.s > y.s
    end)
    local idx = buffer.create(#cells * 6 * 4)
    for k, cell in cells do
        local i = cell.i
        local base = (k - 1) * 24
        local w = cols + 1
        local list = { i, i + 1, i + w + 1, i, i + w + 1, i + w }
        for m = 1, 6 do
            buffer.writeu32(idx, base + (m - 1) * 4, list[m])
        end
    end

    local light, view = face.light, face.view
    local tint = o.tint or { 0.4, 0.55, 0.8 }
    local style = KINDS[o.kind] + (if face.ux * face.vy - face.uy * face.vx < 0 then 4 else 0) + (if o.image then 8 else 0)
    self.params = {
        light[1], light[2], light[3], o.gloss or 0.9,
        view[1], view[2], view[3], o.alpha or 0.022,
        tint[1], tint[2], tint[3], o.wrinkle or 0.05,
        o.width, o.height, style, 1,
    }
    self.mesh = kulisa.mesh({
        parent = o.parent, image = o.image, cols = cols, rows = rows, width = o.width, height = o.height,
        shader = sh.plastic, params = self.params, z = o.z,
    })
    self.mesh:set_indices(idx)
    self.shadow = kulisa.mesh({
        parent = o.parent, cols = scols, rows = srows, width = o.width, height = o.height,
        shader = sh.shadow, params = { o.width, o.height, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, z = o.z - 1,
    })

    self.pos = buffer.create(n * 8)
    self.dat = buffer.create(n * 16)
    self.spos = buffer.create((scols + 1) * (srows + 1) * 8)
    self.sdat = buffer.create((scols + 1) * (srows + 1) * 16)
    self:reset()
    return self
end

-- Back to a fresh, fully stuck film.
function Film:reset()
    self.mode = "stuck" -- stuck | peel (partly peeled, held or let go) | free (off the case) | gone
    self.f, self.th, self.thv = 0, 1.1, 0
    self.droop = 0
    self.f_prev = 0
    self.held = false
    self.hx, self.hq = 0, 0
    self.qh = 0
    self.off_x, self.off_q = 0, 0
    self.slip = 0 -- seconds left of the current slip
    self.slip_to = 0
    self.tension = 0
    self.hint = 1
    self.flap_t = 0
    self.free = nil
    self.mesh.visible, self.shadow.visible = true, true
    self.params[16] = 1
    self.mesh.params = self.params
    self.dirty = true
    self.done = false
    self:_write_rest()
    self:_upload()
end

function Film:_write_rest()
    local pos, dat, spos, sdat = self.pos, self.dat, self.spos, self.sdat
    local BX, BY, SHI = self.BX, self.BY, self.SHI
    local sh = 0
    for i = 1, self.n do
        local o8, o16 = (i - 1) * 8, (i - 1) * 16
        buffer.writef32(pos, o8, BX[i])
        buffer.writef32(pos, o8 + 4, BY[i])
        buffer.writef32(dat, o16, 0)
        buffer.writef32(dat, o16 + 4, 0)
        buffer.writef32(dat, o16 + 8, 1)
        buffer.writef32(dat, o16 + 12, 0)
        local k = SHI[i]
        if k > 0 then
            buffer.writef32(spos, (k - 1) * 8, BX[i])
            buffer.writef32(spos, (k - 1) * 8 + 4, BY[i])
            buffer.writef32(sdat, (k - 1) * 16, 0)
        end
    end
end

function Film:_upload()
    self.mesh:set_positions(self.pos)
    self.mesh:set_data(self.dat)
    self.shadow:set_positions(self.spos)
    self.shadow:set_data(self.sdat)
end

-- Screen position of the free corner as drawn now.
function Film:corner_screen(): (number, number)
    if self.f <= 0 then
        return self.Cx, self.Cy
    end
    local x, z = self:_corner_xz()
    return self.Cx + x * self.Nx + self.qh * self.Tx + z * self.Zx, self.Cy + x * self.Ny + self.qh * self.Ty + z * self.Zy
end

-- Position of the free corner (s = 0, u = f) in the fold's (x, z) plane.
function Film:_corner_xz(): (number, number)
    local f, R, th = self.f, self.Rcur, self.th
    local t = f / R
    if t > th then
        local dx, dz = tail_end(f - R * th, th, self.droop)
        return f - R * sin(th) + dx, R * (1 - cos(th)) + dz
    end
    return f - R * sin(t), R * (1 - cos(t))
end

-- x of the free corner for a given fold position f and wrap angle th.
local function corner_x(f: number, th: number, R: number): number
    local te = min(th, f / R)
    local tail = max(0, f - R * th)
    return f - R * sin(te) - tail * cos(th)
end

-- The smallest fold position whose corner can reach x with the film bent as far as it goes.
local function fold_for(x: number, R: number): number
    if x <= 0 then
        return 0
    end
    local th = TH_MAX
    local lin = (x + R * sin(th) - R * th * cos(th)) / (1 - cos(th))
    if lin >= R * th then
        return lin
    end
    -- Still on the arc (no straight tail yet): f - R sin(f / R) = x.
    local lo, hi = 0, R * th
    for _ = 1, 14 do
        local mid = (lo + hi) * 0.5
        if mid - R * sin(mid / R) < x then
            lo = mid
        else
            hi = mid
        end
    end
    return (lo + hi) * 0.5
end

-- The wrap angle that puts the corner at x for fold f (clamped to the allowed range).
local function wrap_for(x: number, f: number, R: number, lo: number): number
    if corner_x(f, TH_MAX, R) <= x then
        return TH_MAX
    end
    if corner_x(f, lo, R) >= x then
        return lo
    end
    local a, b = lo, TH_MAX
    for _ = 1, 12 do
        local mid = (a + b) * 0.5
        if corner_x(f, mid, R) < x then
            a = mid
        else
            b = mid
        end
    end
    return (a + b) * 0.5
end

-- Is the point (stage coordinates) on the part of the film that starts a peel?
function Film:can_grab(sx: number, sy: number): boolean
    if self.mode == "gone" or self.mode == "free" and not self.free.held then
        return false
    end
    if self.mode == "free" then
        return false
    end
    if self.f > 0.5 then
        local cx, cy = self:corner_screen()
        local dx, dy = sx - cx, sy - cy
        return dx * dx + dy * dy < 62 * 62
    end
    local o = self.o
    local a, b = Geom.to_face(self.face, sx, sy)
    local da, db = a - self.ca, b - self.cb
    -- Inside the film's corner: the corner's two edges point towards +a/+b or -a/-b.
    local sa = if o.corner[1] <= o.a0 + 1 then 1 else -1
    local sb = if o.corner[2] <= o.b0 + 1 then 1 else -1
    da, db = da * sa, db * sb
    return da > -10 and da < o.tab[1] and db > -10 and db < o.tab[2]
end

function Film:begin_grab(sx: number, sy: number)
    self.held = true
    self.mode = "peel"
    self.hint = 0
    local a, b = Geom.to_face(self.face, sx, sy)
    local da, db = a - self.ca, b - self.cb
    local px = da * self.cph + db * self.sph
    local pq = -da * self.sph + db * self.cph
    local cx, cq = self:_corner_xz(), self.qh
    -- Keep the corner where it is: remember how far the pointer is from it.
    self.off_x, self.off_q = px - cx, pq - cq
    self.hx, self.hq = cx, cq
    self.tension = 0
    self.moved = 0
end

function Film:drag(sx: number, sy: number)
    local a, b = Geom.to_face(self.face, sx, sy)
    local da, db = a - self.ca, b - self.cb
    self.hx = da * self.cph + db * self.sph - self.off_x
    self.hq = -da * self.sph + db * self.cph - self.off_q
    if self.mode == "free" then
        self.free.px, self.free.py = sx, sy
    end
end

function Film:release()
    self.held = false
    if self.mode == "free" then
        self.free.held = false
    end
end

-- Callbacks: on_slip(strength), on_detach(), on_stick(), on_flutter(speed). Set by the owner.

local ADHESION = 11 -- how far the pull can run ahead of the fold before it slips (px)

function Film:_peel_step(dt: number)
    local R = self.Rcur
    local f, th = self.f, self.th
    local Lmax = self.smax
    if self.held then
        local hx = self.hx
        -- Hand behind its reach: the fold retreats (the film sticks back down as you push back).
        local th_min = 0.55
        local f_ret = (hx + R * sin(th_min) - R * th_min * cos(th_min)) / (1 - cos(th_min))
        local req = fold_for(hx, R)
        if f > f_ret + 0.5 and f > 0 then
            f = max(max(f_ret, 0), f - 900 * dt)
            self.slip = 0
            self.tension = 0
        elseif req > f + 0.001 then
            -- The pull wants to go further than the film allows: tension builds until it slips.
            local pull = req - f
            self.tension = pull
            local threshold = ADHESION * (0.55 + 0.9 * (0.5 + 0.5 * sin(f * 0.37 + 1.3))) * (if f < 4 then 1.5 else 1)
            if self.slip <= 0 and pull > threshold then
                self.slip = 0.07
                self.slip_to = f + pull * (0.6 + 0.35 * (0.5 + 0.5 * sin(f * 1.9)))
                if self.on_slip then
                    self.on_slip(math.clamp(pull / (threshold * 2.5), 0.1, 1), pull)
                end
            end
            th = TH_MAX
        else
            self.tension = 0
        end
        if self.slip > 0 then
            self.slip -= dt
            f += (self.slip_to - f) * math.min(1, dt / 0.04)
            if self.slip <= 0 then
                f = self.slip_to
            end
        end
        if self.tension == 0 then
            local target = wrap_for(hx, f, R, th_min)
            th += (target - th) * math.min(1, dt * 24)
        else
            th += (TH_MAX - th) * math.min(1, dt * 24)
        end
        self.thv = 0
        self.droop += (0 - self.droop) * math.min(1, dt * 10)
        self.qh += (math.clamp(self.hq, -200, 200) - self.qh) * math.min(1, dt * 18)
    else
        self.slip = 0
        self.tension = 0
        -- Let go: the flap droops to a curl and swings (a damped spring); a small peel sticks back.
        local rest = 0.5 + 1.7 * (1 - exp(-f / (R * 5)))
        local acc = -95 * (th - rest) - 8.5 * self.thv
        self.thv += acc * dt
        th += self.thv * dt
        th = math.clamp(th, 0.3, PI + 0.4)
        self.qh += (0 - self.qh) * math.min(1, dt * 5)
        self.droop += ((if f > 0 then 0.0042 else 0) - self.droop) * math.min(1, dt * 2.5)
        if f > 0 and f < max(1.7 * R, 30) then
            f = max(0, f - (60 + f * 5) * dt)
            if f == 0 and self.on_stick then
                self.on_stick()
            end
        end
    end
    f = math.clamp(f, 0, Lmax)
    self.f, self.th = f, th
    if f >= Lmax * 0.94 and f > 0 then
        self:_detach()
    end
end

-- Writes positions, normals and shadow strengths for the peeled part (and its neighbourhood).
function Film:_write_peel()
    local R, f, th = self.Rcur, self.f, self.th
    local pos, dat, spos, sdat = self.pos, self.dat, self.spos, self.sdat
    local S, Q, BX, BY, SHI, ord = self.S, self.Q, self.BX, self.BY, self.SHI, self.ord
    local Cx, Cy, Nx, Ny, Tx, Ty, Zx, Zy = self.Cx, self.Cy, self.Nx, self.Ny, self.Tx, self.Ty, self.Zx, self.Zy
    local cph, sph = self.cph, self.sph
    local face = self.face
    local qh = self.qh
    local qmax = self.qmax
    local cone = 0.45
    local sth, cth = sin(th), cos(th)
    local kap = self.droop
    -- Where the shadow of a lifted point lands: away from the light, in face axes.
    local l = face.light
    local sdx = (-l[1] / l[3]) * face.ux + (-l[2] / l[3]) * face.vx
    local sdy = (-l[1] / l[3]) * face.uy + (-l[2] / l[3]) * face.vy
    local limit = max(f, self.f_prev) + 4 * R + 2
    local fi = 1 / max(f, 1)
    local fold_ao = 0.5 * sin(min(th, PI))
    for k = 1, self.n do
        local i = ord[k]
        local s = S[i]
        if s >= limit then
            break
        end
        local o8, o16 = (i - 1) * 8, (i - 1) * 16
        local u = f - s
        local shi = SHI[i]
        if u > 0 then
            local q = Q[i]
            local Ri = R * (1 + cone * abs(q) / qmax)
            local t = u / Ri
            local x, z, st, ct
            if t > th then
                local dx, dz, te = tail_end(u - Ri * th, th, kap)
                x = f - Ri * sth + dx
                z = Ri * (1 - cth) + dz
                st, ct = sin(te), cos(te)
                t = th
            else
                st, ct = sin(t), cos(t)
                x = f - Ri * st
                z = Ri * (1 - ct)
            end
            local w = u * fi
            local qn = q + qh * w * w
            local px, py = Cx + x * Nx + qn * Tx, Cy + x * Ny + qn * Ty
            buffer.writef32(pos, o8, px + z * Zx)
            buffer.writef32(pos, o8 + 4, py + z * Zy)
            local sh = sin(t * 0.5)
            local k1 = 1 - 0.42 * sh * sh * exp(-u / (5 * Ri))
            buffer.writef32(dat, o16, st * cph * k1)
            buffer.writef32(dat, o16 + 4, st * sph * k1)
            buffer.writef32(dat, o16 + 8, ct * k1)
            if shi > 0 then
                local s8 = (shi - 1) * 8
                local zs = min(z, 90)
                buffer.writef32(spos, s8, px + zs * sdx)
                buffer.writef32(spos, s8 + 4, py + zs * sdy)
                buffer.writef32(sdat, (shi - 1) * 16, 0.5 * smoothstep(1.5, 18, z) * exp(-z / 110))
            end
        else
            buffer.writef32(pos, o8, BX[i])
            buffer.writef32(pos, o8 + 4, BY[i])
            local d = u / (2 * R)
            local k1 = 1 - (if f > 0 then fold_ao * exp(-d * d) else 0)
            buffer.writef32(dat, o16, 0)
            buffer.writef32(dat, o16 + 4, 0)
            buffer.writef32(dat, o16 + 8, k1)
            if shi > 0 then
                buffer.writef32(spos, (shi - 1) * 8, BX[i])
                buffer.writef32(spos, (shi - 1) * 8 + 4, BY[i])
                buffer.writef32(sdat, (shi - 1) * 16, 0)
            end
        end
    end
    self.f_prev = f
end

-- Everything is off the case: the film becomes a free sheet in the air.
function Film:_detach()
    if self.mode == "free" then
        return
    end
    local sx, sy = self:corner_screen()
    local o = self.o
    -- The sheet hangs from its corner: its centre's offset in screen space gives the resting angle.
    local ca, cb = self.ca, self.cb
    local mx, my = Geom.to_screen(self.face, o.a0 + o.width * 0.5, o.b0 + o.height * 0.5, 0)
    local c0x, c0y = mx - self.Cx, my - self.Cy
    self.free = {
        px = sx, py = sy, vx = 0, vy = 0, psi = 0, omega = 0, t = 0, blend = 0,
        held = self.held, c0x = c0x, c0y = c0y, psi_eq = PI / 2 - math.atan2(c0y, c0x), fade_t = 0,
        lx = sx, ly = sy, speed = 0, snap = buffer.create(self.n * 8), sdat_snap = nil,
    }
    buffer.copy(self.free.snap, 0, self.pos, 0, self.n * 8)
    self.mode = "free"
    self.f, self.f_prev = self.smax, self.smax
    if self.on_detach then
        self.on_detach(self.held)
    end
end

function Film:_free_step(dt: number)
    local fr = self.free
    fr.t += dt
    local px, py = fr.px, fr.py
    if fr.held then
        -- Follow the pointer; the sheet swings about the corner like a flag.
        local nvx, nvy = (px - fr.lx) / max(dt, 1e-3), (py - fr.ly) / max(dt, 1e-3)
        fr.vx += (nvx - fr.vx) * math.min(1, dt * 14)
        fr.vy += (nvy - fr.vy) * math.min(1, dt * 14)
        fr.lx, fr.ly = px, py
        local target = fr.psi_eq + 0.14 * sin(fr.t * 6.1) * (1 + math.min(2, math.sqrt(fr.vx * fr.vx + fr.vy * fr.vy) / 700))
        fr.omega += (-70 * (fr.psi - target) - 6.5 * fr.omega - fr.vx * 0.012) * dt
        fr.psi += fr.omega * dt
        fr.fade_t = 0
    else
        -- Adrift: keeps its velocity, drifts, sinks and turns, then fades.
        if fr.released == nil then
            fr.released = true
            fr.ft = 0
            if self.on_flutter then
                self.on_flutter(math.sqrt(fr.vx * fr.vx + fr.vy * fr.vy))
            end
        end
        fr.ft += dt
        fr.vx *= exp(-1.7 * dt)
        fr.vy *= exp(-1.7 * dt)
        fr.vx += (sin(fr.t * 3.3) * 120) * dt
        fr.vy += (150 + 40 * sin(fr.t * 4.1)) * dt
        fr.px += fr.vx * dt
        fr.py += fr.vy * dt
        fr.omega += (-14 * (fr.psi - fr.psi_eq - 0.8 * sin(fr.t * 2.2)) - 2.5 * fr.omega) * dt
        fr.psi += fr.omega * dt
        fr.fade_t = fr.ft
        if fr.ft > 1.6 then
            self.mode = "gone"
            self.mesh.visible, self.shadow.visible = false, false
            self.done = true
            return
        end
    end
    fr.blend = math.min(1, fr.blend + dt / 0.28)
    fr.speed = math.sqrt(fr.vx * fr.vx + fr.vy * fr.vy)
    self.dirty = true
end

function Film:_write_free()
    local fr = self.free
    local pos, dat, spos, sdat = self.pos, self.dat, self.spos, self.sdat
    local S, Q, BX, BY, SHI = self.S, self.Q, self.BX, self.BY, self.SHI
    local face, o = self.face, self.o
    local Cx, Cy = self.Cx, self.Cy
    local psi = fr.psi
    local cp, sp = cos(psi), sin(psi)
    local blend = math.min(1, fr.blend)
    blend = blend * blend * (3 - 2 * blend)
    local amp = 7 + math.min(26, fr.speed * 0.04)
    local lam = 210
    local w = 9.5
    local t = fr.t
    local cph, sph = self.cph, self.sph
    local snap = fr.snap
    local smax = self.smax
    local Zx, Zy = face.zx, face.zy
    local l = face.light
    local sdx = (-l[1] / l[3]) * face.ux + (-l[2] / l[3]) * face.vx
    local sdy = (-l[1] / l[3]) * face.uy + (-l[2] / l[3]) * face.vy
    local lift = 24 + 10 * sin(t * 2)
    local fx, fy = fr.px, fr.py
    local hang = (if fr.held then 0 else 1)
    for i = 1, self.n do
        local s = S[i]
        local o8, o16 = (i - 1) * 8, (i - 1) * 16
        -- Offset from the corner in screen space, turned by psi.
        local ox, oy = BX[i] - Cx, BY[i] - Cy
        local rx, ry = ox * cp - oy * sp, ox * sp + oy * cp
        local env = 0.25 + 0.75 * s / smax
        local ph = 6.2832 * s / lam - w * t + Q[i] * 0.004
        local z = lift + amp * env * sin(ph)
        local slope = amp * env * 6.2832 / lam * cos(ph) * 0.06
        local x = fx + rx + z * Zx
        local y = fy + ry + z * Zy
        local ax, ay = x, y
        if blend < 1 then
            ax = buffer.readf32(snap, o8) * (1 - blend) + x * blend
            ay = buffer.readf32(snap, o8 + 4) * (1 - blend) + y * blend
        end
        buffer.writef32(pos, o8, ax)
        buffer.writef32(pos, o8 + 4, ay)
        local nl = 1 / sqrt(1 + slope * slope)
        buffer.writef32(dat, o16, -slope * cph * nl)
        buffer.writef32(dat, o16 + 4, -slope * sph * nl)
        buffer.writef32(dat, o16 + 8, nl)
        local shi = SHI[i]
        if shi > 0 then
            buffer.writef32(spos, (shi - 1) * 8, x + (z - lift) * sdx * 0 + (lift + 20) * sdx * 0.6)
            buffer.writef32(spos, (shi - 1) * 8 + 4, y + (lift + 20) * sdy * 0.6)
            buffer.writef32(sdat, (shi - 1) * 16, 0.3 * (1 - smoothstep(0.9, 1.6, fr.fade_t)) * hang * 0 + 0.25 * (1 - smoothstep(0.9, 1.6, fr.fade_t)))
        end
    end
    local fade = 1 - smoothstep(0.9, 1.6, fr.fade_t)
    self.params[16] = fade
    self.mesh.params = self.params
end

function Film:update(dt: number)
    if self.mode == "gone" then
        return
    end
    if self.mode == "apply" then
        return -- driven by a Protector
    end
    self.Rcur = Film.radius * self.rscale
    if self.energy > 0.002 or self.energy_on then
        self.energy = max(0, self.energy - dt * 1.6)
        self.params[12] = self.base_wrinkle + self.energy
        self.mesh.params = self.params
        self.energy_on = self.energy > 0.002
    end
    if self.mode == "free" then
        self:_free_step(dt)
        if self.mode == "free" then
            self:_write_free()
            self:_upload()
        end
        return
    end
    -- A fresh film breathes at its tab to show where to start.
    local hint = 0
    if self.mode == "stuck" and self.hint > 0 and self.f == 0 then
        hint = (3.5 + 3 * sin(kulisa.now() * 2.4)) * self.hint
    end
    local before = self.f
    if self.mode == "peel" then
        self:_peel_step(dt)
    end
    if self.mode == "free" then
        return
    end
    local moving = self.f ~= before or self.held or abs(self.thv) > 0.01 or abs(self.qh) > 0.05 or self.slip > 0
        or (self.f > 0 and abs(self.droop - (if self.held then 0 else 0.0042)) > 0.0001)
    if self.f > 0 or moving or hint > 0 or self.dirty then
        if hint > 0 and self.f == 0 then
            -- Draw the breathing tab as a tiny peel without changing the state.
            local f, th = self.f, self.th
            self.f, self.th = hint, 1.15
            self:_write_peel()
            self.f, self.th = f, th
            self.f_prev = hint
        else
            self:_write_peel()
        end
        self:_upload()
        self.dirty = false
    end
    if self.mode == "peel" and self.f == 0 and not self.held and abs(self.thv) < 0.02 and not self.dirty then
        self.mode = "stuck"
        self.hint = 1
    end
end

-- Crinkling: moving the film adds wrinkles that settle again.
function Film:crinkle(amount: number)
    self.energy = min(0.32, self.energy + amount)
    self.energy_on = true
end

function Film:peeled_fraction(): number
    return self.f / self.smax
end

function Film:is_done(): boolean
    return self.mode == "free" or self.mode == "gone"
end

return Film
