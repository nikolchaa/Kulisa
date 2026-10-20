-- Text RPG template: an illustrated text adventure with a scrolling log, choices, a hero panel,
-- an inventory and dice checks, driven by .kulisa stories (story/) plus Luau rules (game/).
-- Built only with the public Kulisa API. See README.md.
--
-- Controls
--   Click or tap the log, Space or Enter: continue. 1-9: pick a choice (or the arrow keys /
--   d-pad, then Enter / A). F5 quick save, F9 quick load, Escape opens the menu.

local config = require("game/config")

kulisa.display(config.display)
kulisa.save.setup({ id = config.id, version = config.save_version })
for _, f in config.fonts do
    kulisa.load_font(f)
end
kulisa.screen.clear = config.colors.bg

local play = require("game/play")
require("game/rules")
local main = require("ui/main")
local menus = require("ui/menus")


-- Preferences --------------------------------------------------------------------------------------

local prefs = kulisa.persist.read("prefs")
if type(prefs) == "table" then
    for k, v in prefs do
        menus.prefs[k] = v
    end
end

local function apply()
    main.prefs.cps = menus.prefs.cps
    kulisa.audio.volume("music", menus.prefs.music)
    kulisa.audio.volume("sound", menus.prefs.sound)
end

menus.set_pref = function(key, value)
    menus.prefs[key] = value
    apply()
    kulisa.persist.write("prefs", menus.prefs)
end

-- Saves ------------------------------------------------------------------------------------------------

local function save(slot: string)
    local title = nil
    local vars = kulisa.story.vars()
    if vars.chapter then
        title = vars.chapter
    end
    kulisa.save.write(slot, { thumbnail = kulisa.screen:capture(384, 216), title = title })
end

local function load(slot: string)
    if not kulisa.save.exists(slot) then
        return
    end
    menus.show(nil)
    play.started = true
    kulisa.save.load(slot)
end

menus.save = save
menus.load = load

-- Input ------------------------------------------------------------------------------------------------

kulisa.on("key", function(key)
    if key == "escape" or key == "back" then
        if menus.open:peek() == nil and play.started then
            menus.show(if kulisa.screen.profile == "phone" then "menu" else "save")
        elseif menus.open:peek() ~= "title" then
            menus.show(if play.started then nil else "title")
        end
        return
    end
    if menus.open:peek() ~= nil or not play.started then
        return
    end
    if key == "space" or key == "enter" then
        play.advance()
    elseif key == "f5" then
        save("quick")
    elseif key == "f9" then
        load("quick")
    else
        local n = tonumber(key)
        local list = play.choices:peek()
        if n and list and list[n] then
            play.choose(list[n])
        end
    end
end)

kulisa.on("pad_press", function(button)
    if menus.open:peek() == nil and play.started and button == "a" and play.choices:peek() == nil then
        play.advance()
    elseif button == "start" or button == "b" then
        menus.show(if menus.open:peek() == nil then "save" elseif play.started then nil else "title")
    end
end)

-- Start ------------------------------------------------------------------------------------------------

apply()
kulisa.story.load()
menus.show("title")
