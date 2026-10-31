-- The story glue: lines go to the log, menus become choices, `scene` lines change the
-- illustration, and advancing moves the story on.

local log = require("game/log")

local ui = kulisa.ui

local play = {}

play.started = false
play.ended = ui.signal(false)
play.choices = ui.signal(nil) -- menu items while a menu waits
play.waiting = false -- a line waits for the player
play.scene = ui.signal(nil) -- the illustration's image name
local restoring = false

kulisa.on("say", function(line)
    local who = line.who
    local e = {
        kind = if who then "say" else "narrate",
        text = line.text,
        who = who and kulisa.t(who.name),
        color = who and who.color,
    }
    local last = log.last()
    if line.replay and last then
        -- A language switch shows the current line again.
        last.text, last.who = e.text, e.who
    elseif restoring and last and last.text == e.text then
        -- After a load the current line comes again; it is in the log already.
        last.done = true
    else
        log.add(e)
    end
    restoring = false
    play.waiting = true
end)

kulisa.on("menu", function(items, caption)
    if caption then
        local last = log.last()
        if caption.replay and last then
            last.text = caption.text
        else
            log.add({ kind = "narrate", text = caption.text })
        end
    end
    restoring = false
    play.waiting = false
    play.choices:set(items)
end)

kulisa.on("stage", function(e)
    if e.kind == "scene" then
        play.scene:set(e.image and table.concat(e.image, " ") or nil)
    end
end)

kulisa.on("pause", function(secs)
    play.waiting = false
    if secs then
        kulisa.after(secs, function()
            kulisa.story.next()
        end)
    else
        play.waiting = true
    end
end)

kulisa.on("end", function()
    play.waiting = false
    play.ended:set(true)
end)

kulisa.on("restore", function(stage)
    restoring = true
    play.choices:set(nil)
    play.ended:set(false)
    play.scene:set(stage.scene and table.concat(stage.scene, " ") or nil)
end)

-- Click, tap, Space or Enter: finish the newest line, then go on.
function play.advance()
    if not play.started or play.choices:peek() then
        return
    end
    local newest = ui.find("main", "newest")
    if newest and not newest.complete then
        newest:skip()
        return
    end
    if play.waiting then
        play.waiting = false
        kulisa.story.next()
    end
end

function play.choose(item)
    play.choices:set(nil)
    log.add({ kind = "choice", text = item.text })
    kulisa.story.choose(item.index)
end

function play.start()
    log.clear()
    play.started = true
    play.ended:set(false)
    play.choices:set(nil)
    kulisa.story.start()
end

return play
