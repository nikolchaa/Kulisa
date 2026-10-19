-- Menu choices: centered cards. Choices picked in an earlier playthrough are dimmer.

local config = require("game/config")
local flow = require("game/flow")
local state = require("game/state")
local style = require("ui/style")

local ui = kulisa.ui
local c = config.colors

ui.screen("choice", function(props)
    local cards = {}
    for _, item in props.items do
        table.insert(cards, style.card(function()
            flow.choose(item.index)
        end, {
            key = tostring(item.index),
            width = style.size(820),
            padding = style.size(18),
            -- Keyboard and gamepad start on the first choice.
            autofocus = item.index == props.items[1].index,
        }, {
            ui.text({
                text = item.text,
                width = "fill",
                align = "center",
                size = style.size(32),
                font = config.font,
                color = if item.chosen then c.haze else c.paper,
                hover_color = c.paper,
            }),
        }))
    end
    return ui.box({
        visible = not state.hidden(),
        ui.column({
            anchor = "center",
            y = -style.size(90),
            gap = style.size(18),
            align = "center",
            cards,
        }),
    })
end, { z = 120, area = "design", nav = true })

return {}
