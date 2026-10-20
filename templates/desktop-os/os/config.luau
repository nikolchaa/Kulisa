-- The settings a game usually changes first: names, colors, apps, wallpapers. Plain data; the
-- rest of os/ and apps/ reads it.

local config = {}

config.title = "Gull Point"
-- Save folder name: change it for your game.
config.id = "kulisa_desktop_os"
config.save_version = 1

config.display = {
    width = 1920,
    height = 1080,
    scale = "expand",
    aspect = { "4:3", "21:9" },
    orientation = "landscape",
}

config.fonts = { "fonts/Inter-Regular.ttf", "fonts/Inter-Bold.ttf" }
config.font = "Inter"

-- The player's account on the fake computer.
config.user = { name = "Alex Rowan", avatar = "avatars/me.png" }

config.colors = {
    text = "#e9ebf3",
    dim = "#a3a9bc",
    faint = "#6d748b",
    accent = "#8ea2ff",
    accent2 = "#ff8fb3",
    good = "#4fe0a0",
    panel = "#1b1f2d",
    panel_hi = "#262b3d",
    line = "#ffffff14",
    hover = "#ffffff12",
    pressed = "#ffffff1f",
    bubble_in = "#2a3044",
    bubble_out = "#5c6cff",
    danger = "#ff5f6d",
}

-- Apps by name: title, icon, default window size (design units), whether only one window of it
-- can be open, and the module that builds its content (apps/<name>.luau).
config.apps = {
    chat = { title = "Messages", icon = "apps/chat.png", size = { 980, 680 }, single = true },
    browser = { title = "Browser", icon = "apps/browser.png", size = { 1180, 780 }, single = true },
    notes = { title = "Notes", icon = "apps/notes.png", size = { 760, 560 }, single = true },
    settings = { title = "Settings", icon = "apps/settings.png", size = { 860, 640 }, single = true },
}
-- Taskbar and start menu order; desktop icons.
config.pinned = { "chat", "browser", "notes", "settings" }
config.desktop = { "chat", "browser", "notes", "settings" }

-- Wallpapers are one shader with different params (shaders/wallpaper.wgsl): three colors and a
-- flow speed. Add your own here.
config.wallpapers = {
    dusk = { name = "Dusk", colors = { "#1b1440", "#7a3b78", "#ff8a6b" }, speed = 0.05 },
    tide = { name = "Tide", colors = { "#04121f", "#0f4c6a", "#3ad0c0" }, speed = 0.04 },
    ember = { name = "Ember", colors = { "#120808", "#5a1f2a", "#ffb35c" }, speed = 0.06 },
}
config.wallpaper_order = { "dusk", "tide", "ember" }
config.wallpaper = "dusk"

-- Seconds a contact "types" per character before a message arrives (clamped).
config.typing = { per_char = 0.035, min = 0.7, max = 2.4 }
-- Seconds the story waits after a message the player can see, plus a little per character.
config.read_delay = { base = 0.7, per_char = 0.012 }

return config
