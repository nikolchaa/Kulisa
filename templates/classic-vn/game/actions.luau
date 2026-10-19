-- What buttons and keys do: open menus, save, load, start, quit.

local flow = require("game/flow")
local history = require("game/history")
local prefs = require("game/prefs")
local stage = require("game/stage")
local state = require("game/state")

local actions = {}
local ui = kulisa.ui

-- Shows `message` with Yes/No; runs `yes` on Yes. Set by ui/confirm.
actions.confirm = function(_: string, yes)
    yes()
end

-- The in-game screens (shown while the story runs).
local GAME_SCREENS = { "say", "quick" }

function actions.open_menu(section: string)
    if state.menu:peek() ~= nil then
        state.menu:set(section)
        return
    end
    if state.started then
        -- The thumbnail for saves: the game as it was when the menu opened.
        state.shot = kulisa.screen:capture(384, 216, state.shot)
        flow.set_mode("normal")
    end
    state.menu:set(section)
    ui.show("game_menu")
end

function actions.close_menu()
    if state.menu:peek() == nil then
        return
    end
    state.menu:set(nil)
    ui.hide("game_menu")
    prefs.flush(true)
end

function actions.toggle_menu(section: string?)
    if ui.shown("confirm") then
        ui.hide("confirm")
    elseif state.menu:peek() ~= nil then
        actions.close_menu()
    elseif state.started then
        actions.open_menu(section or "save")
    end
end

function actions.save(slot: string)
    kulisa.save.write(slot, { thumbnail = state.shot })
    ui.refresh("game_menu")
end

local function enter_game()
    state.started = true
    ui.hide("main_menu")
    for _, s in GAME_SCREENS do
        ui.show(s)
    end
end

function actions.load(slot: string)
    if not kulisa.save.exists(slot) then
        return
    end
    actions.close_menu()
    if not state.started then
        enter_game()
    end
    flow.set_mode("normal")
    kulisa.save.load(slot)
end

function actions.quick_save()
    if not state.started then
        return
    end
    kulisa.save.write("quick", { thumbnail = kulisa.screen:capture(384, 216) })
    flow.notify("Quick saved.")
end

function actions.quick_load()
    if kulisa.save.exists("quick") then
        actions.load("quick")
    else
        flow.notify("No quick save yet.")
    end
end

function actions.start()
    history.clear()
    state.count = 0
    stage.clear()
    enter_game()
    flow.set_mode("normal")
    kulisa.story.start()
end

-- Back to the main menu (from the game menu or the story's end).
function actions.main_menu()
    actions.close_menu()
    state.started = false
    flow.set_mode("normal")
    for _, s in GAME_SCREENS do
        ui.hide(s)
    end
    ui.hide("choice")
    state.window:set(false)
    state.line = nil
    stage.clear()
    stage.scene({ "bg", "night" })
    kulisa.story.load()
    ui.show("main_menu")
end

function actions.quit()
    actions.confirm("Are you sure you want to quit?", function()
        prefs.flush(true)
        kulisa.window:quit()
    end)
end

function actions.toggle_hidden()
    if state.started and state.menu:peek() == nil then
        state.hidden:set(not state.hidden:peek())
    end
end

return actions
