-- Browser: an address bar, back / forward / reload, bookmarks and pages from web/pages.luau.
-- Pages are Luau tables of blocks (headings, text, images, links, forum posts, a search box),
-- or functions that build them from story state. Visits are reported to the story.

local bridge = require("os/story")
local config = require("os/config")
local data = require("os/data")
local pages = require("web/pages")
local theme = require("os/theme")
local wm = require("os/wm")

local ui = kulisa.ui
local c = config.colors
local S = theme.size

local browser = {}

local state = { history = {}, index = 0 }
local url = ui.signal("")
local address = ui.signal("")
local query = ui.signal("")
local loading = ui.signal(0) -- a short fake load bar

local function normalize(u: string): string
    u = u:lower():gsub("^%s+", ""):gsub("%s+$", "")
    u = u:gsub("^https?://", ""):gsub("^www%.", ""):gsub("/$", "")
    return u
end

-- Shows `u` (pushing it on the history unless `keep` is set).
function browser.go(u: string, keep: boolean?)
    u = normalize(u)
    if u == "" then
        return
    end
    if not keep then
        while #state.history > state.index do
            table.remove(state.history)
        end
        table.insert(state.history, u)
        state.index = #state.history
    end
    url:set(u)
    address:set(u)
    loading:set(kulisa.now())
    kulisa.after(0.35, function()
        bridge.visited(u)
    end)
end

function browser.init(_win, arg)
    if #state.history == 0 and arg == nil then
        browser.go("searchly.com")
    end
end

function browser.open(_win, arg)
    if arg then
        browser.go(arg)
    end
end

local function back()
    if state.index > 1 then
        state.index -= 1
        browser.go(state.history[state.index], true)
    end
end

local function forward()
    if state.index < #state.history then
        state.index += 1
        browser.go(state.history[state.index], true)
    end
end

-- Blocks --------------------------------------------------------------------------------------------

local function link(text: string, target: string, theme_)
    return ui.button({
        key = target .. text,
        id = `link:{target}`,
        on_click = function()
            browser.go(target)
        end,
        cursor = "pointer",
        theme.label(text, { size = S(theme_.link_size or 19), color = theme_.link, hover_color = theme_.link_hover }),
    })
end

local function block(b, t, width: number)
    local kind = b[1]
    if kind == "h1" then
        return theme.bold(b[2], { size = S(36), color = t.fg, width = "fill" })
    elseif kind == "h2" then
        return theme.bold(b[2], { size = S(25), color = t.fg, width = "fill", margin = { S(10), 0, 0, 0 } })
    elseif kind == "meta" then
        return theme.label(b[2], { size = S(15), color = t.dim, width = "fill" })
    elseif kind == "p" then
        return theme.label(b[2], { size = S(19), color = t.fg, line_height = 1.55, width = "fill" })
    elseif kind == "quote" then
        return ui.row({
            width = "fill",
            gap = S(14),
            ui.box({ width = S(4), height = "fill", background = t.accent }),
            theme.label(`\{i}{b[2]}\{/i}`, { size = S(19), color = t.dim, width = "fill" }),
        })
    elseif kind == "img" then
        local w = math.min(width, S(720))
        return ui.column({
            width = "fill",
            gap = S(6),
            ui.image({ image = b[2], width = w, height = w * (b.aspect or 9 / 16) }),
            b[3] and theme.label(b[3], { size = S(14), color = t.dim, width = "fill" }),
        })
    elseif kind == "link" then
        return ui.column({
            width = "fill",
            gap = S(2),
            link(b[2], b[3], t),
            theme.label(b[3], { size = S(14), color = t.good or "#5fb38a" }),
            b[4] and theme.label(b[4], { size = S(16), color = t.dim, width = "fill" }),
        })
    elseif kind == "links" then
        local items = {}
        for _, l in b[2] do
            table.insert(items, link(l[1], l[2], t))
        end
        return ui.row({ gap = S(18), items })
    elseif kind == "post" then
        return ui.row({
            width = "fill",
            gap = S(14),
            padding = S(14),
            background = "gui/round.png",
            slice = 8,
            tint = t.card,
            theme.avatar(b.avatar or "avatars/unknown.png", S(44)),
            ui.column({
                width = "fill",
                gap = S(4),
                ui.row({ gap = S(10), theme.bold(b.user, { size = S(16), color = t.accent }), theme.label(b.when or "", { size = S(14), color = t.dim }) }),
                theme.label(b.text, { size = S(18), color = t.fg, width = "fill" }),
            }),
        })
    elseif kind == "search" then
        return ui.row({
            width = "fill",
            gap = S(10),
            ui.input({
                id = "search",
                value = query,
                placeholder = "Search the web",
                width = "fill",
                padding = { S(14), S(22) },
                background = "gui/pill.png",
                slice = 15,
                tint = t.card,
                hover_tint = t.card_hover or t.card,
                selected_tint = t.card_hover or t.card,
                size = S(20),
                color = t.fg,
                cursor = "text",
                on_submit = function(q)
                    browser.go("searchly.com/search?q=" .. q)
                end,
            }),
            theme.button("Search", function()
                browser.go("searchly.com/search?q=" .. query:peek())
            end, { primary = true }),
        })
    elseif kind == "space" then
        return ui.box({ height = S(b[2] or 16), width = 1 })
    elseif kind == "rule" then
        return ui.box({ width = "fill", height = 1, background = t.rule or "#00000022" })
    elseif kind == "widget" then
        return b[2]
    end
    return theme.label(`unknown block {kind}`, { color = c.danger })
