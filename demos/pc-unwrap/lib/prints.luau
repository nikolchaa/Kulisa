-- Printed layers of the foil, the films, the sticker and the bag. They are drawn once with
-- ordinary nodes into render targets, and the targets' textures are then the images of the meshes
-- (the shader composites them as the print). Rendering stops once the glyphs have been drawn.

local Geom = require("lib/geom")

local Prints = {}

local FONT: string

function Prints.init(font: string)
    FONT = font
end

local pending = {}

-- A transparent target of width x height, to be filled by `draw(parent)`. `flip` draws it upside
-- down, for faces whose v axis runs up the screen (the top).
local function make(parent, width: number, height: number, draw, flip: boolean?)
    local target = kulisa.target({ parent = parent, width = width, height = height, clear = "#00000000" })
    if flip then
        draw(kulisa.group({ parent = target, y = height, scale_y = -1 }))
    else
        draw(target)
    end
    table.insert(pending, { node = target, frames = 3 })
    return target.texture
end

local function rect(parent, x, y, w, h, color: string, alpha: number?)
    return kulisa.sprite({ parent = parent, x = x, y = y, width = w, height = h, tint = color, opacity = alpha or 1 })
end

local function label(parent, x, y, text: string, size: number, color: string, alpha: number?, wrap: number?, align: string?)
    return kulisa.text({
        parent = parent, x = x, y = y, text = text, size = size, font = FONT, color = color, opacity = alpha or 1,
        wrap = wrap, align = align or "left",
    })
end

local function frame(parent, w, h, inset, thickness, color, alpha)
    rect(parent, inset, inset, w - inset * 2, thickness, color, alpha)
    rect(parent, inset, h - inset - thickness, w - inset * 2, thickness, color, alpha)
    rect(parent, inset, inset, thickness, h - inset * 2, color, alpha)
    rect(parent, w - inset - thickness, inset, thickness, h - inset * 2, color, alpha)
end

-- The pull tab at the start of a foil: a colored patch in the middle of its left edge.
local function tab(parent, y: number, h: number, w: number, number: string, text: string, color: string)
    rect(parent, 0, y, w, h, color, 0.92)
    rect(parent, 0, y + h - 3, w, 3, "#ffffff", 0.45)
    rect(parent, w - 3, y, 3, h, "#ffffff", 0.45)
    label(parent, 12, y + 10, "{b}" .. number .. "{/b}", 38, "#ffffff")
    label(parent, 12, y + 58, "{b}" .. text .. "{/b}", 18, "#ffffff")
    label(parent, 12, y + 82, "pull across", 13, "#e8f0ff", 0.85)
    -- Chevrons showing the direction.
    for i = 0, 2 do
        label(parent, w + 6 + i * 20, y + h / 2 - 16, "›", 34, "#ffffff", 0.7 - 0.2 * i)
    end
end

-- The shrink-wrap foil around the glass: front glass, then the side glass. 1220 x 480.
function Prints.wrap(parent)
    local w, h = Geom.LEN + Geom.DEPTH, Geom.HEIGHT
    return make(parent, w, h, function(t)
        frame(t, w, h, 10, 1.5, "#9cc4ff", 0.5)
        tab(t, h / 2 - 75, 150, 110, "1", "PULL", "#2f6fed")
        label(t, 0, h - 44, "{b}PROTECTIVE FOIL{/b}  ·  remove before use", 14, "#cfe0ff", 0.55, Geom.LEN, "center")
        label(t, Geom.LEN, h - 44, "{b}SIDE PANEL{/b}  ·  peel off", 14, "#cfe0ff", 0.55, Geom.DEPTH, "center")
        for i = 0, 9 do
            rect(t, 560 + i * 4 + (i % 2) * 2, h - 80, 2, 22, "#cfe0ff", 0.4)
        end
        for _, p in { { 16, 16 }, { w - 16, 16 }, { w - 16, h - 16 }, { 16, h - 16 }, { Geom.LEN, 16 }, { Geom.LEN, h - 16 } } do
            rect(t, p[1] - 6, p[2], 12, 1, "#cfe0ff", 0.6)
            rect(t, p[1], p[2] - 6, 1, 12, "#cfe0ff", 0.6)
        end
    end)
