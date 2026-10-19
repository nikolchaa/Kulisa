-- The quick menu under the text box, and the "Skipping" / "Auto" indicator.

local actions = require("game/actions")
local config = require("game/config")
local flow = require("game/flow")
local state = require("game/state")
local style = require("ui/style")

local ui = kulisa.ui

ui.screen("quick", function()
    local mode = state.mode()
    local size = style.size(22)
    local function b(text, f, opts)
        opts = opts or {}
        opts.size = size
        return style.button(text, f, opts)
    end
    return ui.box({
        visible = not state.hidden() and state.menu() == nil,
        ui.row({
            anchor = "bottom",
            y = -style.size(2),
            gap = style.size(22),
            config.rollback and b("Back", flow.rollback, { disabled = not kulisa.story.can_rollback() }),
            b("History", function()
                actions.open_menu("history")
            end),
            b("Skip", function()
                flow.toggle("skip")
            end, { selected = mode == "skip" }),
            b("Auto", function()
                flow.toggle("auto")
            end, { selected = mode == "auto" }),
            b("Save", function()
                actions.open_menu("save")
            end),
            b("Q.Save", actions.quick_save),
            b("Q.Load", actions.quick_load),
            b("Prefs", function()
                actions.open_menu("prefs")
            end),
        }),
        mode ~= "normal" and ui.box({
            anchor = "top_left",
            margin = style.size(20),
            background = "gui/round.png",
            slice = 16,
            tint = "#111116cc",
            padding = { style.size(8), style.size(18) },
            style.label(if mode == "skip" then "Skipping" else "Auto", { size = style.size(24), color = config.colors.accent }),
        }),
    })
end, { z = 110, area = "safe" })

return {}