end

-- Content ----------------------------------------------------------------------------------------

local function toolbar(win)
    local bookmarks = {}
    for _, b in data.bookmarks do
        table.insert(bookmarks, theme.chip(b, function()
            browser.go(b)
        end, { key = b, size = S(14), padding = { S(5), S(12) }, tint = "#ffffff0c" }))
    end
    local secure = url():sub(1, 6) ~= "photo:"
    local load = math.clamp((kulisa.now() - loading()) / 0.35, 0, 1)
    return ui.column({
        width = "fill",
        background = "#00000026",
        ui.row({
            width = "fill",
            padding = { S(8), S(10) },
            gap = S(4),
            align = "center",
            theme.icon_button("back", back, { disabled = state.index <= 1, id = "back" }),
            theme.icon_button("forward", forward, { disabled = state.index >= #state.history }),
            theme.icon_button("reload", function()
                browser.go(url:peek(), true)
            end),
            ui.row({
                width = "fill",
                align = "center",
                gap = S(8),
                padding = { 0, S(14) },
                background = "gui/pill.png",
                slice = 15,
                tint = c.panel_hi,
                secure and theme.glyph("lock", S(15), c.dim),
                ui.input({
                    id = "address",
                    value = address,
                    width = "fill",
                    padding = { S(8), 0 },
                    size = S(17),
                    color = c.text,
                    cursor = "text",
                    on_submit = function(u)
                        browser.go(u)
                    end,
                }),
            }),
            theme.icon_button("home", function()
                browser.go("searchly.com")
            end),
        }),
        #bookmarks > 0 and ui.row({ padding = { 0, S(14), S(8), S(14) }, gap = S(6), bookmarks }),
        -- The load bar.
        ui.box({ width = "fill", height = S(2), ui.box({ width = string.format("%d%%", load * 100), height = "fill", background = if load < 1 then c.accent else "#00000000" }) }),
    })
end

function browser.build(win)
    data.revision()
    local u = url()
    local p = pages.get(u)
    local t = p.theme
    win.title = p.title
    if kulisa.now() - loading() < 0.4 then
        -- Rebuild until the load bar is full.
        kulisa.after(0.03, function()
            wm.refresh(win)
        end)
    end
    local width = math.min(win.w - S(80), S(820))
    local body = {}
    for i, b in p.blocks do
        local w = block(b, t, width)
        if type(w) == "table" then
            w.key = w.key or `b{i}`
        end
        table.insert(body, w)
    end
    return ui.column({
        width = "fill",
        height = "fill",
        toolbar(win),
        ui.scroll({
            id = "page",
            key = u,
            width = "fill",
            height = "fill",
            background = t.bg,
            ui.column({
                width = "fill",
                align = "center",
                padding = { S(34), S(24), S(60), S(24) },
                ui.column({ width = width, gap = S(16), body }),
            }),
        }),
    })
end

function browser.save()
    return { history = state.history, index = state.index }
end

function browser.load(d)
    state.history = (d and d.history) or {}
    state.index = (d and d.index) or 0
    local u = state.history[state.index]
    if u then
        url:set(u)
        address:set(u)
    end
end

return browser
