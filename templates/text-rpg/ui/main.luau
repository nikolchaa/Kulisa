-- The play screen. Landscape: the illustration and the log on the left, the hero and the pack on
-- the right. Phones (portrait): the illustration on top, the log below, the hero and the pack in
-- sheets that slide up (ui/menus).

local config = require("game/config")
local items = require("game/items")
local log = require("game/log")
local play = require("game/play")
local rules = require("game/rules")
local style = require("ui/style")

local ui = kulisa.ui
local c = config.colors
local S = style.size

local main = {}

main.prefs = { cps = config.prefs.cps }
main.selected = ui.signal(nil) -- item id selected in the pack
main.open_menu = function(_name: string) end -- set by ui/menus

-- Illustration ------------------------------------------------------------------------------------

-- Two sprites cross-fade when the scene changes; the new picture drifts slowly (a gentle zoom).
local frame_w, frame_h = 16, 9
local holder = kulisa.group({})
local below = kulisa.sprite({ parent = holder, width = frame_w, height = frame_h, tint = "#000000" })
local above = kulisa.sprite({ parent = holder, width = frame_w, height = frame_h, opacity = 0 })
local shown_scene = nil

local function size_pictures(w: number, h: number)
    if w == frame_w and h == frame_h then
        return
    end
    frame_w, frame_h = w, h
    below.width, below.height = w, h
    above.width, above.height = w, h
    above.origin_x, above.origin_y, above.x, above.y = w / 2, h / 2, w / 2, h / 2
    holder.clip = { 0, 0, w, h }
end

local function image_for(name: string?): string?
    if name == nil then
        return nil
    end
    local p = `images/{name}.png`
    return if kulisa.exists(p) then p else nil
end

local function show_scene(name: string?)
    if name == shown_scene then
        return
    end
    shown_scene = name
    local img = image_for(name)
    -- What was on top goes below, the new picture fades in above it.
    if above.opacity > 0 and above.image then
        below.image = above.image
        below.width, below.height = frame_w, frame_h
        below.tint = "#ffffff"
    end
    if img then
        above.image = img
        above.width, above.height = frame_w, frame_h
        above.opacity = 0
        above.scale = 1.0
        above:tween({ opacity = 1 }, { time = 0.9, ease = "in_out_sine" })
        above:tween({ scale = 1.06 }, { time = 30, ease = "linear" })
    else
        above:tween({ opacity = 0 }, 0.6)
    end
end

kulisa.on("update", function()
    show_scene(play.scene:peek())
end)

local function illustration(w: number, h: number)
    size_pictures(w, h)
    return ui.box({
        width = w,
        height = h,
        ui.node({ node = holder, width = w, height = h }),
        ui.box({ width = "fill", height = "fill", background = "gui/frame.png", slice = 20, margin = -S(6) }),
    })
end

-- Log -------------------------------------------------------------------------------------------------

local function entry(e, newest: boolean, old: boolean)
    local alpha = if old then 0.55 else 1
    local typed = newest and not e.done and (e.kind == "say" or e.kind == "narrate")
    local cps = if typed then main.prefs.cps else 0
    local id = if newest then "newest" else nil
    if e.kind == "say" then
        local name = `\{b}\{color={e.color or c.gold}}{e.who}\{/color}\{/b}`
        return ui.text({ key = tostring(e.n), id = id, text = `{name}   “{e.text}”`, size = S(27), font = config.font,
            color = c.paper, line_height = 1.5, cps = cps, width = "fill", opacity = alpha })
    elseif e.kind == "choice" then
        return style.label(`› {e.text}`, { key = tostring(e.n), size = S(23), color = c.gold, width = "fill", opacity = alpha * 0.9 })
    elseif e.kind == "item" or e.kind == "roll" then
        local col = if e.color == "good" then c.good elseif e.color == "bad" then c.bad else c.gold_hi
        return ui.row({
            key = tostring(e.n),
            gap = S(12),
            align = "center",
            opacity = alpha,
            ui.image({ image = e.icon, width = S(38), height = S(38) }),
            style.label(e.text, { size = S(22), color = col, width = "fill" }),
        })
    elseif e.kind == "note" then
        local col = if e.color == "bad" then c.bad else c.dim
        return style.label(e.text, { key = tostring(e.n), size = S(22), color = col, width = "fill", opacity = alpha })
    end
    return ui.text({ key = tostring(e.n), id = id, text = e.text, size = S(27), font = config.font, color = c.text,
        line_height = 1.5, cps = cps, width = "fill", opacity = alpha })
