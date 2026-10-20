-- PC unwrap: a brand-new PC in its bag. Slide the bag off, peel the protective films and the
-- I/O sticker (corner first, the fold rolling into a cylinder), then smooth a fresh protector
-- on with a squeegee. Built only on the public `kulisa` API: meshes with per-vertex data,
-- shaders, targets, drag input, audio and haptics. See README.md.

local Geom = require("lib/geom")
local PcView = require("lib/pcview")
local Film = require("lib/film")
local Bag = require("lib/bag")
local Prints = require("lib/prints")
local Protector = require("lib/protector")
local Sfx = require("lib/sfx")

kulisa.display({ width = Geom.STAGE_W, height = Geom.STAGE_H, scale = "expand", orientation = "any" })
kulisa.screen.clear = "#13141a"
local FONT = kulisa.load_font("fonts/Inter-Regular.ttf")[1]
kulisa.load_font("fonts/Inter-Bold.ttf")
local ui = kulisa.ui

local world = kulisa.layer({ parallax = 0 })
local stage = kulisa.group({ parent = world })

-- Fits the stage into the visible part of the screen, leaving room for the HUD below it.
local function fit()
    local x, y, w, h = kulisa.screen:visible()
    local usable_h = h - 120
    local s = math.min(w / Geom.STAGE_W, usable_h / Geom.STAGE_H)
    stage.scale = s
    stage.x = x + (w - Geom.STAGE_W * s) / 2
    stage.y = y + math.max(10, (usable_h - Geom.STAGE_H * s) / 2)
end
fit()
kulisa.on("resize", fit)
kulisa.on("display", fit)

local function to_stage(x: number, y: number): (number, number)
    return (x - stage.x) / stage.scale, (y - stage.y) / stage.scale
end

-- The scene -----------------------------------------------------------------------------

PcView.build(stage)
Prints.init(FONT)
local print_host = kulisa.group({ parent = stage })

local sticker = Film.new({
    parent = stage, face = Geom.top, a0 = 140, b0 = 6, width = 190, height = 64, cols = 14, rows = 6, z = 24,
    kind = "sticker", corner = { 140, 6 }, dir = 0.0, tab = { 56, 34 }, rscale = 0.4, gloss = 0.7,
    image = Prints.sticker(print_host), wrinkle = 0.02,
})
local top = Film.new({
    parent = stage, face = Geom.top, a0 = 0, b0 = 0, width = 760, height = 380, cols = 40, rows = 20, z = 22,
    kind = "clear", corner = { 0, 0 }, dir = 0.4, tab = { 110, 70 }, image = Prints.top(print_host),
})
local glass = Film.new({
    parent = stage, face = Geom.glass, a0 = 0, b0 = 0, width = 760, height = 440, cols = 48, rows = 48, z = 20,
    kind = "clear", corner = { 0, 0 }, dir = 0.5, tab = { 110, 70 }, image = Prints.glass(print_host),
})
local films = { sticker = sticker, top = top, glass = glass }
-- What is drawn on top is grabbed first.
local film_order = { sticker, top, glass }
local bag = Bag.new(stage, Prints.bag(print_host), 40)
local spare = Film.new({
    parent = stage, face = Geom.glass, a0 = 0, b0 = 0, width = 760, height = 440, cols = 48, rows = 48, z = 26,
    kind = "clear", corner = { 0, 0 }, dir = 0.5, tab = { 1, 1 }, image = Prints.protector(print_host),
})
local protector = Protector.new(spare, stage, 60)

-- State and HUD ---------------------------------------------------------------------------

local tasks = {
    { key = "bag", label = "Slide the bag off", done = ui.signal(false) },
    { key = "glass", label = "Peel the glass film", done = ui.signal(false) },
    { key = "top", label = "Peel the top film", done = ui.signal(false) },
    { key = "sticker", label = "Remove the I/O sticker", done = ui.signal(false) },
    { key = "apply", label = "Fit a fresh protector (optional)", done = ui.signal(false), optional = true },
}
local applying = ui.signal(false) -- the spare protector has been started
local hint = ui.signal("")
local radius = ui.signal(Film.radius)
local show_stats = ui.signal(true)
local stats_text = ui.signal("")
local celebrated = false

