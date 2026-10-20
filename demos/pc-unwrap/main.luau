-- PC unwrap: a brand-new PC in a shipping box. Peel the tape, open the box, take the foam out,
-- lift the PC out, pull its bag off over the top, peel the films off the side and the front glass
-- and the I/O sticker (corner first, the fold rolling into a cylinder), then smooth a fresh
-- protector on with a squeegee. The box and the PC are Blockbench renders (images/pc); the
-- bag, the films, the tape and the glass are live meshes laid exactly over them. Built only on
-- the public `kulisa` API. See README.md.

local Geom = require("lib/geom")
local PcView = require("lib/pcview")
local Film = require("lib/film")
local Bag = require("lib/bag")
local Prints = require("lib/prints")
local Protector = require("lib/protector")
local Unbox = require("lib/unbox")
local Sfx = require("lib/sfx")

kulisa.display({ width = Geom.STAGE_W, height = Geom.STAGE_H, scale = "expand", orientation = "any" })
kulisa.screen.clear = "#13141a"
local FONT = kulisa.load_font("fonts/Inter-Regular.ttf")[1]
kulisa.load_font("fonts/Inter-Bold.ttf")
local ui = kulisa.ui

local world = kulisa.layer({ parallax = 0 })
local stage = kulisa.group({ parent = world })
-- The camera on the scene: zoomed out while the box is there, in on the PC after (lib/unbox).
local view = kulisa.group({ parent = stage })

-- Fits the stage into the visible part of the screen, centred (the HUD sits in the corners and
-- along the bottom edge, over the desk).
local function fit()
    local x, y, w, h = kulisa.screen:visible()
    local s = math.min(w / Geom.STAGE_W, (h - 40) / Geom.STAGE_H)
    stage.scale = s
    stage.x = x + (w - Geom.STAGE_W * s) / 2
    stage.y = y + (h - Geom.STAGE_H * s) / 2
end
fit()
kulisa.on("resize", fit)
kulisa.on("display", fit)

-- Design point -> scene point (inside the view).
local function to_stage(x: number, y: number): (number, number)
    local sx, sy = (x - stage.x) / stage.scale, (y - stage.y) / stage.scale
    return (sx - view.x) / view.scale, (sy - view.y) / view.scale
end

-- The scene -----------------------------------------------------------------------------

-- Everything on the PC (its render, the glass, films, bag) is in one group, lifted out of the box.
local pc = kulisa.group({ parent = view, z = 15 })
local case_art, case_sprite = PcView.build(view, pc)
Prints.init(FONT)
local print_host = kulisa.group({ parent = stage })
local films_group = kulisa.group({ parent = pc, z = 20 })

-- One film per glass panel, and the sticker over the I/O on top.
local side = Film.new({
    parent = films_group, spans = { { face = Geom.side, len = Geom.side.width } }, b0 = 0, height = Geom.side.height,
    cols = 48, rows = 36, z = 20, kind = "clear", tab = { 100, 260 }, corner = true, wedge = 1,
    image = Prints.side(print_host),
})
local front = Film.new({
    parent = films_group, spans = { { face = Geom.front, len = Geom.front.width } }, b0 = 0, height = Geom.front.height,
    cols = 24, rows = 36, z = 21, kind = "clear", tab = { 70, 220 }, corner = true, wedge = 1, rscale = 0.7,
    image = Prints.front(print_host),
})
local sticker = Film.new({
    parent = films_group, spans = { { face = Geom.top, len = Geom.top.width } }, b0 = 0, height = Geom.top.height,
    cols = 14, rows = 6, z = 24, kind = "sticker", tab = { 40, 40 }, rscale = 0.4, gloss = 0.7, wedge = 0.4,
    image = Prints.sticker(print_host), wrinkle = 0.02,
})
local films = { side = side, front = front, sticker = sticker }
-- What is drawn on top is grabbed first.
local film_order = { sticker, front, side }
local bag = Bag.new(pc, Prints.bag(print_host, Bag.sizes()), 40)
local spare = Film.new({
    parent = pc, spans = { { face = Geom.side, len = Geom.side.width } }, b0 = 0, height = Geom.side.height,
    cols = 48, rows = 48, z = 26, kind = "clear", tab = { 1, 1 }, image = Prints.protector(print_host),
})
local protector = Protector.new(spare, view, 60)
-- Scene point -> design point.
local function to_design(sx: number, sy: number): (number, number)
    local x, y = sx * view.scale + view.x, sy * view.scale + view.y
    return x * stage.scale + stage.x, y * stage.scale + stage.y
end
local unbox = Unbox.new(view, pc, bag, case_sprite, to_design, films_group)

