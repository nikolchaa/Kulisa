-- Desktop OS template: a story told through a fake computer, built only with the public Kulisa
-- API. The player reads chats, browses a small fake web and takes notes while the story
-- (story/*.kulisa) messages them. See README.md.
--
-- Controls
--   Mouse or touch: windows drag by the title bar, resize from edges and corners, double-click
--   the title bar to maximize. Desktop icons open with a double-click (one tap on touch) and
--   can be dragged around. Notifications open what they are about.
--   Space / Enter: continue a caption. Escape: close the start menu or a dialog.

local config = require("os/config")

kulisa.save.setup({ version = config.save_version })
for _, f in config.fonts do
    kulisa.load_font(f)
end
kulisa.screen.clear = "#05060a"

local data = require("os/data")
local shell = require("os/shell")
local theme = require("os/theme")
local wm = require("os/wm")
local bridge = require("os/story")

local apps = {
    chat = require("apps/chat"),
    browser = require("apps/browser"),
    notes = require("apps/notes"),
    settings = require("apps/settings"),
}
wm.apps = apps
local settings = apps.settings
local ui = kulisa.ui

-- Settings players keep between sessions -------------------------------------------------------

local saved_prefs = kulisa.persist.read("prefs")
if type(saved_prefs) == "table" then
    for k, v in saved_prefs do
        settings.prefs[k] = v
    end
end

local function apply_prefs()
    local p = settings.prefs
    kulisa.audio.volume("music", p.music)
    kulisa.audio.volume("sound", p.sound)
    if kulisa.window.can_quit and kulisa.window.fullscreen ~= p.fullscreen then
        kulisa.window.fullscreen = p.fullscreen
    end
    shell.set_wallpaper(p.wallpaper)
end

settings.on_change = function()
    apply_prefs()
    kulisa.persist.write("prefs", settings.prefs)
end

-- What saves remember besides the story: chats, notes, browsing, windows, clock -----------------

kulisa.state.register("desk", {
    save = function()
        return {
            data = data.save(),
            browser = apps.browser.save(),
            windows = wm.snapshot(),
            clock = theme.time_now(),
            wallpaper = shell.wallpaper,
            notifications = shell.history,
        }
    end,
    load = function(d)
        d = d or {}
        data.load(d.data)
        apps.browser.load(d.browser)
        theme.clock = { minutes = d.clock or theme.clock.minutes, at = kulisa.now() }
        if d.wallpaper then
            shell.set_wallpaper(d.wallpaper)
        end
        table.clear(shell.history)
        for _, n in d.notifications or {} do
            table.insert(shell.history, n)
        end
        wm.restore_snapshot(d.windows)
    end,
})

local function signed_in()
    return not ui.shown("signin")
end

settings.save_slot = function(slot: string)
    local shot = kulisa.screen:capture(384, 216)
    kulisa.save.write(slot, { thumbnail = shot })
    shell.notify({ title = "Saved", text = "Your progress was saved.", app = "settings", silent = true })
end

local function load_slot(slot: string)
    if ui.shown("signin") then
        ui.hide("signin")
        shell.show_desktop()
    end
    shell.close_dialog()
    bridge.started = true
    kulisa.save.load(slot)
end

-- Live reload (`kulisa run`): a script change restarts the game; go back to the same line.
kulisa.on("reload", function(slot)
    if slot then
        load_slot(slot)
    end
end)

settings.load_slot = function(slot: string)
    shell.dialog({
        title = "Load this save?",
        text = "Anything since your last save is lost.",
        buttons = { { "Cancel" }, { "Load", function()
            load_slot(slot)
        end, true } },
    })
end

-- Start menu and sign-in --------------------------------------------------------------------------

local function begin()
    shell.sign_in(function()
        theme.sound("startup", 0.6)
        kulisa.after(1.2, bridge.start)
    end)
end

local function newest_save()
    local best = nil
    for _, s in kulisa.save.list() do
        if best == nil or s.timestamp > best.timestamp then
            best = s
        end
    end
    return best
end

local function load_menu()
    local slots = {}
    for _, s in kulisa.save.list() do
        table.insert(slots, { os.date("%d %b %H:%M", s.timestamp) .. "  —  " .. s.slot, function()
            load_slot(s.slot)
        end })
    end
    table.insert(slots, { "Cancel" })
    shell.dialog({ title = "Load", buttons = slots })
end

bridge.restart = function()
    wm.close_all()
    data.load(nil)
    apps.browser.load(nil)
    table.clear(shell.history)
    shell.unread:set(0)
    shell.caption(nil)
    shell.hint(nil)
    bridge.start()
end

local refresh_signin

shell.actions = {
    { label = "Save", icon = "save", run = function() wm.open("settings", "saves") end },
    { label = "Settings", icon = "maximize", run = function() wm.open("settings", "appearance") end },
    { label = "Restart story", icon = "reload", run = function()
        shell.dialog({ title = "Start the story over?", text = "Unsaved progress is lost.", buttons = { { "Cancel" }, { "Start over", bridge.restart, true } } })
    end },
    { label = if kulisa.window.can_quit then "Shut down" else "Sign out", icon = "power", run = function()
        if kulisa.window.can_quit then
            kulisa.window:quit()
        else
            wm.close_all()
            ui.hide("desktop")
            ui.hide("taskbar")
            refresh_signin()
            ui.show("signin")
        end
    end },
}

refresh_signin = function()
    shell.signin_buttons = { { if newest_save() then "New story" else "Sign in", begin, true } }
    if newest_save() then
        table.insert(shell.signin_buttons, 1, { "Continue", function()
            load_slot(newest_save().slot)
        end, true })
        shell.signin_buttons[2][3] = false
        table.insert(shell.signin_buttons, { "Load…", load_menu })
    end
end

-- Input ---------------------------------------------------------------------------------------------

kulisa.on("key", function(key)
    if kulisa.input.text_active() then
        return
    end
    if key == "escape" or key == "back" then
        if shell.dialog_shown() then
            shell.close_dialog()
        elseif ui.shown("start") then
            shell.close_start()
        end
    elseif (key == "space" or key == "enter") and signed_in() and not shell.dialog_shown() then
        bridge.advance()
    end
end)

-- Start -----------------------------------------------------------------------------------------------

apply_prefs()
kulisa.story.load()
shell.boot()
refresh_signin()
ui.show("signin")
