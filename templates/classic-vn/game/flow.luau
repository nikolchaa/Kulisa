-- The flow of play: lines, choices, advancing, skipping, auto-forward and rollback, the
-- way Ren'Py players expect them.
--
-- Skip (Tab or the quick menu toggles it; holding Ctrl skips while held) moves through
-- lines quickly, but only those seen before unless "Skip unseen text" is on, and it stops at
-- choices unless "After choices" is on. Auto (A or the quick menu) moves on after each line.
-- Back (wheel up, Page Up, the quick menu, or tapping the rollback side on touch screens)
-- rolls back a line; wheel down or Page Down rolls forward again. Clicking or rolling back
-- stops skipping.

local config = require("game/config")
local history = require("game/history")
local prefs = require("game/prefs")
local state = require("game/state")

local flow = {}
local ui = kulisa.ui

local skip_timer = 0
local auto_timer = 0
-- Shows notifications ("Quick saved."); set by ui/notify.
flow.notify = function(_: string) end

local function what()
    return ui.find("say", "what")
end

local function skipping(): boolean
    return state.mode:peek() == "skip" or (kulisa.input.down("control") and state.menu:peek() == nil)
end

function flow.set_mode(mode: string)
    state.mode:set(mode)
    skip_timer, auto_timer = 0, 0
end

function flow.toggle(mode: string)
    flow.set_mode(if state.mode:peek() == mode then "normal" else mode)
end

-- The story ran into a line.
function flow.say(line)
    state.count += 1
    history.add(line)
    state.line = { who = line.who, text = line.text, seen = line.seen, n = state.count }
    state.choices = nil
    -- Window auto: a text box hidden by `scene` comes back after its transition.
    if state.busy:peek() and not state.window:peek() then
        state.window_after = true
    else
        state.window:set(true)
    end
    state.ctc:set(false)
    ui.hide("choice")
    ui.show("say", { line = state.line, instant = state.instant })
    state.instant = false
    skip_timer, auto_timer = 0, 0
    if state.mode:peek() == "skip" and not (line.seen or prefs.get("skip_unseen")) then
        -- Unseen text stops skipping, as in Ren'Py.
        flow.set_mode("normal")
    end
end

-- The story ran into a menu.
function flow.menu(items, caption)
    if caption then
        flow.say(caption)
    end
    state.choices = { items = items }
    state.instant = false
    if config.autosave and state.started then
        local shot = kulisa.screen:capture(384, 216)
        kulisa.save.write("auto", { thumbnail = shot, title = caption and caption.text or nil })
    end
    if state.mode:peek() == "skip" and not prefs.get("skip_after_choices") then
        flow.set_mode("normal")
    end
    ui.show("choice", { items = items })
end

function flow.choose(index: number)
    ui.hide("choice")
    state.choices = nil
    kulisa.story.choose(index)
end

-- Click, tap, Space or Enter.
function flow.advance()
    if not state.started or state.menu:peek() ~= nil then
        return
    end
    if state.hidden:peek() then
        state.hidden:set(false)
        return
    end
    if state.mode:peek() == "skip" then
        flow.set_mode("normal")
        return
    end
    if state.transition and not state.transition.done then
        state.transition:skip()
        return
    end
    if state.choices then
        return
    end
    local w = what()
    if w and not w.complete then
        w:skip()
        return
    end
    kulisa.story.next()
end

function flow.rollback()
    if not config.rollback or not state.started or state.menu:peek() ~= nil then
        return
    end
    if state.mode:peek() == "skip" then
        flow.set_mode("normal")
    end
    kulisa.story.rollback()
end

function flow.roll_forward()
    if not state.started or state.menu:peek() ~= nil then
        return
    end
    if not kulisa.story.roll_forward() and not state.choices then
        flow.advance()
    end
end

-- After rollback or load: the stage is rebuilt by ui/stage, the line shows at once.
function flow.restored()
    ui.hide("choice")
    state.choices = nil
    state.instant = true
    if state.transition then
        state.transition:skip()
    end
end

kulisa.on("update", function(dt)
    if not state.started then
        return
    end
    local w = what()
    local complete = w ~= nil and w.complete
    if complete ~= state.ctc:peek() then
        state.ctc:set(complete)
    end
    if state.menu:peek() ~= nil or state.choices or state.busy:peek() then
        return
    end
    if skipping() then
        local line = state.line
        if line and not (line.seen or prefs.get("skip_unseen")) and state.mode:peek() ~= "skip" then
            return -- Ctrl does not skip unseen text either
        end
        skip_timer += dt
        if skip_timer >= config.skip_delay then
            skip_timer = 0
            if w then
                w:skip()
            end
            kulisa.story.next()
        end
    elseif state.mode:peek() == "auto" and complete then
        auto_timer += dt
        local chars = if state.line then #state.line.text else 0
        if auto_timer >= prefs.get("auto_delay") + chars * 0.02 then
            auto_timer = 0
            kulisa.story.next()
        end
    end
end)

return flow
