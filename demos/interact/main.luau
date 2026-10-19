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
kulisa.screen.clear = "#16151d"
local fonts = kulisa.load_font("fonts/Inter-Regular.ttf")
local FONT = fonts[1]
local ui = kulisa.ui

local PALETTE = { slot = "#2a2836", slot_hot = "#4a4670", ok = "#7bdcb5", bad = "#e8718d", text = "#ece8f5", dim = "#8d89a0" }

-- The last thing that happened, shown in the HUD.
local last = ui.signal("move the mouse, touch, or press a pad button")
local function note(text: string)
    last:set(text)
end

local function label(text: string, x: number, y: number, size: number?, color: string?)
    return kulisa.text({ text = text, x = x, y = y, size = size or 22, font = FONT, color = color or PALETTE.dim })
end

-- 1. Cloth: a mesh grid whose vertices are rewritten every frame through a buffer -----------

label("Mesh: drag the flag", 60, 52)
local COLS, ROWS, FW, FH = 16, 10, 320, 200
local flag = kulisa.mesh({ image = "images/flag.png", cols = COLS, rows = ROWS, width = FW, height = FH, x = 60, y = 90 })
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

label("Drag and drop: coins, gems, slots", 440, 52)
local function slot(x: number, y: number, name: string, accept, capacity: number?)
    local s = kulisa.sprite({ x = x, y = y, width = 110, height = 110, tint = PALETTE.slot })
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
    local s = kulisa.sprite({ image = image, x = x, y = y, center = true })
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

piece("images/coin.png", 480, 130, "coin")
piece("images/coin.png", 560, 130, "coin")
piece("images/gem.png", 640, 130, "gem")
piece("images/gem.png", 720, 130, "gem")
piece("images/coin.png", 800, 130, "coin")

-- 3. Hotspots with alpha ---------------------------------------------------------------------

label("Hotspots: the ring and the square", 900, 52)
local square = kulisa.sprite({ x = 1000, y = 190, width = 60, height = 60, center = true, tint = PALETTE.ok })
local clicks = { ring = 0, square = 0 }
kulisa.hotspot(square, {
    on_click = function()
        clicks.square += 1
        note(`square clicked through the ring's hole ({clicks.square})`)
        square:tween({ rotation = square.rotation + math.pi / 2 }, 0.25)
    end,
})
local ring = kulisa.sprite({ image = "images/ring.png", x = 900, y = 90, center = true })
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

label("Pinch and rotate (two fingers)", 880, 330)
local arrow = kulisa.sprite({ image = "images/arrow.png", x = 1030, y = 480, center = true })
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
local stick = kulisa.sprite({ width = 18, height = 18, center = true, tint = PALETTE.dim, x = 1180, y = 520 })
kulisa.on("update", function(dt)
    local ax, ay = kulisa.input.pad_axis("lx"), kulisa.input.pad_axis("ly")
    stick.x = 1180 + ax * 40
    stick.y = 520 + ay * 40
    stick.tint = if kulisa.input.pad_down("a") then PALETTE.ok else PALETTE.dim
end)
label("left stick", 1150, 570, 16)

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
        padding = { 8, 18 },
        background = "#2f2c40",
        hover_background = "#4a4670",
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
                background = "#25232f",
                hover_background = "#34314a",
                selected_background = "#3d3a5c",
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
