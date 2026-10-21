-- The settings a game usually changes first. Plain data; game/ and ui/ read it.

local config = {}

-- Title, save id and display shapes (phones play in portrait) live in kulisa.toml.
config.title = kulisa.config.game.title
config.save_version = 1

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
