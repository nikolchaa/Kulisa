-- Parallax demo: a valley in many layers (shader sky, terrain-rendered mountains, painted forests,
-- GPU particles, a day-night cycle), a ray-marched diorama brought to life by its depth map, post-processing
-- (bloom, tilt-shift, grading, CRT, grain), and a display lab that shows one scene on every
-- screen shape. Built only on the public `kulisa` API. See README.md.
--
-- Keys: 1 valley, 2 diorama, Tab hide the panel, B bloom, T tilt-shift, G grading, C CRT,
-- N grain, D display overlay, M scale mode, A aspect clamp, F3 stats; on desktop F6-F10
-- reshape the window (16:9, 21:9, 32:9, 4:3, 9:16). The pointer (or tilting a phone) moves the
-- camera.

-- Script time per frame: from the first update handler (this one) to the last (the stats).
local frame_start = 0
kulisa.on("update", function()
    frame_start = os.clock()
end)

local Lab = require("lib/display_lab")
Lab.apply()
kulisa.screen.clear = "#05060c"
local FONT = kulisa.load_font("fonts/Inter-Regular.ttf")[1]
kulisa.load_font("fonts/Inter-Bold.ttf")

local Valley = require("lib/valley")
local Diorama = require("lib/diorama")
local Fx = require("lib/fx")

local ui = kulisa.ui
local phone = kulisa.screen.profile == "phone"
local S = function(n: number): number
    return if phone then math.floor(n * 1.6) else n
end

-- The world: one layer with the post-processing chain; scenes inside it have their own parallax.
local world = kulisa.layer({ parallax = 1, z = 0 })
Valley.build(world)
Diorama.build(world)
Fx.apply(world)

local state = {
    scene = ui.signal("valley"),
    cycle = ui.signal(false),
    camera = ui.signal(if kulisa.sensors.available() then "tilt" else "pointer"),
    panel = ui.signal(not phone),
    stats = ui.signal(true),
    stats_text = ui.signal(""),
    revision = ui.signal(0), -- bumps when anything shown on the panel changes
}

local function changed()
    state.revision:set(state.revision:peek() + 1)
end

local function set_scene(name: string)
    state.scene:set(name)
    Valley.show(name == "valley")
    Diorama.show(name == "diorama")
    changed()
end

local function relayout()
    Valley.place()
    Diorama.place()
    Lab.update()
    ui.refresh()
end
kulisa.on("resize", relayout)
kulisa.on("display", relayout)
Lab.on_change = relayout

-- Camera ------------------------------------------------------------------------------------------

local look_x, look_y = 0, 0 -- -1..1: where the viewer looks from
local cam_x, cam_y = 0, 0

local function input_look(): (number, number)
    local mode = state.camera:peek()
    if mode == "tilt" then
        local tx, ty = kulisa.sensors.tilt()
        if tx then
            return math.clamp(tx * 1.6, -1, 1), math.clamp(-ty * 1.6, -1, 1)
        end
    elseif mode == "pointer" then
        local px, py = kulisa.input.pointer()
        if px then
            local vx, vy, vw, vh = kulisa.screen:visible()
            return math.clamp((px - vx) / vw * 2 - 1, -1, 1), math.clamp((py - vy) / vh * 2 - 1, -1, 1)
        end
    end
    local t = kulisa.now()
    return math.sin(t * 0.21) * 0.7, math.sin(t * 0.13 + 1) * 0.35
end

kulisa.on("update", function(dt)
    local tx, ty = input_look()
    local k = 1 - math.exp(-dt * 4)
    look_x += (tx - look_x) * k
    look_y += (ty - look_y) * k
    -- The camera pans a little; layers move by their parallax factor.
    local cx, cy = kulisa.screen.width / 2, kulisa.screen.height / 2
    cam_x, cam_y = look_x * 70, look_y * 30
    kulisa.camera.x, kulisa.camera.y = cx + cam_x, cy + cam_y
    Diorama.view(look_x, look_y, dt)
    if state.cycle:peek() then
        Valley.set_time(Valley.time + dt / 60)
    end
end)

-- Panel -------------------------------------------------------------------------------------------

local C = {
    panel = "#0b0d16dc",
    text = "#e8ebf5",
    dim = "#9aa1b8",
    accent = "#ffc857",
    off = "#ffffff12",
    hover = "#ffffff26",
}

local function label(text: string, opts)
    opts = opts or {}
    opts.text = text
    opts.size = opts.size or S(15)
    opts.font = FONT
    opts.color = opts.color or C.text
    return ui.text(opts)
end

local function heading(text: string)
    return label(`\{b}{string.upper(text)}\{/b}`, { size = S(12), color = C.dim })
end