end

-- The top film: 740 x 480 face pixels.
function Prints.top(parent)
    return make(parent, Geom.LEN, Geom.DEPTH, function(t)
        frame(t, Geom.LEN, Geom.DEPTH, 10, 1.5, "#9cc4ff", 0.55)
        tab(t, Geom.DEPTH / 2 - 75, 150, 110, "2", "PULL", "#e0663a")
        label(t, 0, Geom.DEPTH - 44, "{b}TOP PANEL FILM{/b}  ·  remove before use", 14, "#cfe0ff", 0.55, Geom.LEN, "center")
    end, true)
end

-- A spare protector to fit on the front glass: 740 x 480 face pixels.
function Prints.protector(parent)
    return make(parent, Geom.LEN, Geom.HEIGHT, function(t)
        frame(t, Geom.LEN, Geom.HEIGHT, 10, 1.5, "#9cf0cf", 0.5)
        label(t, 0, Geom.HEIGHT - 44, "{b}NEW PROTECTOR{/b}  ·  smooth out the air bubbles", 14, "#d7fff0", 0.6, Geom.LEN, "center")
        for _, p in { { 16, 16 }, { Geom.LEN - 16, 16 }, { Geom.LEN - 16, Geom.HEIGHT - 16 }, { 16, Geom.HEIGHT - 16 } } do
            rect(t, p[1] - 6, p[2], 12, 1, "#d7fff0", 0.6)
            rect(t, p[1], p[2] - 6, 1, 12, "#d7fff0", 0.6)
        end
    end)
end

-- The sticker over the I/O ports on top: 170 x 60 face pixels.
function Prints.sticker(parent)
    return make(parent, 170, 60, function(t)
        rect(t, 0, 0, 170, 60, "#f4f4f6")
        rect(t, 0, 0, 170, 6, "#f0b429")
        rect(t, 0, 54, 170, 6, "#f0b429")
        label(t, 10, 7, "{b}I/O{/b}", 38, "#1b1d26")
        label(t, 72, 11, "{b}REMOVE{/b}", 15, "#1b1d26")
        label(t, 72, 30, "before use", 12, "#555a6b")
        for i = 0, 3 do
            rect(t, 140 + i * 6, 13, 3, 32, "#1b1d26", 0.8)
        end
    end, true)
end

-- The bag: printed warnings, repeated, fading into the plastic.
function Prints.bag(parent, w: number, h: number)
    return make(parent, w, h, function(t)
        frame(t, w, h, 8, 2, "#ffffff", 0.35)
        -- A heat-sealed strip along the left (open) end, with a tear notch line.
        rect(t, 36, 0, 14, h, "#ffffff", 0.22)
        for y = 14, h - 14, 22 do
            rect(t, 24, y, 8, 1.5, "#ffffff", 0.5)
        end
        for row = 0, 4 do
            for col = 0, 2 do
                local x = 110 + col * 290 + (row % 2) * 90
                local y = 60 + row * 140
                label(t, x, y, "{b}THIS SIDE UP{/b}", 20, "#ffffff", 0.35)
                label(t, x, y + 26, "keep dry  ·  handle with care", 13, "#ffffff", 0.3)
            end
        end
        label(t, w - 420, h - 90, "{b}WARNING{/b}  plastic bag can cause suffocation, keep away from children", 14, "#ffffff", 0.45, 380)
    end)
end

-- Called every frame: stops rendering targets once their contents have settled.
function Prints.tick()
    for i = #pending, 1, -1 do
        local p = pending[i]
        p.frames -= 1
        if p.frames <= 0 then
            p.node.visible = false
            table.remove(pending, i)
        end
    end
end

return Prints
