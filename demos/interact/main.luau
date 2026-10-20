-- Interaction demo: meshes, alpha hit testing, drag and drop, hotspots, multi-touch, gamepad
-- and keyboard navigation, text input, timers, haptics and motion sensors, all through the
-- public `kulisa` API. Every part works with a mouse, a touch screen or a gamepad:
--   * the flag is a deformable mesh: press on it and drag to pull the cloth
--   * drag the coins and gems into the slots (each takes its own kind; the third takes two)
--   * the pink ring is a hotspot that ignores its see-through middle; the square in the hole
--     is a second hotspot clicks fall through to
--   * two fingers pinch and rotate the green arrow (mouse: wheel scales, Q / E rotate)
--   * the HUD at the bottom is navigable with arrows / d-pad + Enter / A

kulisa.display(1280, 720, "expand")
kulisa.screen.clear = "#0f1224"
local fonts = kulisa.load_font("fonts/Inter-Regular.ttf")
kulisa.load_font("fonts/Inter-Bold.ttf")
local FONT = fonts[1]
local ui = kulisa.ui

local PALETTE = { slot = "#2d3050", slot_hot = "#5b63a8", ok = "#6be3b0", bad = "#ff7b9c", text = "#f1eefa", dim = "#9aa0c4" }

-- Backdrop and cards ---------------------------------------------------------------------------

local backdrop = kulisa.layer({ parallax = 0, z = -100 })
kulisa.sprite({
    parent = backdrop, x = -800, y = -500, width = 3000, height = 1900, shader = kulisa.load_shader("shaders/bg.wgsl"),
    params = { 3000, 1900 },
})

-- A rounded card with a colored title bar behind each demo.
local function card(x: number, y: number, w: number, h: number, title: string, accent: string)
    local back = kulisa.group({ x = x, y = y, z = -50 })
    kulisa.sprite({ parent = back, image = "images/panel.png", slice = 22, width = w, height = h, tint = "#141730", opacity = 0.82 })
    kulisa.sprite({ parent = back, x = 20, y = 18, width = 5, height = 24, tint = accent })
    kulisa.text({ parent = back, x = 36, y = 14, text = "{b}" .. title .. "{/b}", size = 22, font = "Inter", color = "#f1eefa" })
    return back
end

-- The last thing that happened, shown in the HUD.
local last = ui.signal("move the mouse, touch, or press a pad button")
local function note(text: string)
    last:set(text)
end

local function label(text: string, x: number, y: number, size: number?, color: string?)
    return kulisa.text({ text = text, x = x, y = y, size = size or 22, font = FONT, color = color or PALETTE.dim })
end

-- 1. Cloth: a mesh grid whose vertices are rewritten every frame through a buffer -----------

card(40, 24, 370, 316, "Cloth: drag the flag", "#ff7b9c")
local COLS, ROWS, FW, FH = 16, 10, 320, 200 -- the flag, hanging from its pole
local flag = kulisa.mesh({ image = "images/flag.png", cols = COLS, rows = ROWS, width = FW, height = FH, x = 64, y = 100 })
local positions = buffer.create((COLS + 1) * (ROWS + 1) * 8)
local grabbed: number? = nil -- the pointer id pulling the flag

kulisa.on("press", function(x, y, _, id)
    if flag:contains(x, y) then
        grabbed = id
        note(`flag grabbed by pointer {id}`)
    end
end)
kulisa.on("release", function(_, _, _, id)
    if grabbed == id then
        grabbed = nil
    end
end)

local function pointer_in_flag()
    if grabbed then
        for _, p in kulisa.input.pointers() do
            if p.id == grabbed then
                return p.x - flag.x, p.y - flag.y
            end
        end
    end
    return nil, nil
end

kulisa.on("update", function()
    local t = kulisa.now()
    local px, py = pointer_in_flag()
    local i = 0
    for r = 0, ROWS do
        for c = 0, COLS do
            local u, v = c / COLS, r / ROWS
            local x = u * FW
            local y = v * FH + math.sin(t * 3 + u * 6 + v * 2) * 9 * u
            if px then
                local dx, dy = px - x, py - y
                local pull = math.exp(-(dx * dx + dy * dy) / (2 * 70 * 70)) * 0.85
                x += dx * pull
                y += dy * pull
            end
            buffer.writef32(positions, i * 8, x)
            buffer.writef32(positions, i * 8 + 4, y)
            i += 1
        end
    end
    flag:set_positions(positions)
end)

