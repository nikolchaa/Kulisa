-- Player preferences: kept in persistent data (not in saves), applied to audio and the
-- window. Screens read `prefs.changed` so they rebuild when a preference changes.

local config = require("game/config")

local prefs = {}
local values = table.clone(config.prefs)
local dirty = false
local since = 0

prefs.changed = kulisa.ui.signal(0)

local function apply()
    local mute = values.mute
    kulisa.audio.volume("music", if mute then 0 else values.music)
    kulisa.audio.volume("sound", if mute then 0 else values.sound)
    kulisa.audio.volume("voice", if mute then 0 else values.voice)
end

function prefs.load()
    local saved = kulisa.persist.read("prefs")
    if type(saved) == "table" then
        for k, v in saved do
            if values[k] ~= nil and type(v) == type(values[k]) then
                values[k] = v
            end
        end
    end
    apply()
    if values.fullscreen and kulisa.window.fullscreen ~= true then
        kulisa.window.fullscreen = true
    end
end

function prefs.get(key: string)
    return values[key]
end

function prefs.set(key: string, value)
    if values[key] == value then
        return
    end
    values[key] = value
    if key == "fullscreen" then
        kulisa.window.fullscreen = value
    end
    apply()
    dirty = true
    prefs.changed:set(prefs.changed:peek() + 1)
end

-- Writes changed preferences at most twice a second (sliders change them every frame).
function prefs.flush(force: boolean?)
    if dirty and (force or kulisa.now() - since > 0.5) then
        kulisa.persist.write("prefs", values)
        dirty = false
        since = kulisa.now()
    end
end

kulisa.on("update", function()
    prefs.flush()
end)

return prefs
