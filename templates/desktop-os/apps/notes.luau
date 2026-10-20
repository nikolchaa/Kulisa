-- Notes: what the story wrote down (desk.note). A list on the left, the note on the right.

local config = require("os/config")
local data = require("os/data")
local theme = require("os/theme")

local ui = kulisa.ui
local c = config.colors
local S = theme.size

local notes = {}
local selected = ui.signal(1)

function notes.build(win)
    data.revision()
    local sel = math.min(selected(), math.max(1, #data.notes))
    local list = {}
    for i, n in data.notes do
        table.insert(list, ui.button({
            key = n.title .. i,
            width = "fill",
            padding = { S(10), S(12) },
            selected = i == sel,
            background = "gui/round.png",
            slice = 8,
            tint = "#00000000",
            hover_tint = c.hover,
            selected_tint = "#ffd66b26",
            cursor = "pointer",
            on_click = function()
                selected:set(i)
            end,
            ui.column({
                width = "fill",
                gap = S(3),
                theme.bold(n.title, { size = S(17), width = "fill" }),
                theme.label(n.time or "", { size = S(13), color = c.faint }),
            }),
        }))
    end
    local note = data.notes[sel]
    local side = math.min(S(260), win.w * 0.36)
    return ui.row({
        width = "fill",
        height = "fill",
        ui.column({
            width = side,
            height = "fill",
            padding = S(10),
            background = "#00000026",
            #list == 0 and theme.label("No notes yet.", { color = c.faint, margin = S(10) }),
            ui.scroll({ width = "fill", height = "fill", list }),
        }),
        theme.divider(true),
        ui.scroll({
            width = "fill",
            height = "fill",
            padding = { S(28), S(34) },
            note and ui.column({
                width = "fill",
                gap = S(12),
                theme.bold(note.title, { size = S(28), color = "#ffd66b" }),
                theme.label(note.time or "", { size = S(14), color = c.faint }),
                theme.label(note.text, { size = S(20), line_height = 1.6, width = "fill" }),
            }),
        }),
    })
end

return notes
