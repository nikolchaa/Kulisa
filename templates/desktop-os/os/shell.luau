-- The shell: wallpaper, desktop icons and clock, taskbar, start menu, notifications (toasts),
-- story captions, an objective hint, dialogs and the sign-in screen.

local config = require("os/config")
local theme = require("os/theme")
local wm = require("os/wm")

local ui = kulisa.ui
local c = config.colors
local S = theme.size

local shell = {}

-- Wallpaper -----------------------------------------------------------------------------------

local function linear(hex: string): (number, number, number)
    local r, g, b = tonumber(hex:sub(2, 3), 16) / 255, tonumber(hex:sub(4, 5), 16) / 255, tonumber(hex:sub(6, 7), 16) / 255
    return r ^ 2.2, g ^ 2.2, b ^ 2.2
end

-- Params for the wallpaper shader from a wallpaper in config.wallpapers.
function shell.wallpaper_params(name: string, aspect: number): { number }
    local w = config.wallpapers[name] or config.wallpapers[config.wallpaper]
    local p = {}
    for i, hex in w.colors do
        local r, g, b = linear(hex)
        table.insert(p, r)
        table.insert(p, g)
        table.insert(p, b)
        table.insert(p, if i == 1 then w.speed elseif i == 2 then aspect else 0.37)
    end
    return p
end

local WALL_SHADER = kulisa.load_shader("shaders/wallpaper.wgsl")
local wall_layer = kulisa.layer({ parallax = 0, z = -1000 })
local wall = kulisa.sprite({ parent = wall_layer, shader = WALL_SHADER })
shell.wallpaper = config.wallpaper

local function fit_wallpaper()
    local x, y, w, h = kulisa.screen:visible()
    wall.x, wall.y, wall.width, wall.height = x, y, w, h
    wall.params = shell.wallpaper_params(shell.wallpaper, w / h)
end

function shell.set_wallpaper(name: string)
    if config.wallpapers[name] then
        shell.wallpaper = name
        fit_wallpaper()
    end
end

-- A small node showing a wallpaper (settings thumbnails).
function shell.wallpaper_preview(name: string, w: number, h: number)
    local s = kulisa.sprite({ shader = WALL_SHADER, width = w, height = h })
    s.params = shell.wallpaper_params(name, w / h)
    return s
end

-- Desktop ---------------------------------------------------------------------------------------

local CELL_W, CELL_H = S(112), S(122)
local icon_pos = {} -- app -> { col, row }
for i, app in config.desktop do
    icon_pos[app] = { 0, i - 1 }
end
local selected = ui.signal(nil)
local dragging = ui.signal(nil) -- { app, dx, dy }
local clock_tick = ui.signal(theme.time_text())
local last_click = { app = nil, at = -1 }

local function occupied(col: number, row: number, except: string): boolean
    for app, p in icon_pos do
        if app ~= except and p[1] == col and p[2] == row then
            return true
        end
    end
    return false
end

local function desktop_icon(app: string)
    local def = config.apps[app]
    local p = icon_pos[app]
    local d = dragging()
    local ox, oy = 0, 0
    if d and d.app == app then
        ox, oy = d.dx, d.dy
    end
    local sel = selected() == app
    return ui.button({
        key = app,
        id = `icon:{app}`,
        x = p[1] * CELL_W + ox,
        y = p[2] * CELL_H + oy,
        width = CELL_W - S(8),
        padding = { S(8), S(4) },
        selected = sel,
        background = "gui/round.png",
        slice = 8,
        tint = "#00000000",
        hover_tint = "#ffffff14",
        selected_tint = "#8ea2ff40",
        cursor = "pointer",
        on_click = function()
            local now = kulisa.now()
            if last_click.app == app and now - last_click.at < 0.45 then
                last_click.at = -1
                selected:set(nil)
                wm.open(app)
            else
                last_click = { app = app, at = now }
                selected:set(app)
                -- One tap opens on touch screens.
                if kulisa.screen.touch then
                    wm.open(app)
                end
            end
        end,
        on_drag = function(e)
            if e.phase == "end" then
                local col = math.max(0, math.floor((p[1] * CELL_W + e.dx) / CELL_W + 0.5))
                local row = math.max(0, math.floor((p[2] * CELL_H + e.dy) / CELL_H + 0.5))
                if not occupied(col, row, app) then
                    icon_pos[app] = { col, row }
                end
                dragging:set(nil)
            else
                selected:set(app)
                dragging:set({ app = app, dx = e.dx, dy = e.dy })
            end
        end,
        ui.column({
            width = "fill",
            gap = S(6),
            align = "center",
            ui.image({ image = def.icon, width = S(64), height = S(64) }),
            theme.label(def.title, { size = S(16), align = "center", width = "fill" }),
        }),
    })
