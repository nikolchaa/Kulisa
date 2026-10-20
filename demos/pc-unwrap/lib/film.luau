-- A peelable foil (the shrink-wrap around the glass), film or sticker.
--
-- The foil is a grid mesh laid over one or more faces of the case: `spans` lists the faces it runs
-- across, one after another along its length s, so a foil can wrap from the front glass around
-- the corner onto the side glass. You grab it by the tab at its start (s = 0) and pull it along
-- its length, off the case. Vertices keep their rest position on the faces; the peel is described
-- by a few numbers and turned into positions and normals in one loop over the peeled part:
--
--   s  distance along the foil from its start, q  distance across (from the middle)
--   f  where the fold is: everything with s >= f is still stuck, s < f has been peeled. The fold
--      is a V: it runs ahead at the middle (where you hold it) by `wedge` per unit of |q|, so the
--      foil comes away from the middle outwards
--   th how far the free part is bent up before it runs straight to the hand (the "wrap angle")
--   R  the radius of the cylinder the foil rolls over at the fold (tunable), larger away from
--      the middle (the roll is a cone)
--
-- A peeled vertex at arc length u = sf - s from its fold (sf = f - wedge |q|) sits on the cylinder
-- while u < R*th and on a straight tail after that:  x = sf - R sin t - tail cos th,
-- z = R (1 - cos t) + tail sin th, with t = min(u / R, th), in the plane of the face the fold is
-- on (blended across the corner, so the foil rolls round it). The hand holds the free middle of
-- the start, which fixes f for a given th:
--   corner x = f (1 - cos th) - R sin th + R th cos th
-- The fold sticks: it only advances when the pull exceeds an adhesion threshold, then slips
-- forward (a click, a haptic pulse), like real film. A foil that is let go keeps its fold (a
-- partial peel persists); a small one sticks back down.

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

-- Tunables shared by every film; the HUD's slider changes `radius`.
Film.radius = 20
-- How far the free part is bent when the hand has pulled as far as it can: about 80 degrees, the
-- way you peel off a sticker. Pulling further makes the fold slip forward.
local TH_MAX = 1.4
local CONE = 0.5
local CORNER_BLEND = 26 -- the foil rolls round the corner over this many units either side

-- A tail of length L leaving the arc at wrap angle th, curling further by `kap` radians per unit
-- (a foil that hangs droops and eventually lies flat over itself). Returns the tail's end offset
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

export type Span = { face: Geom.Face, len: number, a0: number? }

export type Options = {
    parent: any,
    spans: { Span }, -- the faces it covers, in order along its length
    b0: number, height: number, -- extent across (face b coordinates)
    cols: number, rows: number, -- both even
    z: number,
    kind: string, -- "clear", "sticker"
    image: any?,
    tab: { number }, -- grab area at the start: length along s, height across (units)
    rscale: number?, -- roll radius relative to the shared setting (small films roll tighter)
    wedge: number?,
    alpha: number?, gloss: number?, wrinkle: number?, tint: { number }?,
}

local KINDS = { clear = 0, bag = 1, sticker = 2 }