-- One entry per pointer that is holding something.
local grabs: { [number]: { obj: any, kind: string, lx: number, ly: number, speed: number } } = {}

local function task(key: string)
    for _, t in tasks do
        if t.key == key then
            return t
        end
    end
    error(`no task {key}`)
end

local function remaining(): number
    local n = 0
    for _, t in tasks do
        if not t.done:peek() and not t.optional then
            n += 1
        end
    end
    return n
end

local function refresh_hint()
    if not task("bag").done:peek() then
        hint:set("Grab the bag and slide it off the PC")
    elseif remaining() > 0 then
        hint:set("Grab a corner tab and peel it back. Faster pulls crinkle more")
    elseif applying:peek() and not task("apply").done:peek() then
        hint:set("Rub the squeegee over the film to push the air bubbles out to the edges")
    elseif task("apply").done:peek() then
        hint:set("Perfect fit. Enjoy your new PC")
    else
        hint:set("All unwrapped. Want to fit a fresh protector?")
    end
end
refresh_hint()

local function complete(key: string)
    local t = task(key)
    if t.done:peek() then
        return
    end
    t.done:set(true)
    refresh_hint()
    if remaining() == 0 and not celebrated then
        celebrated = true
        kulisa.after(0.5, Sfx.chime)
    end
end

local function mid_x(): number
    return Geom.STAGE_W / 2
end

local function start_protector()
    if applying:peek() or not task("glass").done:peek() then
        return
    end
    applying:set(true)
    protector:start()
    Sfx.whoosh(300, mid_x())
    refresh_hint()
end

local function reset_all()
    for id, g in grabs do
        g.obj:release()
        grabs[id] = nil
    end
    bag:reset()
    for _, film in films do
        film:reset()
    end
    protector:reset()
    for _, t in tasks do
        t.done:set(false)
    end
    applying:set(false)
    celebrated = false
    refresh_hint()
end


bag.on_slide_off = function(dx: number)
    Sfx.whoosh(600, mid_x() + dx * 300)
    kulisa.haptics.pulse(0.4, 0.05)
end
bag.on_gone = function()
    complete("bag")
end

protector.on_pop = function(size: number, x: number)
    Sfx.pop(size, x)
end
protector.on_move = function(speed: number, x: number, pushed: number)
    Sfx.squeak(speed, x)
    if pushed > 0 then
        kulisa.haptics.pulse(0.18, 0.02)
    end
end
protector.on_done = function()
    complete("apply")
end

for key, film in films do
    film.on_slip = function(strength: number)
        Sfx.slip(strength, mid_x())
        film:crinkle(0.1 + strength * 0.1)
    end
    film.on_stick = function()
        Sfx.stick(mid_x())
    end
    film.on_detach = function()
        Sfx.tear(mid_x())
        complete(key)
    end
    film.on_flutter = function(speed: number)
        Sfx.whoosh(speed, mid_x())
    end
end

local function task_row(t)
    return ui.row({
        gap = 10,
        align = "center",
        ui.box({ width = 18, height = 18, background = if t.done() then "#4fd18b" else "#3a3f52" }),
        ui.text({
            text = t.label,
            size = 20,
            font = FONT,
            color = if t.done() then "#8a90a6" else "#e8ebf5",
        }),
    })
end