end

ui.screen("desktop", function()
    local icons = {}
    for _, app in config.desktop do
        table.insert(icons, desktop_icon(app))
    end
    local ax, ay = theme.work_area()
    local vx, vy = kulisa.screen:visible()
    return ui.box({
        block = true,
        on_press = function()
            selected:set(nil)
            shell.close_start()
        end,
        ui.box({ x = ax - vx + S(18), y = ay - vy + S(18), icons }),
        -- A clock and date, top right.
        ui.column({
            anchor = "top_right",
            margin = { ay - vy + S(40), S(56), 0, 0 },
            align = "end",
            theme.label(clock_tick(), { size = S(112), color = "#ffffffe6" }),
            theme.label(os.date("%A, %d %B"), { size = S(26), color = "#ffffffb0" }),
        }),
    })
end, { z = 0, area = "screen" })

-- Taskbar ---------------------------------------------------------------------------------------

local start_open = ui.signal(false)
local unread = ui.signal(0) -- notifications not looked at
shell.unread = unread

local function task_button(app: string)
    local def = config.apps[app]
    local win = wm.find(app)
    local focused = win ~= nil and wm.focused() == win.id and not win.minimized
    local sz = theme.taskbar_height() - S(10)
    return ui.button({
        id = `task:{app}`,
        key = app,
        width = sz,
        height = sz,
        selected = focused,
        background = "gui/round.png",
        slice = 8,
        tint = "#00000000",
        hover_tint = "#ffffff18",
        selected_tint = "#ffffff22",
        cursor = "pointer",
        on_click = function()
            shell.close_start()
            if win and focused then
                wm.minimize(win)
            elseif win then
                if win.minimized then
                    wm.restore(win)
                else
                    wm.focus(win)
                end
            else
                wm.open(app)
            end
        end,
        ui.box({ anchor = "center", ui.image({ image = def.icon, width = S(34), height = S(34) }) }),
        -- Running indicator.
        win and ui.box({
            anchor = "bottom",
            y = -S(1),
            width = if focused then S(18) else S(6),
            height = S(4),
            background = "gui/pill.png",
            slice = 15,
            slice_scale = 0.13,
            tint = if focused then c.accent else c.dim,
        }),
    })
end

wm.taskbar_spot = function(app: string)
    local x, y, w, h = ui.bounds("taskbar", `task:{app}`)
    if x then
        return x + w / 2, y + h / 2
    end
    return nil, nil
end

