-- The stage: the background and character sprites the story's scene/show/hide lines ask
-- for, and transitions for `with`.
--
-- Images are found by name, as in Ren'Py: `show kitty happy` shows images/kitty happy.png,
-- `scene bg kitchen` shows images/bg kitchen.png. The first word is the tag: showing
-- `kitty neutral` replaces `kitty happy`. `scene black` and `scene white` need no image.
-- Characters stand on the bottom edge, scaled by config.sprite_scale (per tag).
--
-- Layered characters are a folder: images/kitty/base.png, then a folder per layer with a png per
-- look, like images/kitty/face/happy.png. `show kitty happy blush` picks those looks, and a
-- layer's default.png shows when none is asked for. Layers stack by folder name.
--
-- Sprite sheets: config.sheets["kitty walk"] = { cols = 4, rows = 2, fps = 8 } plays
-- images/kitty walk.png (see kulisa.frames).

local config = require("game/config")
local prefs = require("game/prefs")
local state = require("game/state")

local stage = {}

local root = kulisa.layer({ z = 0 })
local bg_layer = kulisa.group({ parent = root, z = 0 })
local sprite_layer = kulisa.group({ parent = root, z = 10 })
local W, H = config.display.width, config.display.height

local background = nil
-- Shown sprites by tag: { node, words, at }.
local shown = {}
local order = 0

local SOLIDS = { black = "#000000", white = "#ffffff" }

local function image_path(words): string?
    local name = table.concat(words, " ")
    for _, path in { `images/{name}.png`, `images/{name}.jpg`, `images/{name}.webp` } do
        if kulisa.exists(path) then
            return path
        end
    end
    return nil
end

-- The layers of images/<tag>/ as { name = { look = path } }, sorted by folder, or nil.
local function layer_folders(tag: string)
    if not kulisa.exists(`images/{tag}/base.png`) then
        return nil
    end
    local layers = {}
    for _, path in kulisa.list(`images/{tag}`) do
        local layer, look = path:match(`^images/{tag}/([^/]+)/([^/]+)%.png$`)
        if layer then
            layers[layer] = layers[layer] or {}
            layers[layer][look] = path
        end
    end
    return layers
end

-- The base image with a child sprite per layer, all the size of the base.
local function layered_sprite(words, layers)
    local tag = words[1]
    local node = kulisa.sprite({ parent = sprite_layer, image = `images/{tag}/base.png` })
    local wanted = {}
    for i = 2, #words do
        wanted[words[i]] = true
    end
    local names = {}
    for name in layers do
        table.insert(names, name)
    end
    table.sort(names)
    for i, name in names do
        local path = layers[name].default
        for look, file in layers[name] do
            if wanted[look] then
                path = file
                wanted[look] = nil
            end
        end
        if path then
            kulisa.sprite({ parent = node, image = path, z = i })
        end
    end
    for look in wanted do
        print(`{tag} has no look \`{look}\` in images/{tag}/`)
    end
    return node
end

-- A labelled gray box for images that do not exist yet.
local function placeholder(parent, words, w: number, h: number)
    local box = kulisa.sprite({ parent = parent, width = w, height = h, tint = "#3a3842", opacity = 0.9 })
    kulisa.text({
        parent = box, x = 16, y = 16, text = `\{b}{table.concat(words, " ")}\{/b}`, size = 28,
        font = config.font, wrap = w - 32, color = config.colors.paper,
    })
    return box
end

-- Scales the background to cover what the window shows (wider screens see more of it).
local function fit_background()
    if not background or background.kind == "group" then
        return
    end
    local vx, vy, vw, vh = kulisa.screen:visible()
    local iw, ih = background.width, background.height
    if background.image == nil then
        iw, ih = 100, 100
    end
    local s = math.max(vw / iw, vh / ih)
    background.scale = s
    background.x = vx + (vw - iw * s) / 2
    background.y = vy + (vh - ih * s) / 2
end

local function set_background(words)
    if background then
        background:remove()
        background = nil
    end
    if words == nil or #words == 0 then
        return
    end
    local solid = SOLIDS[words[1]]
    if solid and #words == 1 then
        background = kulisa.sprite({ parent = bg_layer, tint = solid })
    else
        local path = image_path(words)
        if path then
            background = kulisa.sprite({ parent = bg_layer, image = path })
        else
            background = kulisa.group({ parent = bg_layer })
            placeholder(background, words, W, H)
            background.x, background.y = 0, 0
            return
        end
    end
    fit_background()
