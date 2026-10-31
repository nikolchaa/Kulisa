-- Self-voicing: reads lines, choices and the focused button aloud with the system's voice, so the
-- game can be played without looking at the screen. V or Preferences turns it on.
--
-- A character with `{ alt = true }` is heard only while self-voicing, for what the screen shows:
--     define alt = Character(nil, { alt = true })
--     alt "A sunlit kitchen. Kitty sits on the counter."

local prefs = require("game/prefs")

local voice = {}
local spoken_focus = nil

local function volume()
    return if prefs.get("mute") then 0 else prefs.get("voice")
end

function voice.on(): boolean
    return prefs.get("self_voicing") and kulisa.speech.available()
end

-- Says `text` if self-voicing is on. `queue` lets what is being said finish first.
function voice.say(text: string, queue: boolean?)
    if voice.on() then
        kulisa.speech.say(text, { interrupt = not queue, volume = volume() })
    end
end

function voice.line(line, queue: boolean?)
    local who = line.who
    if who and who.name and not who.alt then
        voice.say(`{kulisa.t(who.name)}: {line.text}`, queue)
    else
        voice.say(line.text, queue)
    end
end

function voice.menu(items, queue: boolean?)
    local texts = {}
    for i, item in items do
        texts[i] = `{i}. {item.text}`
    end
    voice.say(`{kulisa.t("Choices")}: {table.concat(texts, ", ")}`, queue)
end

function voice.toggle()
    local on = not prefs.get("self_voicing")
    prefs.set("self_voicing", on)
    if on then
        voice.say(kulisa.t("Self-voicing on"))
    else
        kulisa.speech.stop()
    end
end

-- Whatever the keyboard or gamepad moves to is read out.
kulisa.on("update", function()
    if not voice.on() then
        spoken_focus = nil
        return
    end
    local text = kulisa.ui.focused_text()
    if text ~= spoken_focus then
        spoken_focus = text
        if text then
            voice.say(text)
        end
    end
end)

return voice