ui.screen("taskbar", function()
    wm.revision()
    local insets = theme.insets()
    local tasks = {}
    for _, app in config.pinned do
        table.insert(tasks, task_button(app))
    end
    local n = unread()
    return ui.box({
        anchor = "bottom",
        width = "fill",
        height = theme.taskbar_height() + insets[3],
        padding = { 0, S(12) + insets[2], insets[3], S(12) + insets[4] },
        background = "gui/taskbar.png",
        slice = { 1, 2, 1, 1 },
        block = true,
        ui.row({
            width = "fill",
            height = "fill",
            align = "center",
            gap = S(6),
            ui.button({
                id = "start",
                width = theme.taskbar_height() - S(10),
                height = theme.taskbar_height() - S(10),
                selected = start_open(),
                background = "gui/round.png",
                slice = 8,
                tint = "#00000000",
                hover_tint = "#ffffff18",
                selected_tint = "#ffffff26",
                cursor = "pointer",
                on_click = function()
                    shell.toggle_start()
                end,
                ui.box({ anchor = "center", ui.image({ image = "apps/start.png", width = S(36), height = S(36) }) }),
            }),
            theme.divider(true),
            tasks,
            ui.spacer({}),
            ui.button({
                id = "bell",
                padding = { S(6), S(10) },
                background = "gui/round.png",
                slice = 8,
                tint = "#00000000",
                hover_tint = "#ffffff18",
                cursor = "pointer",
                on_click = function()
                    unread:set(0)
                    shell.show_history()
                end,
                ui.row({
                    gap = S(6),
                    align = "center",
                    theme.glyph("bell", S(20), if n > 0 then c.accent2 else c.dim),
                    n > 0 and theme.badge(n),
                }),
            }),
            theme.glyph("wifi", S(20), c.dim),
            theme.glyph("volume", S(20), c.dim),
            theme.glyph("battery", S(22), c.dim),
            ui.column({
                padding = { 0, S(8), 0, S(8) },
                align = "end",
                theme.label(clock_tick(), { size = S(16) }),
                theme.label(os.date("%d/%m/%Y"), { size = S(13), color = c.dim }),
            }),
        }),
    })
end, { z = 5000, area = "screen" })

-- Start menu ------------------------------------------------------------------------------------

-- Actions the start menu offers besides apps; main.luau fills them (save, load, power).
shell.actions = {}

function shell.toggle_start()
    start_open:set(not start_open:peek())
    if start_open:peek() then
        ui.show("start")
        local layer = ui.layer("start")
        if layer then
            layer.y, layer.opacity = S(24), 0
            layer:tween({ y = 0, opacity = 1 }, { time = 0.16, ease = "out_cubic" })
        end
    else
        ui.hide("start")
    end
end

function shell.close_start()
    if start_open:peek() then
        shell.toggle_start()
    end
end

ui.screen("start", function()
    local insets = theme.insets()
    local apps = {}
    for _, app in config.pinned do
        local def = config.apps[app]
        table.insert(apps, ui.button({
            key = app,
            padding = S(10),
            background = "gui/round.png",
            slice = 8,
            tint = "#00000000",
            hover_tint = c.hover,
            pressed_tint = c.pressed,
            cursor = "pointer",
            on_click = function()
                shell.close_start()
                wm.open(app)
            end,
            ui.column({
                width = "fill",
                align = "center",
                gap = S(8),
                ui.image({ image = def.icon, width = S(56), height = S(56) }),
                theme.label(def.title, { size = S(15), align = "center" }),
            }),
        }))
    end
    local actions = {}
    for _, a in shell.actions do
        table.insert(actions, ui.button({
            key = a.label,
            padding = { S(8), S(12) },
            background = "gui/round.png",
            slice = 8,
            tint = "#00000000",
            hover_tint = c.hover,
            cursor = "pointer",
            on_click = function()
                shell.close_start()
                a.run()
            end,
            ui.row({ gap = S(8), align = "center", theme.glyph(a.icon, S(18), c.dim), theme.label(a.label, { size = S(16), color = c.dim, hover_color = c.text }) }),
        }))
    end
    return ui.box({
        block = true,
        on_press = function(x, y)
            local px, py, pw, ph = ui.bounds("start", "panel")
            if px == nil or x < px or y < py or x > px + pw or y > py + ph then
                shell.close_start()
            end
        end,
        ui.column({
            id = "panel",
            anchor = "bottom_left",
            margin = { 0, 0, theme.taskbar_height() + insets[3] + S(10), S(12) + insets[4] },
            width = S(560),
            padding = S(24),
            gap = S(18),
            background = "gui/window.png",
            slice = 16,
            ui.row({
                gap = S(14),
                align = "center",
                theme.avatar(config.user.avatar, S(56)),
                ui.column({ gap = S(2), theme.bold(config.user.name, { size = S(20) }), theme.label("Local account", { size = S(15), color = c.dim }) }),
            }),
            theme.divider(),
            theme.label("Apps", { size = S(15), color = c.faint }),
            ui.grid({ columns = 4, gap = S(6), width = "fill", apps }),
            theme.divider(),
            ui.row({ gap = S(4), width = "fill", actions }),
        }),
    })
end, { z = 6000, area = "screen" })

