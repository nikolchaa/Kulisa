-- The valley: a sky shader, three mountain ranges, painted forest and grass strips, a cabin, and
-- particles (fireflies, falling leaves, snow, chimney smoke, campfire embers), each on a layer
-- with its own parallax factor. Everything is placed against what the screen shows, so any
-- aspect ratio (and the phone's portrait design) gets a full scene. `set_time` recolors it for
-- the time of day.

local Valley = {}

local SKY = kulisa.load_shader("shaders/sky.wgsl")
local MOUNTAINS = kulisa.load_shader("shaders/mountains.wgsl")

-- How far beyond the screen edges every layer reaches (the camera moves by up to ~80 units).
local MARGIN = 220

local root
local layers = {}
local sky
local strips = {} -- see `strip`
local ranges = {} -- the mountain strips, far to near
local cabin, windows, fire_glow
local systems = {}
Valley.time = 0.72
Valley.weather = { fireflies = true, leaves = true, snow = false, smoke = true, embers = true }

local function layer(name: string, parallax: number, z: number)
    local l = kulisa.layer({ parent = root, parallax = parallax, z = z })
    layers[name] = l
    return l
end

-- Keyframes of the palette over the day (0 midnight, 0.25 dawn, 0.5 noon, 0.75 dusk). The three
-- mountain ranges (far to near) each get a lit and a shadowed rock color; `haze` is the air in
-- front of them, `snow` lit snow.
local KEYS = {
    { t = 0.0, lit = { "#2e385e", "#262f50", "#1b2139" }, shade = { "#0f1428", "#0c1021", "#090c18" }, haze = "#141a33", snow = "#8494c0", far = "#161d33", near = "#0c101d", grass = "#06080d", light = 1 },
    { t = 0.25, lit = { "#f2b39a", "#d99a8c", "#8f6a78" }, shade = { "#5a4a78", "#4a3d66", "#2e2840" }, haze = "#b88a9c", snow = "#ffe2cf", far = "#4a4462", near = "#2c2740", grass = "#16131f", light = 0.35 },
    { t = 0.5, lit = { "#d6d0c6", "#b7bdb2", "#7d8c68" }, shade = { "#3c4c70", "#384a60", "#26362c" }, haze = "#9fb8d6", snow = "#ffffff", far = "#4a6a74", near = "#2f4a44", grass = "#22341a", light = 0 },
    { t = 0.75, lit = { "#ffae7a", "#e08a6e", "#8a5560" }, shade = { "#5c3f6e", "#4a325c", "#2c2040" }, haze = "#c9808a", snow = "#ffd2b0", far = "#4a3450", near = "#261c30", grass = "#100a14", light = 0.6 },
    { t = 1.0, lit = { "#2e385e", "#262f50", "#1b2139" }, shade = { "#0f1428", "#0c1021", "#090c18" }, haze = "#141a33", snow = "#8494c0", far = "#161d33", near = "#0c101d", grass = "#06080d", light = 1 },
}

local function lin(hex: string): { number }
    local r, g, b = tonumber(hex:sub(2, 3), 16) / 255, tonumber(hex:sub(4, 5), 16) / 255, tonumber(hex:sub(6, 7), 16) / 255
    return { r ^ 2.2, g ^ 2.2, b ^ 2.2 }
end

local function lerp3(a, b, t)
    return { a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t }
end

local function hex(c): string
    local function e(v)
        return math.clamp(math.floor(v ^ (1 / 2.2) * 255 + 0.5), 0, 255)
    end
    return string.format("#%02x%02x%02x", e(c[1]), e(c[2]), e(c[3]))
end

-- The palette at time `t` (0..1).
local function palette(t: number)
    t = t % 1
    local a, b = KEYS[1], KEYS[2]
    for i = 1, #KEYS - 1 do
        if t >= KEYS[i].t and t <= KEYS[i + 1].t then
            a, b = KEYS[i], KEYS[i + 1]
        end
    end
    local k = (t - a.t) / (b.t - a.t)
    k = k * k * (3 - 2 * k)
    local p = { lit = {}, shade = {} }
    for i = 1, 3 do
        p.lit[i] = lerp3(lin(a.lit[i]), lin(b.lit[i]), k)
        p.shade[i] = lerp3(lin(a.shade[i]), lin(b.shade[i]), k)
    end
    p.haze = lerp3(lin(a.haze), lin(b.haze), k)
    p.snow = lerp3(lin(a.snow), lin(b.snow), k)
    p.far = lerp3(lin(a.far), lin(b.far), k)
    p.near = lerp3(lin(a.near), lin(b.near), k)
    p.grass = lerp3(lin(a.grass), lin(b.grass), k)
    p.light = a.light + (b.light - a.light) * k
    return p
end

-- The mountain ranges, far to near: parallax, how much haze covers them (all over, and more at
-- the foot), how much of their snow shows.
local RANGES = {
    { image = "images/mountains_far.png", parallax = 0.08, height = 0.56, bottom = 0.73, haze = 0.3, mist = 0.35, snow = 1 },
    { image = "images/mountains_mid.png", parallax = 0.18, height = 0.44, bottom = 0.8, haze = 0.16, mist = 0.4, snow = 0.9 },
    { image = "images/mountains_near.png", parallax = 0.3, height = 0.3, bottom = 0.84, haze = 0.05, mist = 0.35, snow = 0 },
}

-- A strip of a horizontally tiling image, placed by `place`. `fill` (the image's ground lightness,
-- 0..1) puts a solid block of that color under the strip, down past the bottom edge, so whatever
-- is behind never shows below it.
local function strip(l, image: string, height_frac: number, bottom_frac: number, key: string?, fill: number?)
    local s = { layer = l, image = image, height_frac = height_frac, bottom_frac = bottom_frac, tiles = {}, key = key, fill = fill }
    if fill then
        s.fill_sprite = kulisa.sprite({ parent = l, z = -1 })
    end
    table.insert(strips, s)
    return s
end

function Valley.build(parent)
    root = kulisa.group({ parent = parent })
    local _, _, vw0, vh0 = kulisa.screen:visible()
    local w0 = vw0 + MARGIN * 2
    local sky_layer = layer("sky", 0.03, 0)
    sky = kulisa.sprite({ parent = sky_layer, shader = SKY })
    for i, spec in RANGES do
        local l = layer(`range{i}`, spec.parallax, 10 + i)
        local s = strip(l, spec.image, spec.height, spec.bottom)
        s.shader, s.range = MOUNTAINS, spec
        ranges[i] = s
    end
    local far = layer("far", 0.5, 20)
    strip(far, "images/forest_far.png", 0.24, 0.8, "far", 0.55)
    local vx0, vy0 = kulisa.screen:visible()
    local mid = layer("mid", 0.62, 25)
    systems.fireflies = kulisa.particles({
        parent = mid, capacity = 300, rate = 22, shape = "rect", width = w0, height = vh0 * 0.2, life = { 4, 8 }, speed = { 4, 14 },
        direction = -90, spread = 360, size = { 9, 6 }, wander = { 26, 0.25 },
        colors = { "#e8ff9a00", "#d8ff70", "#c0ff6000" }, fade = { 0.3, 0.4 }, blend = "add", warm_up = 6,
    })
    local near = layer("near", 0.75, 30)
    strip(near, "images/forest_near.png", 0.36, 0.92, "near", 0.26)
    cabin = kulisa.sprite({ parent = near, image = "images/cabin.png", z = 5 })
    -- A sibling, not a child: children take their parent's tint, and the windows must glow.
    windows = kulisa.sprite({ parent = near, image = "images/cabin_windows.png", blend = "add", opacity = 0, z = 6 })
    fire_glow = kulisa.sprite({ parent = near, image = "images/smoke.png", blend = "add", tint = "#ff9a4a", z = 6, center = true })
    systems.smoke = kulisa.particles({
        parent = near, image = "images/smoke.png", capacity = 120, rate = 5, life = { 3.5, 5 }, speed = { 14, 26 },
        direction = -90, spread = 18, size = { 26, 120 }, wander = { 18, 0.15 }, gravity = { 10, -4 },
        colors = { "#bdb6c4aa", "#a49cb066", "#9890a000" }, fade = { 0.15, 0.6 }, rotation = "random", spin = { -20, 20 },
        z = 4, warm_up = 5,
    })
    systems.embers = kulisa.particles({
        parent = near, capacity = 200, rate = 26, shape = "line", width = 30, life = { 0.8, 2.2 },
        speed = { 40, 110 }, direction = -90, spread = 30, size = { 5, 1.5 }, gravity = -30, drag = 0.8,
        wander = { 10, 1.2 }, colors = { "#fff3b0", "#ff8a3a", "#ff200000" }, fade = { 0, 0.5 }, blend = "add", z = 7,
        warm_up = 2,
    })
    local front = layer("front", 1.0, 40)
    systems.leaves = kulisa.particles({
        parent = front, image = "images/leaves.png", frames = { 4, 1 }, capacity = 160, rate = 5, shape = "line", width = w0 * 1.3,
        life = { 7, 11 }, speed = { 20, 50 }, direction = 70, spread = 40, size = { 22, 22 }, scale = { 0.6, 1.2 },
        gravity = { 6, 22 }, drag = 0.3, wander = { 60, 0.18 }, spin = { -160, 160 }, rotation = "random",
        fade = { 0.05, 0.15 }, x = vx0 + vw0 / 2, y = vy0 - 60, warm_up = 8,
    })
    local snow_layer = layer("snow", 1.15, 45)
    systems.snow = kulisa.particles({
        parent = snow_layer, capacity = 1500, rate = 120, shape = "line", width = w0 * 1.4, life = { 8, 12 }, speed = { 30, 60 },
        direction = 95, spread = 20, size = { 7, 7 }, scale = { 0.4, 1.3 }, gravity = { 4, 8 }, wander = { 30, 0.3 },
        colors = "#ffffffdd", fade = { 0.05, 0.1 }, emitting = false,
    })
    local grass_layer = layer("grass", 1.3, 50)
    strip(grass_layer, "images/grass.png", 0.14, 1.0, "grass", 0.1)
    Valley.place()
    Valley.set_weather(Valley.weather)
    Valley.set_time(Valley.time)
end

-- Lays everything out against the visible screen (call after resizes and display changes).
function Valley.place()
    if not root then
        return
    end
    local vx, vy, vw, vh = kulisa.screen:visible()
    local x0, w = vx - MARGIN, vw + MARGIN * 2
    sky.x, sky.y, sky.width, sky.height = x0, vy - MARGIN, w, vh + MARGIN * 2
    for _, s in strips do
        local img = kulisa.load_image(s.image)
        local h = vh * s.height_frac
        local tw = img.width / img.height * h
        local n = math.ceil(w / tw) + 1
        for i = #s.tiles + 1, n do
            s.tiles[i] = kulisa.sprite({ parent = s.layer, image = s.image, shader = s.shader })
        end
        for i, t in s.tiles do
            t.visible = i <= n
            t.width, t.height = tw + 1, h
            t.x = x0 + (i - 1) * tw
            t.y = vy + vh * s.bottom_frac - h
        end
        s.bottom = vy + vh * s.bottom_frac
        s.h = h
        if s.fill_sprite then
            -- Overlaps the strip's own ground by a few pixels, so no seam shows.
            s.fill_sprite.x, s.fill_sprite.y = x0, s.bottom - 3
            s.fill_sprite.width, s.fill_sprite.height = w, vh * (1 - s.bottom_frac) + MARGIN + 3
        end
    end
    local near = strips[#ranges + 2]
    local scale = near.h / 520
    cabin.width, cabin.height = 220 * scale * 1.3, 190 * scale * 1.3
    cabin.x = vx + vw * 0.6
    cabin.y = near.bottom - 90 * scale - cabin.height + 8 * scale
    windows.x, windows.y, windows.width, windows.height = cabin.x, cabin.y, cabin.width, cabin.height
    fire_glow.width, fire_glow.height = 90 * scale, 90 * scale
    fire_glow.x, fire_glow.y = cabin.x - 60 * scale, cabin.y + cabin.height - 4 * scale
    systems.embers.x, systems.embers.y = fire_glow.x, fire_glow.y
    systems.smoke.x, systems.smoke.y = cabin.x + cabin.width * 0.73, cabin.y + cabin.height * 0.12
    systems.fireflies.x, systems.fireflies.y = vx + vw / 2, near.bottom - near.h * 0.45
    systems.fireflies:set({ shape = "rect", width = w, height = near.h * 0.5 })
    systems.leaves.x, systems.leaves.y = vx + vw / 2, vy - 60
    systems.leaves:set({ shape = "line", width = w * 1.3 })
    systems.snow.x, systems.snow.y = vx + vw / 2, vy - 80
    systems.snow:set({ shape = "line", width = w * 1.4 })
end

function Valley.set_time(t: number)
    Valley.time = t % 1
    if not root then
        return
    end
    local _, _, vw, vh = kulisa.screen:visible()
    sky.params = { Valley.time, (vw + MARGIN * 2) / (vh + MARGIN * 2), kulisa.now() * 0.01, 0.55 }
    local p = palette(Valley.time)
    -- Light from the sun by day, the moon by night; both cross the sky from left to right.
    local a = (Valley.time - 0.25) * math.pi * 2
    local side = if math.sin(a) > -0.05 then 0.5 - 0.5 * math.cos(a) else 0.5 + 0.5 * math.cos(a)
    -- Lit from one side or the other (a mix of both would flatten the relief); noon still comes
    -- from the morning side, the turn happens in the afternoon.
    side = math.clamp((side - 0.45) / 0.3, 0, 1)
    side = side * side * (3 - 2 * side)
    for i, s in ranges do
        local spec = s.range
        local lit, shade, haze, snow = p.lit[i], p.shade[i], p.haze, p.snow
        local params = {
            lit[1], lit[2], lit[3], side,
            shade[1], shade[2], shade[3], spec.snow,
            haze[1], haze[2], haze[3], spec.haze,
            snow[1], snow[2], snow[3], spec.mist,
        }
        for _, t in s.tiles do
            t.params = params
        end
    end
    for _, s in strips do
        if s.key then
            local c = p[s.key]
            for _, t in s.tiles do
                t.tint = hex(c)
            end
            if s.fill_sprite then
                local g = s.fill ^ 2.2
                s.fill_sprite.tint = hex({ c[1] * g, c[2] * g, c[3] * g })
            end
        end
    end
    cabin.tint = hex(lerp3(p.near, p.far, 0.45))
    windows.opacity = p.light
    fire_glow.opacity = 0.35 + 0.5 * p.light
    systems.fireflies.rate = if Valley.weather.fireflies then 30 * p.light else 0
end

function Valley.set_weather(w)
    Valley.weather = w
    systems.leaves.emitting = w.leaves
    systems.snow.emitting = w.snow
    systems.smoke.emitting = w.smoke
    systems.embers.emitting = w.embers
    systems.fireflies.emitting = w.fireflies
    Valley.set_time(Valley.time)
end

function Valley.show(on: boolean)
    if root then
        root.visible = on
    end
end

-- Particles alive across the systems (for the stats line).
function Valley.alive(): number
    local n = 0
    for _, s in systems do
        n += s.alive
    end
    return n
end

kulisa.every(1 / 20, function()
    -- The sky's clouds drift; the time of day may be animated by the demo.
    if root and root.visible then
        Valley.set_time(Valley.time)
    end
end)

return Valley
