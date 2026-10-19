-- Classic VN template: a Ren'Py-style visual novel interface built only with the public
-- Kulisa API. Edit game/config.luau first; the screens are in ui/, the story in story/.
--
-- Controls
--   Advance: click, tap, Space, Enter. Back: wheel up, Page Up (wheel down, Page Down roll
--   forward). Skip: Tab toggles, Ctrl skips while held. Auto: A. Hide the interface: H or
--   middle click. Game menu: Escape, right click, long press, Android back. History: swipe
--   down. Touch screens can roll back by tapping the side set in Preferences.

local config = require("game/config")

kulisa.display(config.display)
kulisa.save.setup({ id = config.id, version = config.save_version })
for _, f in config.fonts do
    kulisa.load_font(f)
end
kulisa.screen.clear = config.colors.ink

local actions = require("game/actions")
local flow = require("game/flow")
local prefs = require("game/prefs")
local stage = require("game/stage")
local state = require("game/state")
require("ui/say")
require("ui/choice")
require("ui/quick")
require("ui/game_menu")
require("ui/main_menu")
local overlays = require("ui/overlays")

prefs.load()

-- Story events ---------------------------------------------------------------------------

kulisa.on("say", flow.say)
kulisa.on("menu", flow.menu)
kulisa.on("stage", stage.event)
kulisa.on("restore", function(s)
    stage.restore(s)
    flow.restored()
end)
kulisa.on("pause", function()
    state.ctc:set(false)
end)
kulisa.on("end", function()
    actions.main_menu()
end)

-- Input ----------------------------------------------------------------------------------

local function playing(): boolean
    return state.started and state.menu:peek() == nil and not kulisa.ui.shown("confirm")
end

kulisa.on("tap", function(x)
    if not playing() then
        return
    end
    local side = prefs.get("rollback_side")
    if kulisa.screen.touch and side ~= "disable" then
        local vx, _, vw = kulisa.screen:visible()
        local edge = vw * 0.2
        if (side == "left" and x < vx + edge) or (side == "right" and x > vx + vw - edge) then
            flow.rollback()
            return
        end
    end
    flow.advance()
end)

kulisa.on("swipe", function(dir)
    if playing() and dir == "down" then
        actions.open_menu("history")
    end
end)

kulisa.on("long_press", function()
    if playing() then
        actions.open_menu("save")
    end
end)

kulisa.on("press", function(_, _, button)
    if button == "right" then
        actions.toggle_menu("save")
    elseif button == "middle" then
        actions.toggle_hidden()
    end
end)

kulisa.on("wheel", function(lines)
    if not playing() then
        return
    end
    if lines > 0 then
        flow.rollback()
    else
        flow.roll_forward()
    end
end)

kulisa.on("key", function(key)
    if key == "escape" or key == "back" then
        actions.toggle_menu("save")
        return
    end
    if not playing() then
        return
    end
    if key == "space" or key == "enter" then
        flow.advance()
    elseif key == "tab" then
        flow.toggle("skip")
    elseif key == "a" then
        flow.toggle("auto")
    elseif key == "h" then
        actions.toggle_hidden()
    elseif key == "pageup" then
        flow.rollback()
    elseif key == "pagedown" then
        flow.roll_forward()
    elseif state.choices then
        local n = tonumber(key)
        local item = n and state.choices.items[n]
        if item then
            flow.choose(item.index)
        end
    end
end)

-- Start ----------------------------------------------------------------------------------

kulisa.story.load()
stage.scene({ "bg", "night" })
kulisa.ui.show("main_menu")
overlays.check_rotation()
