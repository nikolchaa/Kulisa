-- Small overlays: the yes/no confirm dialog, notifications, the rotate-your-device hint
-- (web on phones and tablets, where the orientation cannot be locked), and script errors.

local actions = require("game/actions")
local config = require("game/config")
local flow = require("game/flow")
local style = require("ui/style")

local ui = kulisa.ui
local c = config.colors

ui.screen("confirm", function(props)
    return ui.box({
        background = "#000000a0",
        ui.box({
            anchor = "center",
            width = style.size(900),
            background = "gui/frame.png",
            slice = 20,
            slice_scale = style.design,
            padding = style.size(40),
            ui.column({
                width = "fill",
                gap = style.size(36),
                align = "center",
                style.label(props.message, { size = style.size(34), align = "center", width = "fill" }),
                ui.row({
                    gap = style.size(80),
                    style.button("Yes", function()
                        ui.hide("confirm")
                        props.yes()
                    end, { size = style.size(34) }),
                    style.button("No", function()
                        ui.hide("confirm")
                    end, { size = style.size(34), autofocus = true }),
                }),
            }),
        }),
    })
end, { z = 400, modal = true, area = "screen" })

actions.confirm = function(message: string, yes)
    ui.show("confirm", { message = message, yes = yes })
end

ui.screen("notify", function(props)
    return ui.box({
        ui.box({
            anchor = "top_left",
            margin = style.size(24),
            y = style.size(56),
            background = "gui/round.png",
            slice = 16,
            slice_scale = style.design,
            tint = "#111116dd",
            padding = { style.size(10), style.size(20) },
            style.label(props.text, { size = style.size(26) }),
        }),
    })
end, { z = 300, area = "safe" })

local notify_id = 0
flow.notify = function(text: string)
    notify_id += 1
    local mine = notify_id
    ui.show("notify", { text = text })
    kulisa.spawn(function()
        kulisa.wait(2)
        if notify_id == mine then
            ui.hide("notify")
        end
    end)
end

ui.screen("rotate", function()
    return ui.box({
        background = c.ink,
        ui.column({
            anchor = "center",
            align = "center",
            gap = style.size(20),
            ui.box({
                width = style.size(90),
                height = style.size(150),
                background = "gui/round.png",
                slice = 16,
                slice_scale = style.design,
                tint = c.accent,
                padding = style.size(8),
                ui.box({ width = "fill", height = "fill", background = "gui/round.png", slice = 16, slice_scale = style.design, tint = c.ink }),
            }),
            style.label("Please turn your device sideways", { size = style.size(40) }),
        }),
    })
end, { z = 1000, modal = true, area = "screen" })

local function check_rotation()
    if kulisa.screen.mismatch then
        ui.show("rotate")
    elseif ui.shown("rotate") then
        ui.hide("rotate")
    end
end
kulisa.on("display", check_rotation)

ui.screen("error", function(props)
    return ui.box({
        ui.box({
            anchor = "top",
            width = "90%",
            margin = style.size(16),
            background = "gui/round.png",
            slice = 16,
            slice_scale = style.design,
            tint = "#3a1416ee",
            padding = style.size(18),
            ui.column({
                width = "fill",
                gap = style.size(8),
                style.label("{b}Script error{/b}", { size = style.size(26), color = c.accent }),
                style.label(props.text, { size = style.size(22), width = "fill" }),
                style.button("Dismiss", function()
                    ui.hide("error")
                end, { size = style.size(22) }),
            }),
        }),
    })
end, { z = 900, area = "safe" })

kulisa.on("error", function(message: string)
    local first = message:split("\n")[1]
    ui.show("error", { text = (first:gsub("{", "{{"):gsub("}", "}}")) })
end)

return { check_rotation = check_rotation }
