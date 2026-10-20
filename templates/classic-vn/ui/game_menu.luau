-- The game menu: navigation on the left, the open section on the right. Sections: save,
-- load, preferences, history. Also used from the main menu (load, preferences).

local actions = require("game/actions")
local config = require("game/config")
local history = require("game/history")
local prefs = require("game/prefs")
local state = require("game/state")
local style = require("ui/style")

local ui = kulisa.ui
local c = config.colors

-- Save/load page: 1-9, "auto" or "quick".
local page = ui.signal(1)
local SLOTS_PER_PAGE = 6

local function slot_name(p, i: number): string
    if p == "auto" then
        return "auto"
    elseif p == "quick" then
        return "quick"
    end
    return `{p}-{i}`
end

local function slots(mode: string)
    local p = page()
    local saves = {}
    for _, s in kulisa.save.list() do
        saves[s.slot] = s
    end
    local count = if p == "auto" or p == "quick" then 1 else SLOTS_PER_PAGE
    -- Two rows of cards fit between the title and the page buttons: size the thumbnails to
    -- the height that is left (phones use bigger text).
    local _, _, _, vh = kulisa.screen:visible()
    local insets = style.safe_padding(0)
    local room = vh - insets[1] - insets[3] - style.size(56) * 2 - style.size(44 + 24 + 18 * 2 + 40)
    local thumb_h = math.max(60, math.min(style.size(198), room / 2 - style.size(18 + 10 * 2 + 8 * 2 + 24 + 50)))
    local cards = {}
    for i = 1, count do
        local name = slot_name(p, i)
        local info = saves[name]
        local on_click = function()
            if mode == "save" then
                if info then
                    actions.confirm("Overwrite this save?", function()
                        actions.save(name)
                    end)
                else
                    actions.save(name)
                end
            elseif info then
                if state.started then
                    actions.confirm("Loading will lose unsaved progress. Continue?", function()
                        actions.load(name)
                    end)
                else
                    actions.load(name)
                end
            end
        end
        local when = if info then os.date("%Y-%m-%d  %H:%M", info.timestamp) else "Empty slot"
        table.insert(cards, style.card(on_click, {
            key = name,
            disabled = mode == "load" and info == nil or (mode == "save" and (p == "auto" or p == "quick")),
            padding = style.size(10),
        }, {
            ui.column({
                gap = style.size(8),
                width = "fill",
                ui.box({
                    width = "fill",
                    height = thumb_h,
                    background = "#0c0c10",
                    info and info.thumbnail and ui.image({ image = info.thumbnail, width = "fill", height = "fill" }),
                }),
                style.label(when, { size = style.size(22), color = c.idle }),
                style.label(if info then info.title else " ", { size = style.size(20), color = c.haze, width = "fill" }),
            }),
        }))
    end
    local pages = {}
    for _, p2 in { "auto", "quick", 1, 2, 3, 4, 5, 6, 7, 8, 9 } do
        local label = if p2 == "auto" then "A" elseif p2 == "quick" then "Q" else tostring(p2)
        table.insert(pages, style.button(label, function()
            page:set(p2)
        end, { selected = p == p2, key = label, size = style.size(26) }))
    end
    return ui.column({
        width = "fill",
        height = "fill",
        gap = style.size(18),
        ui.grid({ columns = 3, gap = style.size(18), width = "fill", cards }),
        ui.spacer({ width = 1 }),
        ui.row({ gap = style.size(14), align = "center", pages }),
    })
end

local function toggle(label: string, key: string)
    return style.option(label, prefs.get(key), function()
        prefs.set(key, not prefs.get(key))
    end)
end

local function choice(label: string, key: string, value)
    return style.option(label, prefs.get(key) == value, function()
        prefs.set(key, value)
    end)
end