local State = require("lib/state")
State.bag, State.films, State.protector, State.unbox, State.stage, State.pc = bag, films, protector, unbox, stage, pc
State.view, State.to_design = view, to_design

-- State and HUD ---------------------------------------------------------------------------

local tasks = {
    { key = "tape", label = "Peel the tape off the box", done = ui.signal(false) },
    { key = "flaps", label = "Open the box", done = ui.signal(false) },
    { key = "foam", label = "Take out the foam", done = ui.signal(false) },
    { key = "lift", label = "Lift the PC out", done = ui.signal(false) },
    { key = "bag", label = "Pull the bag off over the top", done = ui.signal(false) },
    { key = "side", label = "Peel the side glass film", done = ui.signal(false) },
    { key = "front", label = "Peel the front glass film", done = ui.signal(false) },
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

State.done = function(key: string): boolean
    for _, t in tasks do
        if t.key == key then
            return t.done:peek()
        end
    end
    return false
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
    if not task("tape").done:peek() then
        hint:set("Grab the end of the tape and pull it along the seam")
    elseif not task("flaps").done:peek() then
        hint:set("Pull the flaps open")
    elseif not task("foam").done:peek() then
        hint:set("Drag the foam blocks out of the box")
    elseif not task("lift").done:peek() then
        hint:set("Lift the PC up and out of the box")
    elseif not task("bag").done:peek() then
        hint:set("Grab the bag and pull it up, off over the top of the PC")
    elseif remaining() > 0 then
        hint:set("Grab a film by its corner tab and peel it back. Faster pulls crinkle more")
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
    if applying:peek() or not task("side").done:peek() then
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
    unbox:reset()
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


unbox.on_task = function(key: string)
    complete(key)
end
unbox.on_done = function()
    refresh_hint()
end

bag.on_slide_off = function(lean: number)
    Sfx.whoosh(600, mid_x() + lean * 300)
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
            applying() == false and task("side").done() and ui.button({
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
    if unbox:active() then
        local obj = unbox:target(sx, sy)
        if obj then
            return obj, "box"
        end
        return nil, nil
    end
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
    elseif key == "u" and unbox:active() then
        unbox:skip()
    elseif key == "b" and not bag.gone and not unbox:active() then
        -- Shortcut: whip the bag off at once.
        bag.asleep = false
        bag.fling = true
    end
end)

-- Frame loop -----------------------------------------------------------------------------

local frames, frame_time, script_time = 0, 0, 0
local cost_bag, cost_films, cost_protector = 0, 0, 0
kulisa.on("update", function(dt)
    local t0 = os.clock()
    Prints.tick()
    unbox:update(dt)
    bag:update(dt)
    local t1 = os.clock()
    protector:update(dt)
    local t2 = os.clock()
    for _, film in film_order do
        film:update(dt)
    end
    local t3 = os.clock()
    cost_bag += t1 - t0
    cost_protector += t2 - t1
    cost_films += t3 - t2
    -- Sound and wrinkles follow how fast each held thing is being dragged.
    for _, g in grabs do
        local obj = g.obj
        local x, y
        if g.kind == "bag" then
            x, y = obj.lx or g.lx, obj.ly or g.ly
        elseif g.kind == "tool" or g.kind == "box" then
            x, y = g.lx, g.ly
        else
            x, y = obj:corner_screen()
        end
        local speed = math.sqrt((x - g.lx) ^ 2 + (y - g.ly) ^ 2) / math.max(dt, 1e-3)
        g.speed += (speed - g.speed) * math.min(1, dt * 12)
        g.lx, g.ly = x, y
        if g.kind == "bag" then
            Sfx.rustle(g.speed, x)
        elseif g.kind == "tool" or g.kind == "box" then
            -- (the protector and the box make their own sounds)
        else
            obj:crinkle(math.min(0.2, g.speed * dt * 0.0005))
            Sfx.crinkle(g.speed, x)
        end
    end
    script_time += os.clock() - t0
    frames += 1
    frame_time += dt
    if frame_time >= 0.5 then
        local ms = 1000 / frames
        stats_text:set(string.format(
            "%.0f fps  ·  script %.2f ms/frame  (bag %.2f, films %.2f, protector %.2f)",
            frames / frame_time, script_time * ms, cost_bag * ms, cost_films * ms, cost_protector * ms
        ))
        frames, frame_time, script_time = 0, 0, 0
        cost_bag, cost_films, cost_protector = 0, 0, 0
    end
end)

kulisa.on("error", function(message)
    hint:set("Script error: " .. message:split("\n")[1])
end)
