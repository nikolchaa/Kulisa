-- Smoke demo: one small scene that touches every engine feature, built only with the
-- public `kulisa` API. Layers and parallax, a render target behind a sprite shader, a
-- clipped panel, a screen effect, a typewriter dialogue box and choice menu driven by
-- story/demo.kulisa, music, rollback and quick saves.

print(`hello from {_VERSION} on Kulisa {kulisa.version} ({kulisa.platform})`)

kulisa.display(1920, 1080, "expand")
kulisa.screen.clear = "#111116"
kulisa.save.setup({ id = "kulisa_smoke", version = 1 })
kulisa.load_font("fonts/Inter-Regular.ttf")
kulisa.load_font("fonts/Inter-Bold.ttf")

local MUSIC_VOLUME = 0.35
kulisa.audio.volume("music", MUSIC_VOLUME)

local logo_image = kulisa.load_image("images/logo.png")
local wave = kulisa.load_shader("shaders/wave.wgsl")
local vignette = kulisa.load_shader("shaders/vignette.wgsl")

-- Far backdrop: a gradient wider than any screen, barely moving.
local back = kulisa.layer({ parallax = 0.1 })
local sky = kulisa.sprite({ parent = back, image = "images/sky.png", x = -1040, y = -260, width = 4000, height = 1600 })

-- Stage flats at three depths. They share one texture, so they batch.
for depth, flat in { { 0.3, "#24222c", 7 }, { 0.6, "#2f2a36", 6 }, { 0.85, "#3b3340", 5 } } do
    local d = depth - 1
    local parallax, color, count = flat[1], flat[2], flat[3]
    local layer = kulisa.layer({ parallax = parallax, z = d })
    for i = 0, count - 1 do
        local w = 220 + 40 * d
        local h = 380 + 120 * d + ((i * 37) % 5) * 30
        local x = -400 + i * (2700 / count) + d * 90
        kulisa.sprite({ parent = layer, x = x, y = 1080 - h + 40, width = w, height = h, tint = color })
    end
end

-- World: the logo (shown by the story) with a soft additive halo behind it (a child with
-- negative z draws behind its parent), and a render target shown through a wave shader.
local world = kulisa.layer({ z = 10 })
local logo = kulisa.sprite({
    parent = world, image = logo_image, x = 960, y = 520, width = 320, height = 320, center = true, visible = false,
})
kulisa.sprite({
    parent = logo, image = "images/halo.png", x = 160, y = 160, width = 620, height = 620, center = true, z = -1,
    blend = "add", tint = "#ff6a3d", opacity = 0.22,
})
local target = kulisa.target({ parent = world, width = 256, height = 256, clear = "#1e1c24" })
local mini = kulisa.sprite({ parent = target, image = logo_image, x = 128, y = 128, width = 160, height = 160, center = true })
kulisa.sprite({
    parent = world, image = target.texture, x = 1400, y = 380, shader = wave, params = { 0.02, 18, 3, 0 },
})

-- HUD: fixed to the screen, with a clipped panel.
local hud = kulisa.layer({ parallax = 0, z = 20 })
local panel = kulisa.group({ parent = hud, x = 40, y = 40, clip = { 0, 0, 260, 64 } })
kulisa.sprite({ parent = panel, width = 400, height = 64, tint = "#ff6a3d", opacity = 0.85 })
kulisa.sprite({ parent = panel, image = logo_image, x = 8, y = 8, width = 48, height = 48 })

-- Dialogue box and menu: a typical visual novel UI from the same primitives.
local ui = kulisa.layer({ parallax = 0, z = 30 })
kulisa.sprite({ parent = ui, x = 160, y = 800, width = 1600, height = 236, tint = "#111116", opacity = 0.82 })
local name = kulisa.text({ parent = ui, x = 200, y = 816, font = "Inter", size = 30 })
local line = kulisa.text({
    parent = ui, x = 200, y = 866, font = "Inter", size = 34, wrap = 1520, color = "#f4f1ea", cps = 38,
})
local menu = kulisa.group({ parent = ui })

kulisa.screen.effects = { { shader = vignette, params = { 0.35, 0.85, 0, 0.55 } } }

-- Story presentation --------------------------------------------------------------------

local spinning = false
local choices = {}
-- After rollback or load the player has already read the line: show it at once.
local instant = false

-- Escapes text so it shows literally inside markup.
local function literal(s: string): string
    return (s:gsub("{", "{{"):gsub("}", "}}"))
end

local function clear_menu()
    for _, c in choices do
        c.card:remove()
        c.label:remove()
    end
    choices = {}
end

local function show_line(text: string, who)
    if who then
        name.text = `\{b}{literal(who.name or who.id)}\{/b}`
        name.color = who.color or "#ffffff"
    else
        name.text = ""
    end
    local ok, err = pcall(function()
        line.text = text
    end)
    if not ok then
        line.text = `\{color=#ff6a3d}Markup error:\{/color} {literal(tostring(err))}`
    end
    if instant then
        line:skip()
        instant = false
    end
end

kulisa.on("say", function(l)
    clear_menu()
    show_line(l.text, l.who)
end)

