-- Messages: contacts on the left, the conversation on the right. Contacts and messages come from
-- the story (os/story.luau); replies appear as chips when the story asks a question.

local bridge = require("os/story")
local config = require("os/config")
local data = require("os/data")
local theme = require("os/theme")
local wm = require("os/wm")

local ui = kulisa.ui
local c = config.colors
local S = theme.size

local chat = {}

local selected = ui.signal(nil) -- thread id shown

local function current(): string?
    return selected:peek() or data.thread_order[1]
end

bridge.viewing = function(id: string): boolean
    return wm.is_focused("chat") and current() == id
end

bridge.open_thread = function(id: string)
    wm.open("chat", id)
end

local function mark_read(id: string?)
    local t = id and data.threads[id]
    if t and t.unread > 0 then
        t.unread = 0
        data.changed()
    end
end

function chat.open(_win, thread: string?)
    if thread then
        selected:set(thread)
    end
    mark_read(current())
end

local function contact(t, is_selected: boolean)
    local last = t.messages[#t.messages]
    local preview = if bridge.typing() == t.id then "typing…"
        elseif last == nil then ""
        elseif last.image then "Photo"
        else (if last.from == "me" then "You: " else "") .. last.text
    if #preview > 38 then
        preview = preview:sub(1, utf8.offset(preview, 36) or 36) .. "…"
    end
    return ui.button({
        key = t.id,
        id = `thread:{t.id}`,
        width = "fill",
        padding = { S(10), S(12) },
        selected = is_selected,
        background = "gui/round.png",
        slice = 8,
        tint = "#00000000",
        hover_tint = c.hover,
        selected_tint = "#8ea2ff26",
        cursor = "pointer",
        on_click = function()
            selected:set(t.id)
            mark_read(t.id)
        end,
        ui.row({
            width = "fill",
            gap = S(12),
            align = "center",
            theme.avatar(t.avatar, S(46)),
            ui.column({
                width = "fill",
                gap = S(3),
                ui.row({
                    width = "fill",
                    theme.bold(t.name, { size = S(17), width = "fill" }),
                    last and theme.label(last.time or "", { size = S(13), color = c.faint }),
                }),
                theme.label(preview, { size = S(15), color = if t.unread > 0 then c.text else c.dim, width = "fill" }),
            }),
            t.unread > 0 and theme.badge(t.unread),
        }),
    })
end

local function bubble(t, m, i: number, width: number)
    local mine = m.from == "me"
    local content
    if m.image then
        content = ui.button({
            padding = S(6),
            background = if mine then "gui/bubble_out.png" else "gui/bubble_in.png",
            slice = 20,
            tint = if mine then c.bubble_out else c.bubble_in,
            cursor = "pointer",
            on_click = function()
                wm.open("browser", "photo:" .. m.image)
            end,
            ui.image({ image = m.image, width = math.min(S(360), width * 0.7), height = math.min(S(360), width * 0.7) * 9 / 16 }),
        })
    else
        content = ui.box({
            max_width = width * 0.72,
            padding = { S(10), S(16) },
            background = if mine then "gui/bubble_out.png" else "gui/bubble_in.png",
            slice = 20,
            tint = if mine then c.bubble_out else c.bubble_in,
            theme.label(m.text, { size = S(19), color = if mine then "#ffffff" else c.text }),
        })
    end
    local next_m = t.messages[i + 1]
    local tail = next_m == nil or next_m.from ~= m.from
    return ui.row({
        key = tostring(i),
        width = "fill",
        gap = S(10),
        align = "end",
        padding = { 0, 0, if tail then S(10) else S(2), 0 },
        mine and ui.spacer({}),
        not mine and (if tail then theme.avatar(t.avatar, S(32)) else ui.box({ width = S(32), height = 1 })),
        ui.column({
            gap = S(4),
            align = if mine then "end" else "start",
            content,
            tail and theme.label(m.time or "", { size = S(12), color = c.faint }),
        }),
    })
end

local function conversation(t, width: number)
    if t == nil then
        return ui.box({
            width = "fill",
            height = "fill",
            theme.label("No conversations yet.", { anchor = "center", color = c.faint }),
        })
    end
    local items = {}
    for i, m in t.messages do
        table.insert(items, bubble(t, m, i, width))
    end
    if bridge.typing() == t.id then
        table.insert(items, ui.row({
            key = "typing",
            gap = S(10),
            align = "end",
            theme.avatar(t.avatar, S(32)),
            ui.box({
                padding = { S(8), S(18) },
                background = "gui/bubble_in.png",
                slice = 20,
                tint = c.bubble_in,
                theme.label("{wave=0.16}• • •{/wave}", { size = S(20), color = c.dim }),
            }),
        }))
    end
    local replies = bridge.replies()
    local footer
    if replies and replies.thread == t.id then
        local chips = {}
        for k, it in replies.items do
            table.insert(chips, theme.chip(it.text, function()
                bridge.reply(it.index)
            end, { key = tostring(it.index), id = `reply{k}`, autofocus = k == 1 }))
        end
        footer = ui.column({ width = "fill", gap = S(8), padding = { S(12), S(16) }, align = "end", chips })
    else
        footer = ui.row({
            width = "fill",
            padding = { S(12), S(16) },
            gap = S(10),
            align = "center",
            ui.box({
                width = "fill",
                padding = { S(10), S(16) },
                background = "gui/pill.png",
                slice = 15,
                tint = c.panel_hi,
                theme.label("Message", { size = S(17), color = c.faint }),
            }),
            theme.glyph("send", S(22), c.faint),
        })
    end
    local status = if bridge.typing() == t.id then "typing…" else "online"
    return ui.column({
        width = "fill",
        height = "fill",
        ui.row({
            width = "fill",
            padding = { S(12), S(18) },
            gap = S(12),
            align = "center",
            theme.avatar(t.avatar, S(40)),
            ui.column({
                gap = S(2),
                theme.bold(t.name, { size = S(18) }),
                theme.label(status, { size = S(14), color = if status == "online" then c.good else c.accent }),
            }),
        }),
        theme.divider(),
        ui.scroll({ id = "messages", width = "fill", height = "fill", start = "end", padding = { S(14), S(18) }, items }),
        theme.divider(),
        footer,
    })
end

function chat.build(win)
    data.revision()
    local list = {}
    local cur = selected() or data.thread_order[1]
    for _, id in data.thread_order do
        table.insert(list, contact(data.threads[id], id == cur))
    end
    -- Reading a thread clears its unread count.
    if wm.focused() == win.id and cur and data.threads[cur] and data.threads[cur].unread > 0 then
        kulisa.after(0, function()
            mark_read(cur)
        end)
    end
    local side = math.min(S(320), win.w * 0.36)
    return ui.row({
        width = "fill",
        height = "fill",
        ui.column({
            width = side,
            height = "fill",
            padding = S(10),
            gap = S(4),
            background = "#00000026",
            theme.bold("Messages", { size = S(20), margin = { S(6), S(8), S(10), S(8) } }),
            ui.scroll({ width = "fill", height = "fill", list }),
        }),
        theme.divider(true),
        conversation(cur and data.threads[cur], win.w - side),
    })
end

return chat