end

local function place(entry)
    local node = entry.node
    local at = entry.at or config.default_position
    local x = config.positions[at]
    if x == nil then
        print(`unknown position \`{at}\`; using {config.default_position}`)
        x = config.positions[config.default_position]
    end
    local s = config.sprite_scale[entry.words[1]] or config.sprite_scale.default or 1
    -- Pivot at the feet, so animations scale and turn the character where it stands.
    node.origin_x, node.origin_y = node.width / 2, node.height
    node.scale = s
    node.x = W * x
    node.y = H
end

-- Plays the story's `transform` on a shown tag; nil stops it, back where it stands.
function stage.animate(tag: string, name: string?, instant: boolean?)
    local entry = shown[tag]
    if not entry then
        print(`animate: \`{tag}\` is not on stage`)
        return
    end
    if entry.anim then
        entry.anim.cancel(true)
        entry.anim = nil
    end
    entry.anim_name = name
    if name then
        entry.anim = kulisa.animate(entry.node, name, { instant = instant or state.mode:peek() == "skip" })
    end
end

function stage.show(words, at: string?)
    local tag = words[1]
    local old = shown[tag]
    local z = nil
    if old then
        z = old.node.z
        if old.anim then
            old.anim.cancel()
        end
        old.node:remove()
    end
    local path = image_path(words)
    local layers = if path then nil else layer_folders(tag)
    local node
    if path then
        node = kulisa.sprite({ parent = sprite_layer, image = path })
        local sheet = config.sheets and config.sheets[table.concat(words, " ")]
        if sheet then
            kulisa.frames(node, sheet)
        end
    elseif layers then
        node = layered_sprite(words, layers)
        path = true
    else
        node = kulisa.group({ parent = sprite_layer })
        placeholder(node, words, 400, 700)
    end
    order += 1
    node.z = z or order
    local entry = { node = node, words = words, at = at or (old and old.at) }
    shown[tag] = entry
    if path then
        place(entry)
    else
        local x = config.positions[entry.at or config.default_position] or 0.5
        node.x, node.y = W * x - 200, H - 700
    end
    -- A new look keeps playing what the old one played.
    if old and old.anim_name then
        stage.animate(tag, old.anim_name)
    end
end

function stage.hide(tag: string)
    local entry = shown[tag]
    if entry then
        entry.node:remove()
        shown[tag] = nil
    end
end

function stage.clear()
    for tag in shown do
        stage.hide(tag)
    end
    set_background(nil)
end

function stage.scene(words)
    for tag in shown do
        stage.hide(tag)
    end
    set_background(words)
end

-- Rebuilds everything after a rollback or load (no transitions).
function stage.restore(s)
    stage.clear()
    set_background(s.scene)
    for _, item in s.shown do
        stage.show(item.image, item.at)
        if item.anim then
            -- Where a one-shot ended; a loop starts over.
            stage.animate(item.image[1], item.anim, true)
        end
    end
end

-- Starts the transition called `name` (from config.transitions). Skipping, or turning
-- transitions off in Preferences, makes it instant.
function stage.transition(name: string)
    local t = config.transitions[name]
    if t == nil then
        print(`unknown transition \`{name}\`; add it to config.transitions`)
        return
    end
    local instant = not prefs.get("transitions") or state.mode:peek() == "skip" or kulisa.input.down("control")
    state.transition = kulisa.screen:transition({
        shader = t.shader,
        time = if instant then 0 else t.time,
        ease = t.ease,
        params = t.params,
        mask = t.mask,
    })
    state.busy:set(not state.transition.done)
end

kulisa.on("update", function()
    if state.busy:peek() and (state.transition == nil or state.transition.done) then
        state.busy:set(false)
        if state.window_after then
            state.window_after = false
            state.window:set(true)
        end
    end
end)

-- Story stage events.
function stage.event(e)
    if e.kind == "scene" then
        stage.scene(e.image)
        -- Window auto: the text box waits for the next line.
        state.window:set(false)
    elseif e.kind == "show" then
        stage.show(e.image, e.at)
    elseif e.kind == "hide" then
        stage.hide(e.tag)
    elseif e.kind == "animate" then
        stage.animate(e.tag, e.transform)
    end
    if e.with then
        stage.transition(e.with)
    end
end

kulisa.on("resize", fit_background)
kulisa.on("display", fit_background)

return stage