-- 2. Drag and drop ---------------------------------------------------------------------------

card(430, 24, 450, 530, "Drag and drop: coins, gems, slots", "#ffd166")
local function slot(x: number, y: number, name: string, accept, capacity: number?)
    local s = kulisa.sprite({ image = "images/slot.png", slice = 18, x = x, y = y, width = 110, height = 110, tint = PALETTE.slot })
    label(name, x + 6, y + 114, 18)
    local zone
    zone = kulisa.drop_zone(s, {
        accept = accept,
        capacity = capacity,
        snap = "center",
        on_enter = function()
            s.tint = PALETTE.slot_hot
        end,
        on_leave = function()
            s.tint = PALETTE.slot
        end,
        on_drop = function()
            s.tint = PALETTE.slot
            kulisa.haptics.pulse(0.6, 0.04)
            note(`{name}: accepted`)
        end,
        on_reject = function()
            s.tint = PALETTE.slot
            s:tween({ x = x + 8 }, { time = 0.04, yoyo = true, ["repeat"] = 3 })
            kulisa.haptics.pulse(0.3, 0.12)
            note(`{name}: refused`)
        end,
    })
    return s, zone
end

slot(440, 400, "coins only", "coin")
slot(580, 400, "gems only", "gem")
slot(720, 400, "any, two at most", function(info)
    return info.node ~= nil
end, 2)

local function piece(image: string, x: number, y: number, tag: string)
    local s = kulisa.sprite({ image = image, x = x, y = y, width = 84, height = 84, center = true })
    kulisa.drag(s, {
        tag = tag,
        alpha = 0.2,
        on_start = function()
            s:tween({ scale = 1.15 }, 0.08)
        end,
        on_end = function()
            s:tween({ scale = 1 }, 0.1)
        end,
        on_click = function()
            note(`{tag}: click (press and release without moving)`)
        end,
    })
    return s
end

piece("images/coin.png", 490, 160, "coin")
piece("images/coin.png", 578, 160, "coin")
piece("images/gem.png", 666, 160, "gem")
piece("images/gem.png", 754, 160, "gem")
piece("images/coin.png", 842, 160, "coin")

-- 3. Hotspots with alpha ---------------------------------------------------------------------

card(890, 24, 360, 250, "Hotspots: ring and square", "#6be3b0")
local square = kulisa.sprite({ x = 1000, y = 190, width = 60, height = 60, center = true, tint = PALETTE.ok })
local clicks = { ring = 0, square = 0 }
kulisa.hotspot(square, {
    on_click = function()
        clicks.square += 1
        note(`square clicked through the ring's hole ({clicks.square})`)
        square:tween({ rotation = square.rotation + math.pi / 2 }, 0.25)
    end,
})
local ring = kulisa.sprite({ image = "images/ring.png", x = 900, y = 90, width = 170, height = 170, center = true })
ring.x, ring.y = 1000, 190
kulisa.hotspot(ring, {
    alpha = true,
    on_enter = function()
        ring.tint = "#ffffff"
        ring:tween({ scale = 1.06 }, 0.1)
    end,
    on_leave = function()
        ring:tween({ scale = 1 }, 0.1)
    end,
    on_click = function()
        clicks.ring += 1
        note(`ring clicked ({clicks.ring})`)
        ring:tween({ rotation = ring.rotation + 1 }, { time = 0.3, ease = "out_back" })
        kulisa.haptics.pulse(0.4, 0.03)
    end,
})

-- 4. Multi-touch: pinch and rotate -----------------------------------------------------------

card(890, 292, 360, 262, "Pinch and rotate (two fingers)", "#7aa2ff")
local arrow = kulisa.sprite({ image = "images/arrow.png", x = 1060, y = 440, width = 150, height = 150, center = true })
kulisa.on("pinch", function(factor)
    arrow.scale = math.clamp(arrow.scale * factor, 0.3, 3)
    note(`pinch x{string.format("%.2f", factor)}`)
end)
kulisa.on("rotate", function(delta)
    arrow.rotation += delta
    note(`rotate {string.format("%.2f", delta)} rad`)
end)
kulisa.on("wheel", function(lines)
    arrow.scale = math.clamp(arrow.scale * (1 + lines * 0.08), 0.3, 3)
end)
kulisa.on("update", function(dt)
    if not kulisa.input.text_active() then
        if kulisa.input.down("q") then
            arrow.rotation -= dt * 2
        elseif kulisa.input.down("e") then
            arrow.rotation += dt * 2
        end
    end
end)

