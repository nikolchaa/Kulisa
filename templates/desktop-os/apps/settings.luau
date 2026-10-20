-- Settings: wallpaper, sound, display, and saves (save and load slots with thumbnails).

local config = require("os/config")
local data = require("os/data")
local shell = require("os/shell")
local theme = require("os/theme")

local ui = kulisa.ui
local c = config.colors
local S = theme.size

local settings = {}
local section = ui.signal("appearance")
local slots_rev = ui.signal(0)

-- Set by main.luau: save and load a slot, and the settings players keep (kulisa.persist).
settings.save_slot = function(_slot: string) end
settings.load_slot = function(_slot: string) end
settings.prefs = { music = 0.6, sound = 0.8, fullscreen = false, wallpaper = config.wallpaper }
settings.on_change = function() end

function settings.open(_win, arg)
    if arg then
        section:set(arg)
    end
end

local previews = {} -- wallpaper name -> node (made once)

local function preview(name: string, w: number, h: number)
    local n = previews[name]
    if n == nil or not n:exists() then
        n = shell.wallpaper_preview(name, w, h)
        previews[name] = n
    end
    return n
end

local function heading(text: string)
    return theme.bold(text, { size = S(17), color = c.faint, margin = { S(8), 0, 0, 0 } })
end

local function appearance()
    local cards = {}
    local w, h = S(180), S(110)
    for _, name in config.wallpaper_order do
        local sel = shell.wallpaper == name
        table.insert(cards, ui.button({
            key = name,
            padding = S(6),
            selected = sel,
            background = "gui/round.png",
            slice = 8,
            tint = "#00000000",
            hover_tint = c.hover,
            selected_tint = c.accent,
            cursor = "pointer",
            on_click = function()
                shell.set_wallpaper(name)
                settings.prefs.wallpaper = name
                settings.on_change()
                ui.refresh()
            end,
            ui.column({
                gap = S(6),
                ui.node({ node = preview(name, w, h) }),
                theme.label(config.wallpapers[name].name, { size = S(15), color = if sel then "#10131f" else c.text }),
            }),
        }))
    end
    return ui.column({ gap = S(12), heading("Wallpaper"), ui.row({ gap = S(12), cards }) })
end

local function slider(label: string, key: string)
    return ui.column({
        gap = S(6),
        width = "fill",
        theme.label(label, { size = S(17) }),
        ui.slider({
            value = settings.prefs[key],
            min = 0,
            max = 1,
            width = S(380),
            height = S(22),
            thumb_size = S(22),
            background = "gui/pill.png",
            slice = 15,
            slice_scale = 0.7,
            tint = c.panel_hi,
            bar = { image = "gui/pill.png", tint = c.accent },
            thumb = { image = "gui/circle.png", tint = "#ffffff" },
            on_change = function(v)
                settings.prefs[key] = v
                settings.on_change()
            end,
        }),
    })
end

local function sound()
    return ui.column({ gap = S(16), width = "fill", heading("Volume"), slider("Music", "music"), slider("Sounds", "sound") })
end

local function display()
    local items = { heading("Display") }
    if kulisa.window.can_quit then
        table.insert(items, ui.row({
            gap = S(10),
            theme.button("Window", function()
                settings.prefs.fullscreen = false
                settings.on_change()
                ui.refresh()
            end, { selected = not settings.prefs.fullscreen }),
            theme.button("Fullscreen", function()
                settings.prefs.fullscreen = true
                settings.on_change()
                ui.refresh()
            end, { selected = settings.prefs.fullscreen }),
        }))
    else
        table.insert(items, theme.label("The display follows your device.", { color = c.dim }))
    end
    return ui.column({ gap = S(12), items })
end

local function saves()
    slots_rev()
    data.revision()
    local by = {}
    for _, s in kulisa.save.list() do
        by[s.slot] = s
    end
    local cards = {}
    for i = 1, 6 do
        local slot = `slot{i}`
        local info = by[slot]
        table.insert(cards, ui.column({
            key = slot,
            width = "fill",
            padding = S(10),
            gap = S(8),
            background = "gui/round.png",
            slice = 8,
            tint = c.panel_hi,
            ui.box({
                width = "fill",
                height = S(110),
                background = "#0a0c12",
                info and info.thumbnail and ui.image({ image = info.thumbnail, width = "fill", height = "fill" }),
                not info and theme.label("Empty", { anchor = "center", color = c.faint }),
            }),
            theme.label(if info then os.date("%d %b  %H:%M", info.timestamp) else " ", { size = S(14), color = c.dim }),
            ui.row({
                gap = S(8),
                theme.button("Save", function()
                    if info then
                        shell.dialog({
                            title = "Overwrite this save?",
                            buttons = { { "Cancel" }, { "Overwrite", function()
                                settings.save_slot(slot)
                                slots_rev:set(slots_rev:peek() + 1)
                            end, true } },
                        })
                    else
                        settings.save_slot(slot)
                        slots_rev:set(slots_rev:peek() + 1)
                    end
                end, { size = S(15), padding = { S(6), S(14) }, id = `save{i}` }),
                theme.button("Load", function()
                    settings.load_slot(slot)
                end, { size = S(15), padding = { S(6), S(14) }, disabled = info == nil, id = `load{i}` }),
            }),
        }))
    end
    return ui.column({ gap = S(12), width = "fill", heading("Saves"), ui.grid({ columns = 3, gap = S(12), width = "fill", cards }) })
end

local SECTIONS = {
    { "appearance", "Appearance", "image", appearance },
    { "sound", "Sound", "volume", sound },
    { "display", "Display", "maximize", display },
    { "saves", "Saves", "save", saves },
}

function settings.build(win)
    local cur = section()
    local nav, content = {}, nil
    for _, s in SECTIONS do
        if s[1] == cur then
            content = s[4]()
        end
        table.insert(nav, ui.button({
            key = s[1],
            id = `section:{s[1]}`,
            width = "fill",
            padding = { S(10), S(12) },
            selected = s[1] == cur,
            background = "gui/round.png",
            slice = 8,
            tint = "#00000000",
            hover_tint = c.hover,
            selected_tint = "#8ea2ff26",
            cursor = "pointer",
            on_click = function()
                section:set(s[1])
            end,
            ui.row({ gap = S(10), align = "center", theme.glyph(s[3], S(18), c.dim), theme.label(s[2], { size = S(17) }) }),
        }))
    end
    return ui.row({
        width = "fill",
        height = "fill",
        ui.column({ width = math.min(S(220), win.w * 0.3), height = "fill", padding = S(10), gap = S(4), background = "#00000026", nav }),
        theme.divider(true),
        ui.scroll({ width = "fill", height = "fill", padding = { S(24), S(30) }, content }),
    })
end

return settings