kulisa.on("menu", function(items, caption)
    clear_menu()
    if caption then
        show_line(caption.text, caption.who)
    else
        show_line("")
    end
    local top = 440 - #items * 46
    for i, item in items do
        local y = top + (i - 1) * 92
        local card = kulisa.sprite({
            parent = menu, x = 580, y = y, width = 760, height = 72, tint = "#111116", opacity = 0.88,
        })
        local label = kulisa.text({
            parent = menu, x = 610, y = y + 16, text = item.text, font = "Inter", size = 30, wrap = 700,
            align = "center", color = "#f4f1ea",
        })
        table.insert(choices, { index = item.index, card = card, label = label })
    end
end)

kulisa.on("pause", function()
    show_line("")
end)

kulisa.on("end", function()
    show_line("{i}The end.{/i}")
end)

local function set_scene(image)
    local backdrop = image and image[1] or "dusk"
    if backdrop == "night" then
        sky.tint = "#6f7fb8"
    else
        if backdrop ~= "dusk" then
            print(`no backdrop named {backdrop}`)
        end
        sky.tint = "#ffffff"
    end
    -- A new scene clears what was shown.
    logo.visible = false
    spinning = false
end

local function show(image)
    if image[1] == "logo" then
        logo.visible = true
        spinning = table.find(image, "spin") ~= nil
    else
        print(`no image named {image[1]}`)
    end
end

kulisa.on("stage", function(e)
    if e.kind == "scene" then
        set_scene(e.image)
    elseif e.kind == "show" then
        show(e.image)
    elseif e.kind == "hide" and e.tag == "logo" then
        logo.visible = false
        spinning = false
    end
end)

kulisa.on("restore", function(stage)
    clear_menu()
    set_scene(stage.scene)
    for _, s in stage.shown do
        show(s.image)
    end
    instant = true
end)

kulisa.on("error", function(message: string)
    clear_menu()
    name.text = "{b}Script error{/b}"
    name.color = "#ff6a3d"
    line.text = literal(message:split("\n")[1])
    line:skip()
end)

-- Input ---------------------------------------------------------------------------------

local function advance()
    if not line.complete then
        line:skip()
    else
        kulisa.story.next()
    end
end

-- Click, tap, space or enter: pick the choice under the pointer, or continue.
local function interact(x: number?, y: number?)
    kulisa.audio.play("sound", "audio/blip.wav", { volume = 0.6 })
    if x then
        for _, c in choices do
            if c.card:contains(x, y) then
                kulisa.story.choose(c.index)
                return
            end
        end
    end
    advance()
end

local function roll_forward()
    if not kulisa.story.roll_forward() then
        advance()
    end
end

local music_on = true

kulisa.on("press", function(x, y, button)
    if button == "left" or button == "touch" then
        interact(x, y)
    end
end)

kulisa.on("wheel", function(dy)
    if dy > 0 then
        kulisa.story.rollback()
    elseif dy < 0 then
        roll_forward()
    end
end)

kulisa.on("key", function(key)
    if key == "space" or key == "enter" then
        interact()
    elseif key == "left" or key == "pageup" then
        kulisa.story.rollback()
    elseif key == "right" or key == "pagedown" then
        roll_forward()
    elseif key == "f5" then
        kulisa.save.write("quick")
        print("quicksaved")
    elseif key == "f9" then
        if kulisa.save.exists("quick") then
            kulisa.save.load("quick")
            print("quickloaded")
        end
    elseif key == "m" then
        music_on = not music_on
        kulisa.audio.volume("music", if music_on then MUSIC_VOLUME else 0, 0.6)
    else
        local n = tonumber(key)
        if n and choices[n] then
            kulisa.story.choose(choices[n].index)
        end
    end
end)

-- Animation -----------------------------------------------------------------------------

local camera = kulisa.camera
local CENTER_X, CENTER_Y = 960, 540

kulisa.on("update", function(dt)
    local t = kulisa.now()
    logo.opacity = 0.85 + 0.15 * math.sin(t * 2)
    logo.rotation = if spinning then t * 2.5 else math.sin(t * 0.8) * 0.12
    mini.rotation = t

    -- The camera drifts toward the pointer and breathes.
    local px, py = kulisa.input.pointer()
    local aim_x, aim_y = CENTER_X, CENTER_Y
    if px then
        aim_x, aim_y = CENTER_X + (px - CENTER_X) * 0.25, CENTER_Y + (py - CENTER_Y) * 0.25
    end
    local k = 1 - math.exp(-4 * dt)
    camera.x += (aim_x - camera.x) * k
    camera.y += (aim_y - camera.y) * k
    camera.zoom = 1 + 0.03 * math.sin(t * 0.5)

    for _, c in choices do
        local hover = px ~= nil and c.card:contains(px, py)
        c.card.tint = if hover then "#3b2a2a" else "#111116"
    end
end)

-- Live reload (`kulisa run`): a script change restarts the game; go back to the same line.
kulisa.on("reload", function(slot)
    if slot then
        kulisa.save.load(slot)
    end
end)

kulisa.story.load()
kulisa.story.start()