-- Notifications -----------------------------------------------------------------------------------

local toasts = ui.signal({}) -- { id, title, text, icon, on_click, born }
local history = {} -- every notification, newest last (registered with the save in main.luau)
shell.history = history
local toast_n = 0
local TOAST_TIME = 6

-- Shows a notification. `opts`: { title, text, icon = image path, app, on_click, silent }.
function shell.notify(opts)
    toast_n += 1
    local t = {
        id = toast_n,
        title = opts.title or "",
        text = opts.text or "",
        icon = opts.icon or (opts.app and config.apps[opts.app] and config.apps[opts.app].icon) or "apps/start.png",
        on_click = opts.on_click,
        born = kulisa.now(),
        time = theme.time_text(),
    }
    table.insert(history, { title = t.title, text = t.text, icon = t.icon, time = t.time })
    local list = table.clone(toasts:peek())
    table.insert(list, 1, t)
    while #list > 4 do
        table.remove(list)
    end
    toasts:set(list)
    unread:set(unread:peek() + 1)
    if not opts.silent then
        theme.sound("notify", 0.55)
    end
    ui.show("toasts")
end

local function dismiss(id: number)
    local list = {}
    for _, t in toasts:peek() do
        if t.id ~= id then
            table.insert(list, t)
        end
    end
    toasts:set(list)
end

ui.screen("toasts", function()
    local now = kulisa.now()
    local cards = {}
    for _, t in toasts() do
        local k = math.min(1, (now - t.born) / 0.28)
        local slide = (1 - k) ^ 3 * S(80)
        table.insert(cards, ui.button({
            key = tostring(t.id),
            id = `toast{t.id}`,
            x = slide,
            opacity = k,
            width = S(400),
            padding = S(16),
            background = "gui/window.png",
            slice = 16,
            hover_tint = "#e6ebffff",
            cursor = "pointer",
            on_click = function()
                dismiss(t.id)
                unread:set(math.max(0, unread:peek() - 1))
                if t.on_click then
                    t.on_click()
                end
            end,
            ui.row({
                width = "fill",
                gap = S(14),
                ui.image({ image = t.icon, width = S(44), height = S(44) }),
                ui.column({
                    width = "fill",
                    gap = S(4),
                    ui.row({ width = "fill", theme.bold(t.title, { size = S(17), width = "fill" }), theme.label(t.time, { size = S(13), color = c.faint }) }),
                    theme.label(t.text, { size = S(16), color = c.dim, width = "fill" }),
                }),
            }),
        }))
    end
    local insets = theme.insets()
    return ui.box({
        ui.column({
            anchor = "bottom_right",
            margin = { 0, S(16) + insets[2], theme.taskbar_height() + insets[3] + S(16), 0 },
            gap = S(10),
            align = "end",
            cards,
        }),
    })
end, { z = 7000, area = "screen" })

kulisa.on("update", function()
    local now = kulisa.now()
    local list = toasts:peek()
    local animating, expired = false, nil
    for _, t in list do
        if now - t.born < 0.3 then
            animating = true
        elseif now - t.born > TOAST_TIME then
            expired = t.id
        end
    end
    if expired then
        dismiss(expired)
    elseif animating then
        ui.refresh("toasts")
    end
    local text = theme.time_text()
    if text ~= clock_tick:peek() then
        clock_tick:set(text)
    end
end)

