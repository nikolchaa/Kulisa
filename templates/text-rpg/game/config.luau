-- The settings a game usually changes first. Plain data; game/ and ui/ read it.

local config = {}

config.title = "The Lantern Road"
config.id = "kulisa_text_rpg"
config.save_version = 1

-- Landscape on desktops and tablets; phones play in portrait with their own layout (ui/main).
config.display = {
    width = 1920,
    height = 1080,
    scale = "expand",
    aspect = { "4:3", "21:9" },
    orientation = "landscape",
    phone = { width = 1080, height = 1920, orientation = "portrait", aspect = { "9:22", "3:4" } },
}

config.fonts = { "fonts/Inter-Regular.ttf", "fonts/Inter-Bold.ttf" }
config.font = "Inter"

config.colors = {
    bg = "#0e0a08",
    ink = "#1a1310",
    paper = "#efe3cc",
    text = "#e6d8bd",
    dim = "#9c8b72",
    faint = "#6b5d4c",
    gold = "#d9b26a",
    gold_hi = "#ffd88a",
    blood = "#c4433a",
    good = "#8fc46a",
    bad = "#e0705c",
}

config.hero = { name = "Wren", title = "Lantern-bearer", portrait = "gui/portrait.png" }

-- Stats shown on the hero panel: story variables (story/*.kulisa `default`s) and labels.
-- `roll(stat, target)` in stories adds the stat to a d20.
config.attributes = {
    { key = "might", label = "Might" },
    { key = "wit", label = "Wit" },
    { key = "luck", label = "Luck" },
}

-- How many lines the log keeps (and saves).
config.log_size = 120

-- Preference defaults.
config.prefs = { cps = 55, music = 0.6, sound = 0.8 }

return config