end

local function choices()
    local list = play.choices()
    if not list then
        return nil
    end
    local buttons = {}
    for i, it in list do
        table.insert(buttons, style.button(`{i}.  {it.text}`, function()
            play.choose(it)
        end, { key = `c{it.index}`, id = `choice{i}`, width = "fill", align = "left", autofocus = i == 1,
            padding = { S(12), S(22) } }))
    end
    return ui.column({ key = "choices", width = "fill", gap = S(10), padding = { S(12), 0, 0, 0 }, buttons })
end

local function log_panel()
    log.revision()
    local entries = {}
    local n = #log.entries
    for i, e in log.entries do
        table.insert(entries, entry(e, i == n, i < n - 4))
    end
    local ch = choices()
    if ch then
        table.insert(entries, ch)
    elseif play.ended() then
        table.insert(entries, ui.column({
            key = "end",
            width = "fill",
            align = "center",
            gap = S(14),
            padding = { S(20), 0 },
            style.divider(),
            style.heading("The end", { size = S(26) }),
            ui.row({ gap = S(14), style.button("Play again", play.start, { id = "again" }), style.button("Title", function()
                main.open_menu("title")
            end) }),
        }))
    end
    return ui.button({
        id = "log",
        width = "fill",
        height = "fill",
        background = "gui/panel.png",
        slice = 24,
        padding = { S(26), S(30) },
        on_click = play.advance,
        ui.scroll({ id = "scroll", width = "fill", height = "fill", start = "end", gap = S(18), entries }),
    })
end

-- Hero and pack -------------------------------------------------------------------------------------

function main.hero()
    local v = kulisa.story.vars()
    local stats = {}
    for _, a in config.attributes do
        table.insert(stats, style.stat(a.label, rules.stat(a.key)))
    end
    return ui.column({
        width = "fill",
        gap = S(16),
        ui.row({
            gap = S(18),
            align = "center",
            ui.box({
                padding = S(4),
                background = "gui/frame.png",
                slice = 20,
                slice_scale = 0.5,
                ui.image({ image = config.hero.portrait, width = S(112), height = S(112) }),
            }),
            ui.column({
                gap = S(4),
                style.label(`\{b}{config.hero.name}\{/b}`, { size = S(32), color = c.paper }),
                style.label(config.hero.title, { size = S(19), color = c.dim }),
                style.label(`\{color={c.gold_hi}}●\{/color}  {v.gold or 0} gold`, { size = S(20), color = c.text }),
            }),
        }),
        style.bar("Health", v.hp or 0, v.max_hp or 0, c.blood),
        ui.row({ width = "fill", gap = S(10), stats }),
    })
end

