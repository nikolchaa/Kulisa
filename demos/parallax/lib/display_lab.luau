-- The display lab: shows how one scene meets every screen. Outlines mark the design rectangle
-- (gold), what the window shows (cyan) and the safe area (green); a card lists the profile,
-- design size, scale mode, aspect limits and window size; anchored badges sit in the safe area's
-- corners. The scale mode and the aspect clamp switch at runtime (kulisa.display may be called
-- again), and on desktop the window can be reshaped to common screen shapes.

local Lab = {}

Lab.config = {
    width = 1920,
    height = 1080,
    scale = "expand",
    aspect = { "4:3", "21:9" },
    orientation = "any",
    phone = { width = 1080, height = 1920, orientation = "portrait", aspect = { "9:22", "3:4" } },
}
Lab.clamp = true
Lab.overlay = false
-- Called after the display settings change (the scene lays itself out again).
Lab.on_change = function() end

-- Window shapes for desktops (logical pixels).
Lab.shapes = {
    { name = "16:9", w = 1280, h = 720 },
    { name = "21:9", w = 1680, h = 720 },
    { name = "32:9", w = 1920, h = 540 },
    { name = "4:3", w = 1024, h = 768 },
    { name = "9:16", w = 450, h = 800 },
}

function Lab.apply()
    local c = table.clone(Lab.config)
    c.phone = table.clone(Lab.config.phone)
    if not Lab.clamp then
        c.aspect, c.phone.aspect = false, false
    end
    kulisa.display(c)
    Lab.on_change()
end

local layer = kulisa.layer({ parallax = 0, z = 900 })
local lines = {}

local function outline(key: string, color: string, x: number, y: number, w: number, h: number, t: number)
    local l = lines[key]
    if not l then
        l = {}
        for i = 1, 4 do
            l[i] = kulisa.sprite({ parent = layer, tint = color })
        end
        lines[key] = l
    end
    local parts = { { x, y, w, t }, { x, y + h - t, w, t }, { x, y, t, h }, { x + w - t, y, t, h } }
    for i, p in parts do
        l[i].x, l[i].y, l[i].width, l[i].height = p[1], p[2], p[3], p[4]
    end
end

function Lab.update()
    layer.visible = Lab.overlay
    if not Lab.overlay then
        return
    end
    local vx, vy, vw, vh = kulisa.screen:visible()
    local sx, sy, sw, sh = kulisa.screen:safe()
    local t = 2 / kulisa.screen.scale
    outline("design", "#ffc857", 0, 0, kulisa.screen.width, kulisa.screen.height, t * 2)
    outline("visible", "#4fd6ff", vx, vy, vw, vh, t * 2)
    outline("safe", "#68e08a", sx, sy, sw, sh, t)
end

-- Text for the info card.
function Lab.describe(): string
    local s = kulisa.screen
    local vx, vy, vw, vh = s:visible()
    local range = if s.profile == "phone" then "9:22 to 3:4" else "4:3 to 21:9"
    local aspect = if Lab.clamp then `{range} (bars beyond)` else "off"
    return table.concat({
        `\{b}Profile\{/b}  {s.profile}   \{b}Orientation\{/b}  {s.orientation}`,
        `\{b}Design\{/b}  {s.width} × {s.height}   \{b}Mode\{/b}  {s.mode}`,
        `\{b}Aspect clamp\{/b}  {aspect}`,
        string.format("{b}Visible{/b}  %.0f × %.0f at (%.0f, %.0f)   {b}Scale{/b}  %.2f px/unit", vw, vh, vx, vy, s.scale),
    }, "\n")
end

kulisa.on("resize", Lab.update)
kulisa.on("display", Lab.update)

return Lab