function Film.new(o: Options)
    local sh = load_shaders()
    local self = setmetatable({}, Film)
    self.o = o
    self.rscale = o.rscale or 1
    self.Rcur = Film.radius * self.rscale
    self.base_wrinkle = o.wrinkle or 0.05
    self.energy = 0
    self.wedge = o.wedge or 0.7
    local cols, rows = o.cols, o.rows
    local n = (cols + 1) * (rows + 1)
    self.n, self.cols, self.rows = n, cols, rows

    -- Segments along the length.
    local segs, total = {}, 0
    for _, sp in o.spans do
        table.insert(segs, { face = sp.face, start = total, len = sp.len, a0 = sp.a0 })
        total += sp.len
    end
    self.segs, self.L = segs, total
    self.face = segs[1].face -- the face of the grab area
    self.ds = total / cols
    self.bmid = o.b0 + o.height * 0.5
    -- Callers (the protector) read these.
    o.width = total
    o.a0, o.b0 = 0, o.b0

    -- Rest data per vertex and per row.
    local BX, BY = table.create(n), table.create(n)
    local Q, Rk, SHI = table.create(rows + 1), table.create(rows + 1), table.create(n)
    local qmax = 1
    for r = 0, rows do
        Q[r + 1] = o.b0 + o.height * r / rows - self.bmid
        qmax = max(qmax, abs(Q[r + 1]))
    end
    self.Q, self.qmax = Q, qmax
    for r = 0, rows do
        Rk[r] = 1 + CONE * abs(Q[r + 1]) / qmax
    end
    self.Rk = Rk
    local scols = cols // 2
    for r = 0, rows do
        local b = self.bmid + Q[r + 1]
        for c = 0, cols do
            local i = r * (cols + 1) + c + 1
            local s = c * self.ds
            local seg = segs[#segs]
            for _, sg in segs do
                if s <= sg.start + sg.len + 1e-6 then
                    seg = sg
                    break
                end
            end
            BX[i], BY[i] = Geom.to_screen(seg.face, (seg.a0 or 0) + s - seg.start, b, 0)
            SHI[i] = if c % 2 == 0 and r % 2 == 0 then (r // 2) * (scols + 1) + c // 2 + 1 else 0
        end
    end
    self.BX, self.BY, self.SHI = BX, BY, SHI
    self.Cx, self.Cy = Geom.to_screen(segs[1].face, segs[1].a0 or 0, self.bmid, 0)

    -- Triangles are drawn from the far end of the foil towards the start, so the peeled part
    -- (small s) is painted over the stuck part.
    local cells = {}
    for r = 0, rows - 1 do
        for c = 0, cols - 1 do
            local qa = (abs(Q[r + 1]) + abs(Q[r + 2])) * 0.5
            table.insert(cells, { i = r * (cols + 1) + c + 1, s = (c + 0.5) * self.ds + self.wedge * qa })
        end
    end
    table.sort(cells, function(x, y)
        return x.s > y.s
    end)
    local idx = buffer.create(#cells * 6 * 4)
    local w = cols + 1
    for k, cell in cells do
        local i = cell.i
        local base = (k - 1) * 24
        local list = { i, i + 1, i + w + 1, i, i + w + 1, i + w }
        for m = 1, 6 do
            buffer.writeu32(idx, base + (m - 1) * 4, list[m])
        end
    end

    local face = segs[1].face
    local light, view = Geom.light, Geom.view
    local tint = o.tint or { 0.4, 0.55, 0.8 }
    local mirrored = face.ux * face.vy - face.uy * face.vx < 0
    local style = KINDS[o.kind] + (if mirrored then 4 else 0) + (if o.image then 8 else 0)
    self.params = {
        light[1], light[2], light[3], o.gloss or 0.9,
        view[1], view[2], view[3], o.alpha or 0.022,
        tint[1], tint[2], tint[3], o.wrinkle or 0.05,
        total, o.height, style, 1,
    }
    self.mesh = kulisa.mesh({
        parent = o.parent, image = o.image, cols = cols, rows = rows, width = total, height = o.height,
        shader = sh.plastic, params = self.params, z = o.z,
    })
    self.mesh:set_indices(idx)
    self.shadow = kulisa.mesh({
        parent = o.parent, cols = scols, rows = rows // 2, width = total, height = o.height,
        shader = sh.shadow, params = { total, o.height, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, z = o.z - 1,
    })

    self.pos = buffer.create(n * 8)
    self.dat = buffer.create(n * 16)
    local sn = (scols + 1) * (rows // 2 + 1)
    self.spos = buffer.create(sn * 8)
    self.sdat = buffer.create(sn * 16)
    self.fr = {}
    self:reset()
    return self
end

-- Back to a fresh, fully stuck film.
function Film:reset()
    self.mode = "stuck" -- stuck | peel (partly peeled, held or let go) | free (off the case) | gone | apply
    self.f, self.th, self.thv = 0, 1.1, 0
    self.droop = 0
    self.sf_prev = table.create(self.rows + 1, -1e6)
    self.held = false
    self.hx, self.hq = 0, 0
    self.qh = 0
    self.off_x, self.off_q = 0, 0
    self.slip = 0 -- seconds left of the current slip
    self.slip_to = 0
    self.tension = 0
    self.hint = 1
    self.free = nil
    self.mesh.visible, self.shadow.visible = true, true
    self.params[16] = 1
    self.mesh.params = self.params
    self.dirty = true
    self.done = false
    self:_write_rest()
    self:_upload()
end

-- The segment a column belongs to.
function Film:_seg_at(s: number)
    for _, sg in self.segs do
        if s <= sg.start + sg.len + 1e-6 then
            return sg
        end
    end
    return self.segs[#self.segs]
end

function Film:_write_rest()
    local pos, dat, spos, sdat = self.pos, self.dat, self.spos, self.sdat
    local BX, BY, SHI = self.BX, self.BY, self.SHI
    local cols = self.cols
    for r = 0, self.rows do
        for c = 0, cols do
            local i = r * (cols + 1) + c + 1
            local o8, o16 = (i - 1) * 8, (i - 1) * 16
            buffer.writef32(pos, o8, BX[i])
            buffer.writef32(pos, o8 + 4, BY[i])
            local n3 = self:_seg_at(c * self.ds).face.n3
            buffer.writef32(dat, o16, n3[1])
            buffer.writef32(dat, o16 + 4, n3[2])
            buffer.writef32(dat, o16 + 8, n3[3])
            buffer.writef32(dat, o16 + 12, 0)
            local k = SHI[i]
            if k > 0 then
                buffer.writef32(spos, (k - 1) * 8, BX[i])
                buffer.writef32(spos, (k - 1) * 8 + 4, BY[i])
                buffer.writef32(sdat, (k - 1) * 16, 0)
            end
        end
    end
end

function Film:_upload()
    self.mesh:set_positions(self.pos)
    self.mesh:set_data(self.dat)
    self.shadow:set_positions(self.spos)
    self.shadow:set_data(self.sdat)
end

-- The surface frame at fold position sf across row height b: the screen point on the foil's
-- surface, the screen direction of one unit along s, of the lift, of one unit across, and the
-- surface's world axes (blended across a corner between faces).
function Film:_frame(sf: number, b: number, out)
    local segs = self.segs
    local l = segs[1].face
    if #segs == 1 then
        local a = sf + (segs[1].a0 or 0)
        out.px, out.py = l.ox + a * l.ux + b * l.vx, l.oy + a * l.uy + b * l.vy
        out.nx, out.ny, out.zx, out.zy, out.vx, out.vy = l.ux, l.uy, l.zx, l.zy, l.vx, l.vy
        local a3, n3 = l.a3, l.n3
        out.a1, out.a2, out.a3, out.n1, out.n2, out.n3 = a3[1], a3[2], a3[3], n3[1], n3[2], n3[3]
        return
    end
    local r = segs[2].face
    local sb = segs[1].len
    local w = smoothstep(sb - CORNER_BLEND, sb + CORNER_BLEND, sf)
    local iw = 1 - w
    out.px = (l.ox + sf * l.ux + b * l.vx) * iw + (r.ox + (sf - sb) * r.ux + b * r.vx) * w
    out.py = (l.oy + sf * l.uy + b * l.vy) * iw + (r.oy + (sf - sb) * r.uy + b * r.vy) * w
    out.nx, out.ny = l.ux * iw + r.ux * w, l.uy * iw + r.uy * w
    out.zx, out.zy = l.zx * iw + r.zx * w, l.zy * iw + r.zy * w
    out.vx, out.vy = l.vx, l.vy
    local la, ra, ln, rn = l.a3, r.a3, l.n3, r.n3
    out.a1, out.a2, out.a3 = la[1] * iw + ra[1] * w, la[2] * iw + ra[2] * w, la[3] * iw + ra[3] * w
    out.n1, out.n2, out.n3 = ln[1] * iw + rn[1] * w, ln[2] * iw + rn[2] * w, ln[3] * iw + rn[3] * w
end

-- Position of the free corner (the middle of the start) in the fold's (x, z) plane.
function Film:_corner_xz(): (number, number)
    local f, R, th = self.f, self.Rcur, self.th
    local t = f / R
    if t > th then
        local dx, dz = tail_end(f - R * th, th, self.droop)
        return f - R * sin(th) + dx, R * (1 - cos(th)) + dz
    end
    return f - R * sin(t), R * (1 - cos(t))
end

-- Screen position of the free corner as drawn now.
function Film:corner_screen(): (number, number)
    if self.mode == "free" then
        return self.free.px, self.free.py
    end
    if self.f <= 0 then
        return self.Cx, self.Cy
    end
    local x, z = self:_corner_xz()
    local fr = self.fr
    self:_frame(self.f, self.bmid, fr)
    local dx = x - self.f
    return fr.px + dx * fr.nx + self.qh * fr.vx + z * fr.zx, fr.py + dx * fr.ny + self.qh * fr.vy + z * fr.zy
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

-- Where a stage point is along the foil: (s, q), following the surface under it (the side glass
-- takes over from the front glass at the corner).
function Film:_hand(sx: number, sy: number): (number, number)
    local segs = self.segs
    local a, b = Geom.to_face(segs[1].face, sx, sy)
    a -= segs[1].a0 or 0
    if #segs == 2 and a > segs[1].len then
        local a2, b2 = Geom.to_face(segs[2].face, sx, sy)
        return segs[1].len + a2, b2 - self.bmid
    end
    return a, b - self.bmid
end

-- Is the point (stage coordinates) on the part of the film that starts a peel?
function Film:can_grab(sx: number, sy: number): boolean
    if self.mode == "gone" or self.mode == "free" or self.mode == "apply" then
        return false
    end
    if self.f > 0.5 then
        local cx, cy = self:corner_screen()
        local dx, dy = sx - cx, sy - cy
        return dx * dx + dy * dy < 62 * 62
    end
    local s, q = self:_hand(sx, sy)
    return s > -14 and s < self.o.tab[1] and abs(q) < self.o.tab[2] * 0.5
end

function Film:begin_grab(sx: number, sy: number)
    self.held = true
    self.mode = "peel"
    self.hint = 0
    local ps, pq = self:_hand(sx, sy)
    local cx = self:_corner_xz()
    -- Keep the corner where it is: remember how far the pointer is from it.
    self.off_x, self.off_q = ps - cx, pq - self.qh
    self.hx, self.hq = cx, self.qh
    self.tension = 0
end

function Film:drag(sx: number, sy: number)
    local ps, pq = self:_hand(sx, sy)
    self.hx = ps - self.off_x
    self.hq = pq - self.off_q
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

local ADHESION = 11 -- how far the pull can run ahead of the fold before it slips (units)

function Film:_peel_step(dt: number)
    local R = self.Rcur
    local f, th = self.f, self.th
    local Lmax = self.L + self.wedge * self.qmax
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
    if f >= Lmax * 0.93 and f > 0 then
        self:_detach()
    end
end

-- Writes positions, normals and shadow strengths for the peeled part (and its neighbourhood),
-- row by row: each row has its own fold position and frame, and is peeled from its first column.
function Film:_write_peel()
    local R, f, th = self.Rcur, self.f, self.th
    local pos, dat, spos, sdat = self.pos, self.dat, self.spos, self.sdat
    local BX, BY, SHI, Q, Rk, sf_prev = self.BX, self.BY, self.SHI, self.Q, self.Rk, self.sf_prev
    local cols, ds = self.cols, self.ds
    local qh = self.qh
    local wedge = self.wedge
    local sth, cth = sin(th), cos(th)
    local kap = self.droop
    local fi = 1 / max(f, 1)
    local fold_ao = 0.5 * sin(min(th, PI))
    local fr = self.fr
    local segs = self.segs
    -- Where the shadow of a lifted point lands: away from the light (up and left), per unit of lift.
    local light_x, light_y = 0.55, 0.42
    local straight = kap < 1e-4
    for r = 0, self.rows do
        local q = Q[r + 1]
        local sfr = f - wedge * abs(q)
        local cmax = min(cols, floor((max(sfr, sf_prev[r + 1]) + 4 * R) / ds) + 1)
        if cmax >= 0 then
            local Ri = R * Rk[r]
            local b = self.bmid + q
            self:_frame(sfr, b, fr)
            local nx, ny, zx, zy, vx, vy = fr.nx, fr.ny, fr.zx, fr.zy, fr.vx, fr.vy
            local ppx, ppy = fr.px, fr.py
            local a1, a2, a3, n1, n2, n3 = fr.a1, fr.a2, fr.a3, fr.n1, fr.n2, fr.n3
            -- Tail (straight case): x = ax + u * (-cth), z = az + u * sth.
            local ax = sfr - Ri * sth + Ri * th * cth
            local az = Ri * (1 - cth) - Ri * th * sth
            local arc_u = Ri * th
            local row = r * (cols + 1) + 1
            local lat = qh * vx
            local latv = qh * vy
            local shadow_row = r % 2 == 0
            for c = 0, cmax do
                local i = row + c
                local o8, o16 = (i - 1) * 8, (i - 1) * 16
                local u = sfr - c * ds
                if u > 0 then
                    local x, z, st, ct
                    if u > arc_u then
                        if straight then
                            x, z = ax - u * cth, az + u * sth
                            st, ct = sth, cth
                        else
                            local dx, dz, te = tail_end(u - arc_u, th, kap)
                            x, z = sfr - Ri * sth + dx, Ri * (1 - cth) + dz
                            st, ct = sin(te), cos(te)
                        end
                    else
                        local t = u / Ri
                        st, ct = sin(t), cos(t)
                        x, z = sfr - Ri * st, Ri * (1 - ct)
                    end
                    local w = u * fi
                    local dxs = x - sfr
                    local k = w * w
                    local px = ppx + dxs * nx + k * lat
                    local py = ppy + dxs * ny + k * latv
                    buffer.writef32(pos, o8, px + z * zx)
                    buffer.writef32(pos, o8 + 4, py + z * zy)
                    -- Curl shadow: darker where the foil bends over itself near the fold.
                    local e = 1 + u / (2.5 * Ri)
                    local k1 = 1 - 0.21 * (1 - ct) / (e * e)
                    buffer.writef32(dat, o16, (st * a1 + ct * n1) * k1)
                    buffer.writef32(dat, o16 + 4, (st * a2 + ct * n2) * k1)
                    buffer.writef32(dat, o16 + 8, (st * a3 + ct * n3) * k1)
                    buffer.writef32(dat, o16 + 12, 0)
                    local shi = SHI[i]
                    if shi > 0 then
                        local s8 = (shi - 1) * 8
                        local zs = min(z, 90)
                        buffer.writef32(spos, s8, px + zs * light_x)
                        buffer.writef32(spos, s8 + 4, py + zs * light_y)
                        buffer.writef32(sdat, (shi - 1) * 16, 0.5 * smoothstep(1.5, 18, z) * exp(-z / 110))
                    end
                else
                    buffer.writef32(pos, o8, BX[i])
                    buffer.writef32(pos, o8 + 4, BY[i])
                    local d = u / (2 * R)
                    local k1 = 1 - (if f > 0 then fold_ao / (1 + 1.5 * d * d) else 0)
                    local seg = if #segs == 1 or c * ds <= segs[1].len then segs[1].face else segs[2].face
                    local sn = seg.n3
                    buffer.writef32(dat, o16, sn[1] * k1)
                    buffer.writef32(dat, o16 + 4, sn[2] * k1)
                    buffer.writef32(dat, o16 + 8, sn[3] * k1)
                    buffer.writef32(dat, o16 + 12, 0)
                    local shi = SHI[i]
                    if shi > 0 then
                        buffer.writef32(spos, (shi - 1) * 8, BX[i])
                        buffer.writef32(spos, (shi - 1) * 8 + 4, BY[i])
                        buffer.writef32(sdat, (shi - 1) * 16, 0)
                    end
                end
            end
        end
        sf_prev[r + 1] = sfr
    end
end

-- Everything is off the case: the foil becomes a free sheet in the air.
function Film:_detach()
    if self.mode == "free" then
        return
    end
    local sx, sy = self:corner_screen()
    local o = self.o
    -- The sheet hangs from its corner: its centre's offset in screen space gives the resting angle.
    local mx, my = Geom.to_screen(self.face, math.min(self.L, self.segs[1].len) * 0.5, self.bmid, 0)
    local c0x, c0y = mx - self.Cx, my - self.Cy
    self.free = {
        px = sx, py = sy, vx = 0, vy = 0, psi = 0, omega = 0, t = 0, blend = 0,
        held = self.held, c0x = c0x, c0y = c0y, psi_eq = PI / 2 - math.atan2(c0y, c0x), fade_t = 0,
        lx = sx, ly = sy, speed = 0, snap = buffer.create(self.n * 8),
    }
    buffer.copy(self.free.snap, 0, self.pos, 0, self.n * 8)
    self.mode = "free"
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
        local target = fr.psi_eq + 0.14 * sin(fr.t * 6.1) * (1 + math.min(2, sqrt(fr.vx * fr.vx + fr.vy * fr.vy) / 700))
        fr.omega += (-70 * (fr.psi - target) - 6.5 * fr.omega - fr.vx * 0.012) * dt
        fr.psi += fr.omega * dt
        fr.fade_t = 0
    else
        -- Let go: it falls. A film is light, so air drag soon caps its speed and it sways side to
        -- side on the way down, but gravity always wins: whatever the hand was doing, it ends up
        -- going down.
        if fr.released == nil then
            fr.released = true
            fr.ft = 0
            fr.vy = math.max(fr.vy, -120)
            if self.on_flutter then
                self.on_flutter(sqrt(fr.vx * fr.vx + fr.vy * fr.vy))
            end
        end
        fr.ft += dt
        fr.vx *= exp(-2.2 * dt)
        fr.vy *= exp(-1.2 * dt)
        fr.vx += (sin(fr.t * 3.3) * 160) * dt
        fr.vy += 900 * dt
        fr.px += fr.vx * dt
        fr.py += fr.vy * dt
        fr.omega += (-14 * (fr.psi - fr.psi_eq - 0.8 * sin(fr.t * 2.2)) - 2.5 * fr.omega) * dt
        fr.psi += fr.omega * dt
        fr.fade_t = fr.ft
        if fr.ft > 1.4 then
            self.mode = "gone"
            self.mesh.visible, self.shadow.visible = false, false
            self.done = true
            return
        end
    end
    fr.blend = math.min(1, fr.blend + dt / 0.28)
    fr.speed = sqrt(fr.vx * fr.vx + fr.vy * fr.vy)
    self.dirty = true
end

function Film:_write_free()
    local fr = self.free
    local pos, dat, spos, sdat = self.pos, self.dat, self.spos, self.sdat
    local BX, BY, SHI = self.BX, self.BY, self.SHI
    local Cx, Cy = self.Cx, self.Cy
    local cp, sp = cos(fr.psi), sin(fr.psi)
    local blend = math.min(1, fr.blend)
    blend = blend * blend * (3 - 2 * blend)
    local amp = 7 + math.min(26, fr.speed * 0.04)
    local lam = 210
    local t = fr.t
    local snap = fr.snap
    local L = self.L
    local cols, ds = self.cols, self.ds
    local segs = self.segs
    local face = self.face
    local zx, zy = face.zx, face.zy
    -- Held, the sheet stands a little off where it was; falling, it is flat to the air.
    local lift = if fr.held then 12 + 4 * sin(t * 2) else 4
    local fx, fy = fr.px, fr.py
    local shrink = if #segs > 1 then 0.55 else 1 -- a big wrap crumples as it flies
    local fade = 1 - smoothstep(0.8, 1.4, fr.fade_t)
    local snapping = blend < 1
    for r = 0, self.rows do
        local qr = self.Q[r + 1]
        for c = 0, cols do
            local i = r * (cols + 1) + c + 1
            local s = c * ds
            local o8, o16 = (i - 1) * 8, (i - 1) * 16
            -- Offset from the corner in screen space, turned by psi.
            local ox, oy = (BX[i] - Cx) * shrink, (BY[i] - Cy) * shrink
            local rx, ry = ox * cp - oy * sp, ox * sp + oy * cp
            local env = 0.25 + 0.75 * s / L
            local ph = 6.2832 * s / lam - 9.5 * t + qr * 0.004
            local z = lift + amp * env * sin(ph)
            local slope = amp * env * 6.2832 / lam * cos(ph) * 0.06
            local x = fx + rx + z * zx
            local y = fy + ry + z * zy
            if snapping then
                x = buffer.readf32(snap, o8) * (1 - blend) + x * blend
                y = buffer.readf32(snap, o8 + 4) * (1 - blend) + y * blend
            end
            buffer.writef32(pos, o8, x)
            buffer.writef32(pos, o8 + 4, y)
            local nl = 1 / sqrt(1 + slope * slope)
            local seg = if #segs == 1 or s <= segs[1].len then segs[1].face else segs[2].face
            local a3, n3 = seg.a3, seg.n3
            buffer.writef32(dat, o16, (-slope * a3[1] + n3[1]) * nl)
            buffer.writef32(dat, o16 + 4, (-slope * a3[2] + n3[2]) * nl)
            buffer.writef32(dat, o16 + 8, (-slope * a3[3] + n3[3]) * nl)
            local shi = SHI[i]
            if shi > 0 then
                buffer.writef32(spos, (shi - 1) * 8, x + 18)
                buffer.writef32(spos, (shi - 1) * 8 + 4, y + 46)
                buffer.writef32(sdat, (shi - 1) * 16, 0.25 * fade)
            end
        end
    end
    self.params[16] = fade
    self.mesh.params = self.params
end

function Film:update(dt: number)
    if self.mode == "gone" or self.mode == "apply" then
        return
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
        hint = (4.5 + 4 * sin(kulisa.now() * 2.4)) * self.hint
    end
    local before, th_before, qh_before = self.f, self.th, self.qh
    if self.mode == "peel" then
        self:_peel_step(dt)
    end
    if self.mode == "free" then
        return
    end
    local moving = self.f ~= before or self.th ~= th_before or self.qh ~= qh_before or self.held
        or abs(self.thv) > 0.01 or self.slip > 0
        or (self.f > 0 and abs(self.droop - (if self.held then 0 else 0.0042)) > 0.0001)
        or abs(self.qh) > 0.05
    -- The idle "breathing" tab only needs about 20 updates a second.
    if hint > 0 and not moving and not self.dirty then
        local now = kulisa.now()
        if now - (self.hint_at or -1) < 0.05 then
            hint = 0
        else
            self.hint_at = now
        end
    end
    if moving or hint > 0 or self.dirty then
        if hint > 0 and self.f == 0 then
            -- Draw the breathing tab as a tiny peel without changing the state.
            local f, th = self.f, self.th
            self.f, self.th = hint, 1.15
            self:_write_peel()
            self.f, self.th = f, th
        else
            self:_write_peel()
        end
        self:_upload()
        self.dirty = false
    end
    if self.mode == "peel" and self.f == 0 and not self.held and abs(self.thv) < 0.02 and not moving then
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
    return self.f / self.L
end

function Film:is_done(): boolean
    return self.mode == "free" or self.mode == "gone"
end

return Film