function main.pack()
    local bag = kulisa.story.vars().bag or {}
    local ids = {}
    for id, n in bag do
        if n > 0 and items[id] then
            table.insert(ids, id)
        end
    end
    table.sort(ids)
    local slots = {}
    local sel = main.selected()
    for i = 1, math.max(8, #ids) do
        local id = ids[i]
        local def = id and items[id]
        table.insert(slots, ui.button({
            key = `slot{i}`,
            id = if id then `item:{id}` else nil,
            width = "fill",
            height = S(84),
            background = "gui/slot.png",
            slice = 12,
            selected = id ~= nil and id == sel,
            disabled = id == nil,
            tint = "#ffffff",
            hover_tint = "#fff0d0",
            selected_tint = "#ffd98a",
            cursor = if id then "pointer" else nil,
            on_click = function()
                main.selected:set(if sel == id then nil else id)
            end,
            def and ui.image({ anchor = "center", image = def.icon, width = S(62), height = S(62) }),
            def and bag[id] > 1 and style.label(`\{b}{bag[id]}\{/b}`, { anchor = "bottom_right", margin = S(6), size = S(16), color = c.paper }),
        }))
    end
    local detail = nil
    if sel and items[sel] and (bag[sel] or 0) > 0 then
        local def = items[sel]
        detail = ui.column({
            width = "fill",
            gap = S(8),
            style.label(`\{b}{def.name}\{/b}`, { size = S(22), color = c.gold_hi }),
            style.label(def.text, { size = S(19), color = c.text, width = "fill" }),
            def.use and style.button("Use", function()
                rules.use(sel)
                if not rules.has(sel) then
                    main.selected:set(nil)
                end
                ui.refresh()
            end, { id = "use", size = S(19), padding = { S(6), S(18) } }),
        })
    end
    return ui.column({
        width = "fill",
        gap = S(12),
        style.heading("Pack"),
        ui.grid({ columns = 4, gap = S(10), width = "fill", slots }),
        detail,
    })
end

-- Screens --------------------------------------------------------------------------------------------

local function landscape()
    local _, _, sw, sh = kulisa.screen:safe()
    local side = S(500)
    local pad = S(30)
    local left_w = sw - side - pad * 3
    local pic_h = math.min(left_w * 9 / 16, sh * 0.5)
    local pic_w = pic_h * 16 / 9
    return ui.row({
        width = "fill",
        height = "fill",
        padding = pad,
        gap = pad,
        ui.column({
            width = "fill",
            height = "fill",
            gap = S(26),
            align = "center",
            illustration(pic_w, pic_h),
            log_panel(),
        }),
        ui.column({
            width = side,
            height = "fill",
            gap = pad,
            style.panel({ width = "fill", main.hero() }),
            style.panel({ width = "fill", height = "fill", main.pack() }),
            ui.row({
                width = "fill",
                gap = S(10),
                style.button("Save", function() main.open_menu("save") end, { width = "fill", id = "save" }),
                style.button("Load", function() main.open_menu("load") end, { width = "fill", id = "load" }),
                style.button("Settings", function() main.open_menu("settings") end, { width = "fill", id = "settings" }),
            }),
        }),
    })
end

local function portrait()
    local _, _, sw = kulisa.screen:safe()
    local pad = S(24)
    local w = sw - pad * 2
    local v = kulisa.story.vars()
    return ui.column({
        width = "fill",
        height = "fill",
        padding = pad,
        gap = S(18),
        ui.row({
            width = "fill",
            gap = S(12),
            align = "center",
            ui.box({ width = "fill", style.bar("Health", v.hp or 0, v.max_hp or 0, c.blood, { height = S(14) }) }),
            style.button("Hero", function() main.open_menu("hero") end, { id = "hero_tab", size = S(20) }),
            style.button("Pack", function() main.open_menu("pack") end, { id = "pack_tab", size = S(20) }),
            style.button("Menu", function() main.open_menu("menu") end, { id = "menu_tab", size = S(20) }),
        }),
        illustration(w, w * 9 / 16),
        log_panel(),
    })
end

local function build()
    return ui.box({
        background = "gui/backdrop.png",
        if style.phone then portrait() else landscape(),
    })
end

-- Choices take keyboard and gamepad navigation; otherwise Space and Enter advance the story.
local nav = false
local function define()
    ui.screen("main", build, { z = 10, area = "safe", nav = nav })
end
define()

kulisa.on("update", function()
    local want = play.choices:peek() ~= nil
    if want ~= nav then
        nav = want
        define()
    end
end)

return main
