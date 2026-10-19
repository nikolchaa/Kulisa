-- The history (backlog): every line shown, as story state, so it rolls back and saves
-- with the story.

local config = require("game/config")

local history = {}
local entries = {}

history.changed = kulisa.ui.signal(0)

local function bump()
    history.changed:set(history.changed:peek() + 1)
end

function history.add(line)
    local who = line.who
    table.insert(entries, {
        name = who and (who.name or who.id) or nil,
        color = who and who.color or nil,
        text = line.text,
    })
    while #entries > config.history_size do
        table.remove(entries, 1)
    end
    bump()
end

function history.entries()
    history.changed()
    return entries
end

function history.clear()
    entries = {}
    bump()
end

kulisa.state.register("history", {
    save = function()
        return entries
    end,
    load = function(data)
        entries = if type(data) == "table" then data else {}
        bump()
    end,
})

return history
