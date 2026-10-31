-- The main menu, over the stage's background.

local actions = require("game/actions")
local config = require("game/config")
local style = require("ui/style")

local ui = kulisa.ui
local t = kulisa.t
local c = config.colors

ui.screen("main_menu", function()
    local nav = {
        style.button(t("Start"), actions.start, { size = style.size(40), id = "start", autofocus = true }),
        style.button(t("Load"), function()
            actions.open_menu("load")
        end, { size = style.size(40) }),
        style.button(t("Preferences"), function()
            actions.open_menu("prefs")
        end, { size = style.size(40) }),
        kulisa.window.can_quit and style.button(t("Quit"), actions.quit, { size = style.size(40) }),
    }
    return ui.box({
        ui.box({
            anchor = "left",
            height = "fill",
            width = style.size(560),
            background = c.ink .. "c8",
            padding = { style.size(90), style.size(70) },
            ui.column({
                height = "fill",
                margin = style.safe_padding(0),
                gap = style.size(14),
                style.label(`\{b}{t(config.title)}\{/b}`, { size = style.size(64), color = c.paper }),
                style.label(t("Version {version}", { version = config.version }), { size = style.size(24), color = c.haze }),
                ui.spacer({ width = 1 }),
                nav,
                ui.spacer({ width = 1, height = style.size(40) }),
                style.label(t("Made with Kulisa {version}", { version = kulisa.version }), { size = style.size(20), color = c.haze }),
            }),
        }),
    })
end, { z = 150, modal = true, area = "screen" })

return {}
