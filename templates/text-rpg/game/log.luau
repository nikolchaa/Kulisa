-- The story log: everything that happened, newest last. Entries are plain data so they save:
-- { kind = "say" | "narrate" | "choice" | "item" | "roll" | "note", text, who?, color?, icon? }.

local config = require("game/config")

local ui = kulisa.ui

local log = {}

log.entries = {}
log.revision = ui.signal(0)
-- Bumps for each new entry the typewriter should play.
log.count = 0

function log.add(e)
    table.insert(log.entries, e)
    while #log.entries > config.log_size do
        table.remove(log.entries, 1)
    end
    log.count += 1
    e.n = log.count
    log.revision:set(log.revision:peek() + 1)
    return e
end

function log.last()
    return log.entries[#log.entries]
end

function log.clear()
    table.clear(log.entries)
    log.revision:set(log.revision:peek() + 1)
end

kulisa.state.register("log", {
    save = function()
        local out = {}
        for _, e in log.entries do
            table.insert(out, { kind = e.kind, text = e.text, who = e.who, color = e.color, icon = e.icon })
        end
        return out
    end,
    load = function(d)
        table.clear(log.entries)
        for _, e in d or {} do
            log.count += 1
            e.n = log.count
            e.done = true
            table.insert(log.entries, e)
        end
        log.revision:set(log.revision:peek() + 1)
    end,
})

return log
