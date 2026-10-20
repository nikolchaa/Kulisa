-- The settings a game usually changes first. Everything here is plain data; the screens in
-- ui/ and the logic in game/ read it.

local config = {}

config.title = "Whiskers & Sunbeams"
config.version = "1.0"
-- Save folder name. Change it for your game, or its saves mix with other games' saves.
config.id = "kulisa_classic_vn"
-- Bump when story variables change shape, and add a kulisa.save.migrate step (main.luau).
config.save_version = 1

config.display = {
    width = 1920,
    height = 1080,
    scale = "expand",
    -- Wider than 21:9 or narrower than 4:3 gets bars instead of unpainted space.
    aspect = { "4:3", "21:9" },
    orientation = "landscape",
}

config.fonts = { "fonts/Inter-Regular.ttf", "fonts/Inter-Bold.ttf" }
config.font = "Inter"

-- Warm cream on cocoa, with a pink accent (the colour of a kitten's nose).
config.colors = {
    ink = "#1c1310",
    paper = "#fbf3e6",
    accent = "#f2948a",
    haze = "#a8968a",
    panel = "#2b1d18",
    idle = "#dccbb8",
    disabled = "#6a5a52",
}

config.text = {
    size = 34,
    name_size = 30,
    line_height = 1.45,
}

-- Rollback (the Back button, the wheel, page up). Set to false to turn it off entirely.
config.rollback = true
-- Autosave at every menu.
config.autosave = true
-- Seconds per line while skipping.
config.skip_delay = 0.06
-- History keeps this many lines.
config.history_size = 250

-- Named positions for `show x at left`: where the sprite's center sits (0 = left edge of
-- the design area, 1 = right edge).
config.positions = {
    left = 0.18,
    midleft = 0.33,
    center = 0.5,
    midright = 0.67,
    right = 0.82,
}
config.default_position = "center"

-- How big characters are drawn (their images are scaled by this), per tag; `default` for the rest.
config.sprite_scale = {
    default = 1,
    kitty = 0.62,
    fluffy = 0.85,
}

-- The scene behind the main menu.
config.menu_background = { "bg", "playroom" }

-- Transitions for `with name`. `shader` gets the old screen as `tex` and the progress in
-- params.p[0].x; `params` fill the rest (see shaders/). Add your own here.
config.transitions = {
    fade = { shader = "shaders/fade.wgsl", time = 1.0, params = { 0, 0, 0 } },
    dissolve = { shader = "shaders/dissolve.wgsl", time = 0.5 },
    wiperight = { shader = "shaders/wipe.wgsl", time = 0.8, params = { 0, 0.15 } },
    wipeleft = { shader = "shaders/wipe.wgsl", time = 0.8, params = { 1, 0.15 } },
    wipedown = { shader = "shaders/wipe.wgsl", time = 0.8, params = { 2, 0.15 } },
    wipeup = { shader = "shaders/wipe.wgsl", time = 0.8, params = { 3, 0.15 } },
    swirl = { shader = "shaders/wipe.wgsl", time = 1.2, params = { 4, 0.2 }, mask = "masks/swirl.png" },
}

-- Preference defaults (players change them in Preferences).
config.prefs = {
    cps = 50, -- characters per second; 0 is instant
    auto_delay = 1.5, -- seconds after a line finishes, plus a little per character
    skip_unseen = false,
    skip_after_choices = false,
    transitions = true,
    rollback_side = "disable", -- touch: tapping this edge rolls back ("left", "right")
    music = 0.7,
    sound = 0.8,
    voice = 1.0,
    mute = false,
    fullscreen = false,
}

return config
