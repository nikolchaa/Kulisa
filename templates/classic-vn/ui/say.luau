-- The text box: the speaker's name box and the line, with a click-to-continue arrow.
-- Laid out in the safe area (clear of notches and system bars), no wider than 88% of the
-- design width, so lines keep a readable length on ultrawide screens.

local config = require("game/config")
local prefs = require("game/prefs")
local state = require("game/state")
local style = require("ui/style")

local ui = kulisa.ui
local c = config.colors

ui.screen("say", function(props)
    local line = props and props.line or state.line
    prefs.changed()
    local shown = state.window() and not state.hidden() and line ~= nil
    -- During a transition the box stays up but the next line waits for it, as in Ren'Py.
    local busy = state.busy()
    local who = line and line.who
    local cps = if props and props.instant or state.mode() == "skip" then 0 else prefs.get("cps")
    local name = not busy and who and (who.name or who.id)
    return ui.box({
        visible = shown,
        ui.box({
            anchor = "bottom",
            width = "88%",
            max_width = config.display.width * 0.88,
            height = style.size(250),
            y = -style.size(62),
            background = "gui/textbox.png",
            slice = 20,
            slice_scale = style.design,
            padding = { style.size(34), style.size(48) },
            name and ui.box({
                x = 0,
                y = -style.size(86),
                background = "gui/namebox.png",
                slice = 14,
                slice_scale = style.design,
                padding = { style.size(8), style.size(22) },
                style.label(`\{b}{name}\{/b}`, { size = style.size(config.text.name_size), color = who.color or c.paper }),
            }),
            not busy and ui.text({
                id = "what",
                -- A new key per line restarts the typewriter, even for repeated text.
                key = tostring(line and line.n or 0),
                text = line and line.text or "",
                width = "fill",
                size = style.size(config.text.size),
                line_height = config.text.line_height,
                font = config.font,
                color = c.paper,
                cps = cps,
            }),
            ui.image({
                image = "gui/ctc.png",
                anchor = "bottom_right",
                width = style.size(24),
                height = style.size(24),
                tint = c.accent,
                visible = state.ctc() and not busy,
            }),
        }),
    })
end, { z = 100, area = "safe" })

return {}