ui.screen("hud", function()
    local rows = {}
    for _, t in tasks do
        table.insert(rows, task_row(t))
    end
    return ui.box({
        ui.column({
            anchor = "top_left",
            margin = 20,
            gap = 8,
            ui.text({ text = "{b}Unwrap your new PC{/b}", size = 28, font = FONT, color = "#ffffff" }),
            ui.column({ gap = 6, rows }),
        }),
        ui.row({
            anchor = "bottom_right",
            margin = 20,
            gap = 10,
            applying() == false and task("glass").done() and ui.button({
                id = "apply",
                on_click = start_protector,
                padding = { 10, 18 },
                background = "#2f6fed",
                hover_background = "#4a85ff",
                ui.text({ text = "Fit a fresh protector", size = 18, font = FONT, color = "#ffffff" }),
            }),
            ui.button({
                id = "reset",
                on_click = reset_all,
                padding = { 10, 18 },
                background = "#2f3346",
                hover_background = "#434860",
                ui.text({ text = "Start over", size = 18, font = FONT, color = "#e8ebf5" }),
            }),
        }),
        ui.column({
            anchor = "top_right",
            margin = 20,
            gap = 6,
            width = 240,
            ui.text({ text = `Roll radius  {math.floor(radius())}`, size = 16, font = FONT, color = "#b4bacb" }),
            ui.slider({
                id = "radius",
                value = radius,
                min = 6,
                max = 42,
                step = 1,
                height = 28,
                width = 240,
                on_change = function(v)
                    Film.radius = v
                    radius:set(v)
                end,
                background = "#2b2f40",
                bar = "#5b86f0",
                thumb = "#ffffff",
            }),
        }),
        ui.column({
            anchor = "bottom",
            margin = 28,
            gap = 6,
            align = "center",
            ui.text({ text = hint, size = 22, font = FONT, color = "#dfe4f5", align = "center" }),
            ui.text({ text = stats_text, size = 13, font = FONT, color = "#6f768c", align = "center", visible = show_stats() }),
        }),
    })
end, { z = 100, area = "safe" })
ui.show("hud")

-- Input -----------------------------------------------------------------------------------


local function find_target(sx: number, sy: number)
    if not bag.gone and bag:contains(sx, sy) then
        return bag, "bag"
    end
    for _, film in film_order do
        if film:can_grab(sx, sy) then
            return film, "film"
        end
    end
    if protector:can_grab(sx, sy) then
        return protector, "tool"
    end
    return nil, nil
end

kulisa.on("press", function(x, y, _, id)
    if grabs[id] then
        return
    end
    local sx, sy = to_stage(x, y)
    local obj, kind = find_target(sx, sy)
    if not obj then
        return
    end
    for _, g in grabs do
        if g.obj == obj then
            return
        end
    end
    grabs[id] = { obj = obj, kind = kind, lx = sx, ly = sy, speed = 0 }
    obj:begin_grab(sx, sy)
end)

kulisa.on("move", function(x, y, id)
    local g = grabs[id]
    if g then
        local sx, sy = to_stage(x, y)
        g.obj:drag(sx, sy)
    end
end)

kulisa.on("release", function(_, _, _, id)
    local g = grabs[id]
    if g then
        g.obj:release()
        grabs[id] = nil
    end
end)

kulisa.on("key", function(key)
    if key == "f3" then
        show_stats:set(not show_stats:peek())
    elseif key == "r" then
        reset_all()
    elseif key == "b" and not bag.gone then
        -- Shortcut: toss the bag off at once.
        bag.asleep = false
        bag.glide = 0.4
        bag.gdx, bag.gdy = -1, 0
    end
end)

-- Frame loop -----------------------------------------------------------------------------

local frames, frame_time, script_time = 0, 0, 0
kulisa.on("update", function(dt)
    local t0 = os.clock()
    Prints.tick()
    bag:update(dt)
    protector:update(dt)
    for _, film in film_order do
        film:update(dt)
    end
    -- Sound and wrinkles follow how fast each held thing is being dragged.
    for _, g in grabs do
        local obj = g.obj
        local x, y
        if g.kind == "bag" then
            x, y = obj.lx or g.lx, obj.ly or g.ly
        elseif g.kind == "tool" then
            x, y = g.lx, g.ly
        else
            x, y = obj:corner_screen()
        end
        local speed = math.sqrt((x - g.lx) ^ 2 + (y - g.ly) ^ 2) / math.max(dt, 1e-3)
        g.speed += (speed - g.speed) * math.min(1, dt * 12)
        g.lx, g.ly = x, y
        if g.kind == "bag" then
            Sfx.rustle(g.speed, x)
        elseif g.kind == "tool" then
            -- (the protector makes its own sounds)
        else
            obj:crinkle(math.min(0.2, g.speed * dt * 0.0005))
            Sfx.crinkle(g.speed, x)
        end
    end
    script_time += os.clock() - t0
    frames += 1
    frame_time += dt
    if frame_time >= 0.5 then
        stats_text:set(string.format("%.0f fps  ·  script %.2f ms/frame", frames / frame_time, script_time / frames * 1000))
        frames, frame_time, script_time = 0, 0, 0
    end
end)

kulisa.on("error", function(message)
    hint:set("Script error: " .. message:split("\n")[1])
end)