local function toggle(text: string, on: boolean, f, id: string?)
    return ui.button({
        id = id,
        key = id or text,
        selected = on,
        padding = { S(7), S(12) },
        background = "gui/round.png",
        slice = 10,
        slice_scale = 0.6,
        tint = C.off,
        hover_tint = C.hover,
        selected_tint = C.accent,
        cursor = "pointer",
        on_click = function()
            f()
            changed()
        end,
        label(text, { selected_color = "#171307", size = S(14) }),
    })
end

local function row(items)
    local r = { gap = S(6), width = "fill" }
    for _, it in items do
        table.insert(r, it)
    end
    return ui.row(r)
end

local function wrap(items, per_row: number)
    local rows = {}
    for i = 1, #items, per_row do
        local r = {}
        for j = i, math.min(i + per_row - 1, #items) do
            table.insert(r, items[j])
        end
        table.insert(rows, row(r))
    end
    return ui.column({ gap = S(6), width = "fill", rows })
end

local function panel()
    state.revision()
    local scene = state.scene()
    local items = {
        label(`\{b}Parallax\{/b}`, { size = S(22) }),
        label("Kulisa: layers, particles, depth maps, post-fx", { size = S(12), color = C.dim }),
        heading("Scene"),
        row({
            toggle("Valley", scene == "valley", function() set_scene("valley") end, "scene_valley"),
            toggle("Diorama", scene == "diorama", function() set_scene("diorama") end, "scene_diorama"),
        }),
    }
    if scene == "valley" then
        table.insert(items, heading("Time of day"))
        table.insert(items, ui.slider({
            id = "time",
            value = Valley.time,
            min = 0,
            max = 1,
            width = "fill",
            height = S(16),
            thumb_size = S(20),
            background = "gui/round.png",
            slice = 10,
            slice_scale = 0.5,
            tint = C.off,
            bar = { image = "gui/round.png", tint = "#ffc85788" },
            thumb = { image = "gui/round.png", tint = C.accent },
            on_change = function(v)
                Valley.set_time(v)
            end,
        }))
        table.insert(items, row({ toggle("Cycle", state.cycle(), function() state.cycle:set(not state.cycle:peek()) end, "cycle") }))
        local w = Valley.weather
        local function weather(key, text)
            return toggle(text, w[key], function()
                w[key] = not w[key]
                Valley.set_weather(w)
            end, `w_{key}`)
        end
        table.insert(items, heading("Particles"))
        table.insert(items, wrap({ weather("fireflies", "Fireflies"), weather("leaves", "Leaves"), weather("snow", "Snow"),
            weather("smoke", "Smoke"), weather("embers", "Embers") }, 3))
    end
    table.insert(items, heading("Camera"))
    local cams = { toggle("Pointer", state.camera() == "pointer", function() state.camera:set("pointer") end, "cam_pointer") }
    if kulisa.sensors.available() then
        table.insert(cams, toggle("Tilt", state.camera() == "tilt", function()
            kulisa.sensors.enable(true)
            state.camera:set("tilt")
        end, "cam_tilt"))
    end
    table.insert(cams, toggle("Drift", state.camera() == "drift", function() state.camera:set("drift") end, "cam_drift"))
    table.insert(items, row(cams))

    local function fx(key, text)
        return toggle(text, Fx.on[key], function()
            Fx.on[key] = not Fx.on[key]
            Fx.apply(world)
        end, `fx_{key}`)
    end
    table.insert(items, heading("Post-processing"))
    table.insert(items, wrap({ fx("bloom", "Bloom"), fx("tiltshift", "Tilt-shift"), fx("grade", "Grade"), fx("crt", "CRT"), fx("grain", "Grain") }, 3))
    if Fx.on.grade then
        local grades = {}
        for i, g in Fx.grades do
            table.insert(grades, toggle(g.name, Fx.grade == i, function()
                Fx.grade = i
                Fx.apply(world)
            end, `grade{i}`))
        end
        table.insert(items, wrap(grades, 3))
    end

    table.insert(items, heading("Display"))
    table.insert(items, row({ toggle("Overlay", Lab.overlay, function()
        Lab.overlay = not Lab.overlay
        Lab.update()
    end, "overlay"), toggle("Aspect clamp", Lab.clamp, function()
        Lab.clamp = not Lab.clamp
        Lab.apply()
    end, "clamp") }))
    local modes = {}
    for _, m in { "expand", "fit", "fill" } do
        table.insert(modes, toggle(m:sub(1, 1):upper() .. m:sub(2), kulisa.screen.mode == m, function()
            Lab.config.scale = m
            Lab.apply()
        end, `mode_{m}`))
    end
    table.insert(items, row(modes))
    if kulisa.window.can_resize then
        local shapes = {}
        for _, s in Lab.shapes do
            table.insert(shapes, toggle(s.name, false, function()
                kulisa.window:resize(s.w, s.h)
            end, `shape_{s.name}`))
        end
        table.insert(items, wrap(shapes, 5))
    end
    if Lab.overlay then
        table.insert(items, label(Lab.describe(), { size = S(12), color = C.dim, width = "fill", line_height = 1.6 }))
    end
    table.insert(items, label(if phone then "Tap Settings to hide" else "Tab hides this panel", { size = S(11), color = C.dim }))
    return items
end

ui.screen("panel", function()
    if not state.panel() then
        return ui.box({
            phone and ui.button({
                id = "open_panel",
                anchor = "bottom_right",
                margin = S(14),
                padding = { S(10), S(16) },
                background = "gui/round.png",
                slice = 10,
                tint = C.panel,
                on_click = function()
                    state.panel:set(true)
                end,
                label("Settings"),
            }),
        })
    end
    local items = panel()
    local body = ui.scroll({ width = "fill", height = "fill", gap = S(10), items })
    if phone then
        local _, _, _, sh = kulisa.screen:safe()
        return ui.box({
            ui.column({
                anchor = "bottom",
                width = "fill",
                height = sh * 0.5,
                padding = S(16),
                gap = S(10),
                background = "gui/round.png",
                slice = 10,
                tint = C.panel,
                block = true,
                ui.row({ width = "fill", ui.spacer({}), toggle("Settings", true, function() state.panel:set(false) end, "close_panel") }),
                body,
            }),
        })
    end
    return ui.box({
        ui.column({
            anchor = "top_right",
            margin = S(16),
            width = S(330),
            max_height = select(4, kulisa.screen:safe()) - S(32),
            padding = S(18),
            gap = S(10),
            background = "gui/round.png",
            slice = 10,
            tint = C.panel,
            block = true,
            body,
        }),
    })
end, { z = 100, area = "safe" })

ui.screen("stats", function()
    return ui.box({
        label(state.stats_text(), { anchor = "top_left", margin = S(14), size = S(13), color = "#cfd5e6", visible = state.stats() }),
    })
end, { z = 99, area = "safe" })

-- Badges in the safe area's corners: they stay clear of notches and rounded corners.
ui.screen("corners", function()
    state.revision()
    if not Lab.overlay then
        return ui.box({})
    end
    local function badge(anchor: string, text: string)
        return ui.box({
            anchor = anchor,
            padding = { S(4), S(8) },
            background = "gui/round.png",
            slice = 10,
            slice_scale = 0.5,
            tint = "#68e08acc",
            label(text, { size = S(11), color = "#05120a" }),
        })
    end
    return ui.box({
        badge("top_left", "safe top-left"),
        badge("top_right", "safe top-right"),
        badge("bottom_left", "safe bottom-left"),
        badge("bottom_right", "safe bottom-right"),
    })
end, { z = 98, area = "safe" })

ui.show("panel")
ui.show("stats")
ui.show("corners")

-- Keys ----------------------------------------------------------------------------------------------

local function toggle_fx(key: string)
    Fx.on[key] = not Fx.on[key]
    Fx.apply(world)
    changed()
end

kulisa.on("key", function(key)
    if key == "1" then
        set_scene("valley")
    elseif key == "2" then
        set_scene("diorama")
    elseif key == "tab" then
        state.panel:set(not state.panel:peek())
    elseif key == "b" then
        toggle_fx("bloom")
    elseif key == "t" then
        toggle_fx("tiltshift")
    elseif key == "g" then
        toggle_fx("grade")
    elseif key == "c" then
        toggle_fx("crt")
    elseif key == "n" then
        toggle_fx("grain")
    elseif key == "d" then
        Lab.overlay = not Lab.overlay
        Lab.update()
        changed()
    elseif key == "a" then
        Lab.clamp = not Lab.clamp
        Lab.apply()
        changed()
    elseif key == "m" then
        local order = { expand = "fit", fit = "fill", fill = "expand" }
        Lab.config.scale = order[kulisa.screen.mode] or "expand"
        Lab.apply()
        changed()
    elseif key == "f3" then
        state.stats:set(not state.stats:peek())
    else
        local n = ({ f6 = 1, f7 = 2, f8 = 3, f9 = 4, f10 = 5 })[key]
        if n and kulisa.window.can_resize then
            local s = Lab.shapes[n]
            kulisa.window:resize(s.w, s.h)
        end
    end
end)

-- Stats ---------------------------------------------------------------------------------------------

local frames, elapsed, script = 0, 0, 0
kulisa.on("update", function(dt)
    frames += 1
    elapsed += dt
    if elapsed >= 0.5 then
        local alive = if state.scene:peek() == "valley" then Valley.alive() else Diorama.alive()
        state.stats_text:set(string.format("%.0f fps  ·  %.2f ms/frame  ·  script %.2f ms  ·  %d particles",
            frames / elapsed, elapsed / frames * 1000, script / frames * 1000, alive))
        frames, elapsed, script = 0, 0, 0
    end
    script += os.clock() - frame_start
end)

set_scene("valley")
Lab.update()