-- The notification list (the bell).
function shell.show_history()
    local items = {}
    for i = #history, math.max(1, #history - 12), -1 do
        local h = history[i]
        table.insert(items, ui.row({
            key = tostring(i),
            width = "fill",
            gap = S(12),
            padding = { S(8), 0 },
            ui.image({ image = h.icon, width = S(36), height = S(36) }),
            ui.column({ width = "fill", gap = S(2), theme.bold(h.title, { size = S(16) }), theme.label(h.text, { size = S(15), color = c.dim, width = "fill" }) }),
            theme.label(h.time, { size = S(13), color = c.faint }),
        }))
    end
    if #items == 0 then
        table.insert(items, theme.label("No notifications.", { color = c.dim }))
    end
    shell.dialog({ title = "Notifications", body = ui.scroll({ width = "fill", height = S(380), items }), buttons = { { "Close" } } })
end

-- Captions (narration) and the objective hint -----------------------------------------------------

local caption = ui.signal(nil) -- { who, text }
shell.on_caption_done = function() end

function shell.caption(text: string?, who: string?)
    if text == nil then
        caption:set(nil)
        ui.hide("caption")
        return
    end
    caption:set({ text = text, who = who, n = toast_n })
    ui.show("caption")
end

function shell.caption_shown(): boolean
    return caption:peek() ~= nil
end

-- Click, Space or Enter on a caption: finish the typewriter, then continue.
function shell.advance_caption()
    local t = ui.find("caption", "text")
    if t and not t.complete then
        t:skip()
        return
    end
    caption:set(nil)
    ui.hide("caption")
    shell.on_caption_done()
end

ui.screen("caption", function()
    local cap = caption()
    if cap == nil then
        return ui.box({})
    end
    local insets = theme.insets()
    return ui.box({ ui.button({
        anchor = "bottom",
        margin = { 0, 0, theme.taskbar_height() + insets[3] + S(28), 0 },
        width = math.min(S(1100), select(3, kulisa.screen:visible()) * 0.92),
        padding = { S(18), S(28) },
        background = "gui/round_big.png",
        slice = 18,
        tint = "#0b0d16e8",
        hover_tint = "#11142099",
        on_click = shell.advance_caption,
        ui.column({
            width = "fill",
            gap = S(6),
            cap.who and theme.bold(cap.who, { size = S(18), color = c.accent }),
            ui.text({ id = "text", text = cap.text, size = S(24), font = config.font, color = c.text, cps = 45, width = "fill" }),
            ui.row({ width = "fill", ui.spacer({}), theme.glyph("forward", S(18), c.faint) }),
        }),
    }) })
end, { z = 4500, area = "screen" })

local hint = ui.signal(nil)

function shell.hint(text: string?)
    hint:set(text)
    if text then
        ui.show("hint")
        local layer = ui.layer("hint")
        if layer then
            layer.y, layer.opacity = -S(20), 0
            layer:tween({ y = 0, opacity = 1 }, { time = 0.25, ease = "out_cubic" })
        end
    else
        ui.hide("hint")
    end
end

ui.screen("hint", function()
    local h = hint()
    local insets = theme.insets()
    return ui.box({ ui.row({
        anchor = "top",
        margin = { S(18) + insets[1], 0, 0, 0 },
        padding = { S(10), S(20) },
        gap = S(10),
        align = "center",
        background = "gui/pill.png",
        slice = 15,
        tint = "#0b0d16d8",
        ui.image({ image = "gui/circle.png", width = S(10), height = S(10), tint = c.accent2 }),
        theme.label(h or "", { size = S(18) }),
    }) })
end, { z = 4600, area = "screen" })

-- Dialogs -------------------------------------------------------------------------------------------

local dialog = ui.signal(nil)

