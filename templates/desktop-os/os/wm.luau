-- The window manager. Every window is a UI screen laid out in its own rectangle
-- (kulisa.ui.place), so moving a window is cheap and its content is ordinary widgets. Windows
-- have a title bar (drag to move, double-click to maximize), minimize / maximize / close
-- buttons, edges and corners to resize, focus (a press anywhere raises the window) and z order
-- (kulisa.ui.set_z). Apps (apps/*.luau) only build the content.
--
-- win = { id, app, title, x, y, w, h, maximized, minimized, data }; x, y, w, h is the frame in
-- design units. The screen around it is SHADOW units larger, for the shadow and resize grips.

local config = require("os/config")
local theme = require("os/theme")

local ui = kulisa.ui
local c = config.colors
local S = theme.size

local wm = {}

local SHADOW = 28
local TITLE_H = S(46)
local MIN_W, MIN_H = S(420), S(300)
-- Windows stay below the taskbar (z 5000) and everything above it.
local Z_BASE, Z_TOP = 100, 4000

wm.windows = {} -- id -> win
wm.order = {} -- ids, bottom to top
wm.focused = ui.signal(nil) -- id of the focused window
wm.revision = ui.signal(0) -- bumps when windows open, close or change (taskbar)
wm.apps = {} -- name -> app module (filled by main.luau)

local count = 0
local next_z = Z_BASE

local function changed()
    wm.revision:set(wm.revision:peek() + 1)
end

local function screen_name(win): string
    return `win:{win.id}`
end

local function outer(win)
    return { x = win.x - SHADOW, y = win.y - SHADOW, width = win.w + SHADOW * 2, height = win.h + SHADOW * 2 }
end

-- Keeps the title bar reachable inside the work area.
local function clamp(win)
    local ax, ay, aw, ah = theme.work_area()
    win.w = math.max(MIN_W, math.min(win.w, aw))
    win.h = math.max(MIN_H, math.min(win.h, ah))
    win.x = math.max(ax - win.w + S(120), math.min(win.x, ax + aw - S(120)))
    win.y = math.max(ay, math.min(win.y, ay + ah - TITLE_H))
end

local function place(win)
    if win.maximized then
        local ax, ay, aw, ah = theme.work_area()
        win.x, win.y, win.w, win.h = ax, ay, aw, ah
    end
    ui.place(screen_name(win), outer(win))
end

function wm.find(app: string)
    for _, id in wm.order do
        local w = wm.windows[id]
        if w.app == app then
            return w
        end
    end
    return nil
end

function wm.top()
    for i = #wm.order, 1, -1 do
        local w = wm.windows[wm.order[i]]
        if not w.minimized then
            return w
        end
    end
    return nil
end

function wm.is_focused(app: string): boolean
    local id = wm.focused:peek()
    return id ~= nil and wm.windows[id] ~= nil and wm.windows[id].app == app
end

-- Raises a window to the top and focuses it.
function wm.focus(win)
    if win == nil then
        wm.focused:set(nil)
        return
    end
    local i = table.find(wm.order, win.id)
    if i then
        table.remove(wm.order, i)
    end
    table.insert(wm.order, win.id)
    next_z += 1
    if next_z > Z_TOP then
        -- Renumber from the bottom.
        next_z = Z_BASE
        for _, id in wm.order do
            next_z += 1
            ui.set_z(`win:{id}`, next_z)
        end
    else
        ui.set_z(screen_name(win), next_z)
    end
    if wm.focused:peek() ~= win.id then
        wm.focused:set(win.id)
    end
    changed()
end

local function animate_in(win)
    local layer = ui.layer(screen_name(win))
    if not layer then
        return
    end
    local cx, cy = win.x + win.w / 2, win.y + win.h / 2
    layer.origin_x, layer.origin_y, layer.x, layer.y = cx, cy, cx, cy
    layer.scale, layer.opacity = 0.94, 0
    layer:tween({ scale = 1, opacity = 1 }, { time = 0.18, ease = "out_cubic" })
end

-- Animates the window towards (tx, ty) and away, then calls `done`.
local function animate_out(win, tx, ty, done)
    local layer = ui.layer(screen_name(win))
    if not layer then
        return done()
    end
    local cx, cy = win.x + win.w / 2, win.y + win.h / 2
    layer.origin_x, layer.origin_y, layer.x, layer.y = cx, cy, cx, cy
    local tw = layer:tween({ scale = 0.6, opacity = 0, x = tx or cx, y = ty or cy }, { time = 0.16, ease = "in_cubic" })
    kulisa.spawn(function()
        kulisa.await(tw)
        layer.scale, layer.opacity, layer.x, layer.y = 1, 1, cx, cy
        done()
    end)
end

-- Frame -------------------------------------------------------------------------------------

local function title_button(name: string, on_click, danger: boolean?)
    return ui.button({
        on_click = on_click,
        width = S(46),
        height = TITLE_H - S(10),
        background = "gui/round.png",
        slice = 8,
        tint = "#00000000",
        hover_tint = if danger then c.danger else c.hover,
        pressed_tint = if danger then "#d94452" else c.pressed,
        ui.box({ anchor = "center", theme.glyph(name, S(17), c.dim) }),
    })
end

local last_title_click = { id = nil, at = -1 }

local function title_bar(win, focused: boolean)
    local app = config.apps[win.app]
    return ui.row({
        id = "titlebar",
        width = "fill",
        height = TITLE_H,
        padding = { 0, S(6), 0, S(14) },
        align = "center",
        gap = S(10),
        cursor = "default",
        on_click = function()
            -- Double-click toggles maximize.
            local now = kulisa.now()
            if last_title_click.id == win.id and now - last_title_click.at < 0.4 then
                wm.toggle_maximize(win)
                last_title_click.at = -1
            else
                last_title_click = { id = win.id, at = now }
            end
        end,
        on_drag = function(e)
            if e.phase == "start" then
                if win.maximized then
                    -- Dragging a maximized window restores it under the pointer.
                    local r = win.restore or { w = config.apps[win.app].size[1], h = config.apps[win.app].size[2] }
                    local fx = (e.start_x - win.x) / win.w
                    win.maximized = false
                    win.w, win.h = r.w, r.h
                    win.x = e.start_x - win.w * fx
                    win.y = e.start_y - TITLE_H / 2
                    ui.refresh(screen_name(win))
                end
                win.drag = { x = win.x, y = win.y }
            elseif win.drag then
                win.x, win.y = win.drag.x + e.dx, win.drag.y + e.dy
                clamp(win)
                place(win)
                if e.phase == "end" then
                    win.drag = nil
                end
            end
        end,
        ui.image({ image = app.icon, width = S(22), height = S(22) }),
        theme.label(win.title or app.title, { size = S(17), color = if focused then c.text else c.faint, width = "fill" }),
        title_button("minimize", function()
            wm.minimize(win)
        end),
        title_button(if win.maximized then "restore" else "maximize", function()
            wm.toggle_maximize(win)
        end),
        title_button("close", function()
            wm.close(win)
        end, true),
    })
end

-- A resize grip on an edge or corner: `sx`, `sy` say which edges it moves (-1 left/top,
-- 1 right/bottom, 0 none).
local function grip(win, sx: number, sy: number)
    local cursor = if sx ~= 0 and sy ~= 0 then (if sx == sy then "nwse_resize" else "nesw_resize")
        elseif sx ~= 0 then "ew_resize" else "ns_resize"
    local t = 12 -- grip thickness
    local inset = SHADOW - t / 2
    local corner = 22
    local anchor = ({ [-1] = "left", [0] = "", [1] = "right" })[sx]
    local vert = ({ [-1] = "top", [0] = "", [1] = "bottom" })[sy]
    local name = if vert ~= "" and anchor ~= "" then `{vert}_{anchor}` elseif vert ~= "" then vert else anchor
    return ui.box({
        anchor = name,
        width = if sx == 0 then "fill" elseif sy ~= 0 then corner else t,
        height = if sy == 0 then "fill" elseif sx ~= 0 then corner else t,
        margin = {
            if sy == -1 then inset elseif sy == 0 then SHADOW + corner else 0,
            if sx == 1 then inset elseif sx == 0 then SHADOW + corner else 0,
            if sy == 1 then inset elseif sy == 0 then SHADOW + corner else 0,
            if sx == -1 then inset elseif sx == 0 then SHADOW + corner else 0,
        },
        cursor = cursor,
        on_drag = function(e)
            if e.phase == "start" then
                win.drag = { x = win.x, y = win.y, w = win.w, h = win.h }
                return
            end
            local d = win.drag
            if not d then
                return
            end
            if sx == 1 then
                win.w = math.max(MIN_W, d.w + e.dx)
            elseif sx == -1 then
                win.w = math.max(MIN_W, d.w - e.dx)
                win.x = d.x + d.w - win.w
            end
            if sy == 1 then
                win.h = math.max(MIN_H, d.h + e.dy)
            elseif sy == -1 then
                win.h = math.max(MIN_H, d.h - e.dy)
                win.y = d.y + d.h - win.h
            end
            place(win)
            if e.phase == "end" then
                win.drag = nil
                clamp(win)
                place(win)
            end
        end,
    })
end

local function frame(win)
    local focused = wm.focused() == win.id
    local app = wm.apps[win.app]
    local content = app.build(win)
    local grips = {}
    if not win.maximized and not theme.phone then
        for _, g in { { -1, 0 }, { 1, 0 }, { 0, -1 }, { 0, 1 }, { -1, -1 }, { 1, -1 }, { -1, 1 }, { 1, 1 } } do
            table.insert(grips, grip(win, g[1], g[2]))
        end
    end
    return ui.box({
        width = "fill",
        height = "fill",
        -- The shadow is darker under the focused window.
        background = "gui/shadow.png",
        slice = 40,
        slice_scale = SHADOW / 40,
        opacity = 1,
        tint = if focused then "#ffffffff" else "#ffffff99",
        ui.column({
            width = "fill",
            height = "fill",
            margin = SHADOW,
            background = "gui/window.png",
            slice = 16,
            block = true,
            clip = true,
            on_press = function()
                if wm.focused:peek() ~= win.id or wm.order[#wm.order] ~= win.id then
                    wm.focus(win)
                end
            end,
            title_bar(win, focused),
            ui.box({ width = "fill", height = "fill", padding = { 0, 1, 1, 1 }, content }),
        }),
        grips,
    })
end

-- Opening, closing, minimizing -----------------------------------------------------------------

-- Opens an app (or focuses its window, for single-window apps) and passes `arg` to it.
function wm.open(app_name: string, arg)
    local def = config.apps[app_name]
    assert(def, `unknown app {app_name}`)
    local app = wm.apps[app_name]
    local existing = def.single and wm.find(app_name)
    if existing then
        if existing.minimized then
            wm.restore(existing)
        end
        wm.focus(existing)
        if app.open then
            app.open(existing, arg)
        end
        return existing
    end
    count += 1
    local ax, ay, aw, ah = theme.work_area()
    local w, h = S(def.size[1]), S(def.size[2])
    -- Cascade from the center.
    local k = #wm.order % 6
    local win = {
        id = `{app_name}{count}`,
        app = app_name,
        w = w,
        h = h,
        x = ax + (aw - w) / 2 - S(80) + k * S(36),
        y = ay + (ah - h) / 2 - S(40) + k * S(30),
        maximized = theme.phone,
        data = {},
    }
    clamp(win)
    wm.windows[win.id] = win
    if app.init then
        app.init(win, arg)
    end
    local name = screen_name(win)
    ui.screen(name, function()
        return frame(win)
    end, { area = outer(win), z = next_z + 1 })
    place(win)
    ui.show(name)
    wm.focus(win)
    animate_in(win)
    if app.open then
        app.open(win, arg)
    end
    return win
end

local function forget(win)
    ui.hide(screen_name(win))
    wm.windows[win.id] = nil
    local i = table.find(wm.order, win.id)
    if i then
        table.remove(wm.order, i)
    end
    local app = wm.apps[win.app]
    if app.close then
        app.close(win)
    end
    if wm.focused:peek() == win.id then
        wm.focus(wm.top())
    end
    changed()
end

function wm.close(win, instant: boolean?)
    if instant or win.minimized then
        return forget(win)
    end
    animate_out(win, nil, nil, function()
        forget(win)
    end)
end

-- Where minimized windows fly to (the taskbar sets this).
wm.taskbar_spot = function(_app: string): (number?, number?)
    return nil, nil
end

function wm.minimize(win)
    local tx, ty = wm.taskbar_spot(win.app)
    animate_out(win, tx, ty, function()
        win.minimized = true
        ui.hide(screen_name(win))
        if wm.focused:peek() == win.id then
            wm.focus(wm.top())
        end
        changed()
    end)
end

function wm.restore(win)
    if not win.minimized then
        return
    end
    win.minimized = false
    ui.show(screen_name(win))
    place(win)
    wm.focus(win)
    animate_in(win)
end

function wm.toggle_maximize(win)
    if theme.phone then
        return
    end
    if win.maximized then
        win.maximized = false
        local r = win.restore
        if r then
            win.x, win.y, win.w, win.h = r.x, r.y, r.w, r.h
        end
    else
        win.restore = { x = win.x, y = win.y, w = win.w, h = win.h }
        win.maximized = true
    end
    place(win)
    ui.refresh(screen_name(win))
end

-- Rebuilds a window's content (apps call it when their data changes).
function wm.refresh(win)
    if win and not win.minimized then
        ui.refresh(screen_name(win))
    end
end

function wm.refresh_app(app: string)
    for _, w in wm.windows do
        if w.app == app then
            wm.refresh(w)
        end
    end
end

function wm.close_all()
    for _, w in table.clone(wm.windows) do
        forget(w)
    end
end

-- The open windows, for saves: { app, x, y, w, h, maximized, minimized, focused }, bottom first.
function wm.snapshot()
    local out = {}
    for _, id in wm.order do
        local w = wm.windows[id]
        table.insert(out, {
            app = w.app, x = w.x, y = w.y, w = w.w, h = w.h,
            maximized = w.maximized, minimized = w.minimized or false,
        })
    end
    return out
end

function wm.restore_snapshot(list)
    wm.close_all()
    for _, s in list or {} do
        local win = wm.open(s.app)
        win.x, win.y, win.w, win.h, win.maximized = s.x, s.y, s.w, s.h, s.maximized and true or false
        place(win)
        if s.minimized then
            win.minimized = true
            ui.hide(screen_name(win))
        end
    end
    wm.focus(wm.top())
end

-- The work area changes with the window (and the safe area).
local function fit_all()
    for _, w in wm.windows do
        clamp(w)
        place(w)
    end
end
kulisa.on("resize", fit_all)
kulisa.on("display", fit_all)

return wm