-- A dot that follows the left stick, and lights up while A is held.
local stick = kulisa.sprite({ width = 18, height = 18, center = true, tint = PALETTE.dim, x = 1190, y = 480 })
kulisa.on("update", function(dt)
    local ax, ay = kulisa.input.pad_axis("lx"), kulisa.input.pad_axis("ly")
    stick.x = 1190 + ax * 40
    stick.y = 480 + ay * 40
    stick.tint = if kulisa.input.pad_down("a") then PALETTE.ok else PALETTE.dim
end)
label("left stick", 1160, 520, 16)

-- 5. HUD: status lines, buttons and a text field, navigable with arrows or a d-pad --------------

local name = ui.signal("")
local ticks = ui.signal(0)
local tilt_text = ui.signal("sensors off")
local sensing = ui.signal(false)
kulisa.every(0.5, function()
    ticks:set(ticks:peek() + 1)
end)
kulisa.every(0.1, function()
    if sensing:peek() then
        local x, y = kulisa.sensors.tilt()
        tilt_text:set(if x then `tilt {string.format("%.2f", x)}, {string.format("%.2f", y)}` else "waiting for a reading")
    end
end)

local function button(text: string, id: string, on_click)
    return ui.button({
        id = id,
        on_click = on_click,
        padding = { 10, 20 },
        background = "images/button.png",
        slice = 14,
        tint = "#3a3f72",
        hover_tint = "#5560b0",
        ui.text({ text = text, size = 22, font = FONT, color = PALETTE.text }),
    })
end

ui.screen("hud", function()
    return ui.box({ ui.column({
        anchor = "bottom_left",
        margin = 20,
        gap = 10,
        ui.text({ text = last, size = 20, font = FONT, color = PALETTE.text, width = 760 }),
        ui.text({
            text = `timer ticks {ticks()}  |  pointers {#kulisa.input.pointers()}  |  pads {#kulisa.input.pads()}  |  {tilt_text()}`,
            size = 16,
            font = FONT,
            color = PALETTE.dim,
        }),
        ui.row({
            gap = 12,
            align = "center",
            ui.input({
                id = "name",
                value = name,
                placeholder = "type your name",
                max_length = 16,
                width = 260,
                padding = { 8, 14 },
                background = "images/button.png",
                slice = 14,
                tint = "#262a4a",
                hover_tint = "#343a66",
                selected_tint = "#3d4580",
                size = 22,
                font = FONT,
                color = PALETTE.text,
                on_submit = function(text)
                    note(`hello, {text}`)
                    kulisa.haptics.pulse(0.5, 0.05)
                end,
            }),
            button("Buzz", "buzz", function()
                kulisa.haptics.pulse(1, 0.2)
                note("haptics: pulse")
            end),
            button(if sensing() then "Sensors: on" else "Sensors: off", "sensors", function()
                sensing:set(not sensing:peek())
                kulisa.sensors.enable(sensing:peek())
                note(if kulisa.sensors.available() then "sensors toggled" else "this device has no motion sensors")
            end),
        }),
    }) })
end, { z = 100, area = "safe", nav = true })
ui.show("hud")

-- Everything else that happens, for the status line ----------------------------------------

kulisa.on("tap", function(x, y)
    note(`tap {math.floor(x)}, {math.floor(y)}`)
end)
kulisa.on("long_press", function()
    note("long press")
end)
kulisa.on("swipe", function(dir)
    note(`swipe {dir}`)
end)
kulisa.on("pad_press", function(button, pad)
    note(`pad {pad}: {button}`)
end)
kulisa.on("pad_connect", function(pad, pad_name)
    note(`pad {pad} connected: {pad_name}`)
end)
kulisa.on("key", function(key)
    if not kulisa.input.text_active() and #key > 1 then
        note(`key {key}`)
    end
end)
kulisa.on("error", function(message)
    note(`error: {message:split("\n")[1]}`)
end)