-- A modal dialog: { title, text, body = widget, buttons = { { label, fn?, primary? } }, icon }.
-- A button closes the dialog, then runs its function.
function shell.dialog(d)
    dialog:set(d)
    ui.show("dialog")
    local layer = ui.layer("dialog")
    if layer then
        layer.opacity = 0
        layer:tween({ opacity = 1 }, { time = 0.15 })
    end
end

function shell.close_dialog()
    dialog:set(nil)
    ui.hide("dialog")
end

ui.screen("dialog", function()
    local d = dialog()
    if d == nil then
        return ui.box({})
    end
    local buttons = {}
    for i, b in d.buttons or {} do
        table.insert(buttons, theme.button(b[1], function()
            shell.close_dialog()
            if b[2] then
                b[2]()
            end
        end, { primary = b[3], key = tostring(i), id = `dialog{i}`, autofocus = i == 1 }))
    end
    return ui.box({
        background = "#05060cd0",
        ui.column({
            anchor = "center",
            width = math.min(S(d.width or 620), select(3, kulisa.screen:visible()) * 0.92),
            padding = S(28),
            gap = S(18),
            background = "gui/window.png",
            slice = 16,
            ui.row({ gap = S(12), align = "center", d.icon and ui.image({ image = d.icon, width = S(36), height = S(36) }), theme.bold(d.title or "", { size = S(22) }) }),
            d.text and theme.label(d.text, { size = S(19), color = c.dim, width = "fill" }),
            d.body,
            ui.column({ width = "fill", gap = S(10), buttons }),
        }),
    })
end, { z = 8000, modal = true, area = "screen" })

function shell.dialog_shown(): boolean
    return dialog:peek() ~= nil
end

-- Sign-in screen --------------------------------------------------------------------------------------

-- main.luau sets these: { label, fn, primary } for the sign-in buttons.
shell.signin_buttons = {}

ui.screen("signin", function()
    local buttons = {}
    for i, b in shell.signin_buttons do
        table.insert(buttons, theme.button(b[1], b[2], { primary = b[3], width = S(300), key = tostring(i), id = `signin{i}`, autofocus = i == 1 }))
    end
    return ui.box({
        background = "#06070dc0",
        block = true,
        ui.column({
            anchor = "top",
            margin = { S(120), 0, 0, 0 },
            align = "center",
            theme.label(clock_tick(), { size = S(150), color = "#ffffffee" }),
            theme.label(os.date("%A, %d %B"), { size = S(28), color = "#ffffffb0" }),
        }),
        ui.column({
            anchor = "bottom",
            margin = { 0, 0, S(110), 0 },
            align = "center",
            gap = S(14),
            ui.image({ image = config.user.avatar, width = S(132), height = S(132) }),
            theme.bold(config.user.name, { size = S(30) }),
            ui.spacer({ height = S(6), width = 1 }),
            buttons,
        }),
    })
end, { z = 9000, area = "screen", nav = true })

function shell.sign_in(done)
    local layer = ui.layer("signin")
    if not layer then
        return done()
    end
    local tw = layer:tween({ opacity = 0, y = -S(40) }, { time = 0.45, ease = "in_out_cubic" })
    kulisa.spawn(function()
        kulisa.await(tw)
        ui.hide("signin")
        shell.show_desktop()
        done()
    end)
end

-- The desktop and taskbar fade in after signing in.
function shell.show_desktop()
    for _, name in { "desktop", "taskbar" } do
        if not ui.shown(name) then
            ui.show(name)
            local layer = ui.layer(name)
            if layer then
                layer.opacity = 0
                layer:tween({ opacity = 1 }, { time = 0.5, ease = "out_cubic" })
            end
        end
    end
end

function shell.boot()
    fit_wallpaper()
end

kulisa.on("resize", function()
    fit_wallpaper()
    ui.refresh()
end)
kulisa.on("display", fit_wallpaper)

return shell
