-- Menus: the title screen, save and load slots, settings, and on phones the hero / pack / menu
-- sheets. One modal screen shows whichever is open.

local config = require("game/config")
local log = require("game/log")
local main = require("ui/main")
local play = require("game/play")
local style = require("ui/style")

local ui = kulisa.ui
local c = config.colors
local S = style.size

local menus = {}
local open = ui.signal(nil) -- "title" | "save" | "load" | "settings" | "hero" | "pack" | "menu" | nil
menus.open = open

-- Set by main.luau.
menus.save = function(_slot: string) end
menus.load = function(_slot: string) end
menus.set_pref = function(_key: string, _value) end
menus.prefs = config.prefs

function menus.show(name: string?)
    -- The play screen hides behind the title.
    if name == "title" then
        ui.hide("main")
    elseif not ui.shown("main") then
        ui.show("main")
    end
    open:set(name)
    if name then
        ui.show("menu")
    else
        ui.hide("menu")
    end
end
main.open_menu = menus.show

local function close_button()
    return style.button("Close", function()
        menus.show(if play.started then nil else "title")
    end, { id = "close" })
end

local function slots(mode: string)
    local by = {}
    for _, s in kulisa.save.list() do
        by[s.slot] = s
    end
    local cards = {}
    for i = 1, 6 do
        local slot = `slot{i}`
        local info = by[slot]
        table.insert(cards, ui.button({
            key = slot,
            id = `slot{i}`,
            width = "fill",
            padding = S(10),
            background = "gui/slot.png",
            slice = 12,
            tint = "#ffffff",
            hover_tint = "#ffe6b8",
            disabled = mode == "load" and info == nil,
            disabled_opacity = 0.45,
            cursor = "pointer",
            on_click = function()
                if mode == "save" then
                    menus.save(slot)
                    ui.refresh("menu")
                else
                    menus.load(slot)
                end
            end,
            ui.column({
                width = "fill",
                gap = S(8),
                ui.box({
                    width = "fill",
                    height = S(if style.phone then 150 else 128),
                    background = "#0a0706",
                    info and info.thumbnail and ui.image({ image = info.thumbnail, width = "fill", height = "fill" }),
                    not info and style.label("Empty", { anchor = "center", color = c.faint, size = S(20) }),
                }),
                style.label(if info then os.date("%d %b  %H:%M", info.timestamp) else " ", { size = S(18), color = c.dim }),
                style.label(if info then info.title else " ", { size = S(17), color = c.text, width = "fill" }),
            }),
        }))
    end
    return ui.column({
        width = "fill",
        gap = S(20),
        style.heading(if mode == "save" then "Save" else "Load", { size = S(26) }),
        ui.grid({ columns = if style.phone then 2 else 3, gap = S(16), width = "fill", cards }),
    })
end

local function slider(label: string, key: string, min: number, max: number, step: number?)
    return ui.column({
        width = "fill",
        gap = S(8),
        style.label(label, { size = S(21), color = c.dim }),
        ui.slider({
            value = menus.prefs[key],
            min = min,
            max = max,
            step = step,
            width = "fill",
            height = S(18),
            thumb_size = S(26),
            background = "gui/bar.png",
            slice = 8,
            tint = "#0a0706",
            bar = { image = "gui/bar.png", tint = c.gold },
            thumb = { image = "gui/bar.png", tint = c.paper },
            on_change = function(v)
                menus.set_pref(key, v)
            end,
        }),
    })
end

local function settings()
    return ui.column({
        width = "fill",
        gap = S(24),
        style.heading("Settings", { size = S(26) }),
        slider("Text speed", "cps", 15, 120, 5),
        slider("Music", "music", 0, 1),
        slider("Sounds", "sound", 0, 1),
    })
end

local function title()
    local buttons = {}
    local function add(label, f, id)
        table.insert(buttons, style.button(label, f, { width = S(360), id = id, size = S(26), autofocus = #buttons == 0 }))
    end
    if kulisa.save.exists("quick") or #kulisa.save.list() > 0 then
        add("Continue", function()
            local best
            for _, s in kulisa.save.list() do
                if best == nil or s.timestamp > best.timestamp then
                    best = s
                end
            end
            menus.load(best.slot)
        end, "continue")
    end
    add("New journey", function()
        menus.show(nil)
        play.start()
    end, "new")
    add("Load", function() menus.show("load") end, "title_load")
    add("Settings", function() menus.show("settings") end, "title_settings")
    if kulisa.window.can_quit then
        add("Quit", function() kulisa.window:quit() end, "quit")
    end
    -- The picture covers the screen (cropped, never stretched).
    local _, _, vw, vh = kulisa.screen:visible()
    local pw = math.max(vw, vh * 16 / 9)
    return ui.box({
        width = "fill",
        height = "fill",
        clip = true,
        background = config.colors.bg,
        ui.image({ anchor = "center", image = "images/crossroads.png", width = pw, height = pw * 9 / 16 }),
        ui.box({ width = "fill", height = "fill", background = "#0e0a0870" }),
        ui.column({
            anchor = "center",
            align = "center",
            gap = S(16),
            style.label(`\{b}{config.title}\{/b}`, { size = S(if style.phone then 76 else 92), color = c.paper }),
            style.label("a short text adventure", { size = S(26), color = c.gold }),
            ui.box({ height = S(30), width = 1 }),
            buttons,
        }),
    })
end

ui.screen("menu", function()
    local which = open()
    if which == "title" then
        return title()
    end
    local content
    if which == "save" or which == "load" then
        content = slots(which)
    elseif which == "settings" then
        content = settings()
    elseif which == "hero" then
        content = main.hero()
    elseif which == "pack" then
        content = main.pack()
    elseif which == "menu" then
        content = ui.column({
            width = "fill",
            gap = S(14),
            style.heading("Menu", { size = S(26) }),
            style.button("Save", function() menus.show("save") end, { width = "fill", id = "m_save" }),
            style.button("Load", function() menus.show("load") end, { width = "fill", id = "m_load" }),
            style.button("Settings", function() menus.show("settings") end, { width = "fill", id = "m_settings" }),
            style.button("Title screen", function()
                play.started = false
                log.clear()
                menus.show("title")
            end, { width = "fill", id = "m_title" }),
        })
    else
        return ui.box({})
    end
    local width = if style.phone then "fill" else S(if which == "save" or which == "load" then 1100 else 700)
    return ui.box({
        background = "#0e0a08c8",
        block = true,
        style.panel({
            anchor = if style.phone then "bottom" else "center",
            width = width,
            margin = S(24),
            padding = S(34),
            ui.column({
                width = "fill",
                gap = S(26),
                content,
                ui.row({ width = "fill", ui.spacer({}), close_button() }),
            }),
        }),
    })
end, { z = 100, modal = true, area = "screen" })

return menus