local function preferences()
    prefs.changed()
    local left = {}
    if kulisa.platform ~= "android" and kulisa.platform ~= "ios" then
        table.insert(left, style.heading("Display"))
        table.insert(left, choice("Window", "fullscreen", false))
        table.insert(left, choice("Fullscreen", "fullscreen", true))
    end
    if kulisa.screen.touch then
        table.insert(left, style.heading("Rollback side"))
        table.insert(left, choice("Disable", "rollback_side", "disable"))
        table.insert(left, choice("Left", "rollback_side", "left"))
        table.insert(left, choice("Right", "rollback_side", "right"))
    end
    table.insert(left, style.heading("Skip"))
    table.insert(left, toggle("Unseen text", "skip_unseen"))
    table.insert(left, toggle("After choices", "skip_after_choices"))
    table.insert(left, toggle("Transitions", "transitions"))

    local cps = prefs.get("cps")
    local speed = if cps == 0 then 150 else cps
    local volume = function(label, key)
        return ui.column({
            gap = style.size(6),
            style.heading(label),
            style.slider(prefs.get(key), 0, 1, function(v)
                prefs.set(key, v)
            end),
        })
    end
    return ui.row({
        gap = style.size(80),
        ui.column({ gap = style.size(8), left }),
        ui.column({
            gap = style.size(22),
            ui.column({
                gap = style.size(6),
                style.heading("Text speed"),
                style.slider(speed, 10, 150, function(v)
                    prefs.set("cps", if v >= 150 then 0 else math.floor(v))
                end, { step = 5 }),
            }),
            ui.column({
                gap = style.size(6),
                style.heading("Auto-forward time"),
                -- Faster to the right: the slider shows 4 - delay.
                style.slider(4 - prefs.get("auto_delay"), 0, 3.5, function(v)
                    prefs.set("auto_delay", 4 - v)
                end, { step = 0.1 }),
            }),
            volume("Music volume", "music"),
            volume("Sound volume", "sound"),
            volume("Voice volume", "voice"),
            toggle("Mute all", "mute"),
        }),
    })
end

local function backlog()
    local items = {}
    for i, e in history.entries() do
        table.insert(items, ui.column({
            key = tostring(i),
            width = "fill",
            gap = style.size(4),
            padding = { style.size(10), 0 },
            e.name and style.label(`\{b}{e.name}\{/b}`, { size = style.size(26), color = e.color or c.paper }),
            style.label(e.text, { size = style.size(28), color = c.paper, width = "fill" }),
        }))
    end
    if #items == 0 then
        table.insert(items, style.label("The history is empty.", { color = c.haze }))
    end
    return ui.scroll({ width = "fill", height = "fill", start = "end", items })
end

local TITLES = { save = "Save", load = "Load", prefs = "Preferences", history = "History" }

ui.screen("game_menu", function()
    local section = state.menu() or "save"
    local nav = {}
    local function item(label: string, f, selected: boolean?)
        table.insert(nav, style.button(label, f, { selected = selected, autofocus = selected, size = style.size(32) }))
    end
    if state.started then
        item("History", function() actions.open_menu("history") end, section == "history")
        item("Save", function() actions.open_menu("save") end, section == "save")
    else
        item("Start", function()
            actions.close_menu()
            actions.start()
        end)
    end
    item("Load", function() actions.open_menu("load") end, section == "load")
    item("Preferences", function() actions.open_menu("prefs") end, section == "prefs")
    if state.started then
        item("Main Menu", function()
            actions.confirm("Return to the main menu? Unsaved progress is lost.", actions.main_menu)
        end)
    end
    if kulisa.window.can_quit then
        item("Quit", actions.quit)
    end
    local content
    if section == "save" or section == "load" then
        content = slots(section)
    elseif section == "prefs" then
        content = preferences()
    else
        content = backlog()
    end
    return ui.box({
        background = c.ink,
        ui.row({
            width = "fill",
            height = "fill",
            padding = style.safe_padding(style.size(56)),
            gap = style.size(56),
            ui.column({
                width = style.size(300),
                height = "fill",
                gap = style.size(12),
                style.label(`\{b}{config.title}\{/b}`, { size = style.size(40), color = c.accent }),
                ui.spacer({ height = style.size(30), width = 1 }),
                nav,
                ui.spacer({ width = 1 }),
                style.button("Return", actions.close_menu, { size = style.size(32) }),
            }),
            ui.column({
                width = "fill",
                height = "fill",
                gap = style.size(24),
                style.label(`\{b}{TITLES[section]}\{/b}`, { size = style.size(44) }),
                content,
            }),
        }),
    })
end, { z = 200, modal = true, area = "screen" })

return {}
