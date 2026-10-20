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
    local target = kulisa.target({ parent = parent, width = math.ceil(width), height = math.ceil(height), clear = "#00000000" })
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
    label(parent, 12, y + 82, "from the corner", 13, "#e8f0ff", 0.85)
    -- Chevrons showing the direction.
    for i = 0, 2 do
        label(parent, w + 6 + i * 20, y + h / 2 - 16, "›", 34, "#ffffff", 0.7 - 0.2 * i)
    end
end

-- A protective film for a glass panel, w x h face units, with its pull tab.
local function glass_film(parent, w: number, h: number, number: string, label_text: string, color: string)
    return make(parent, w, h, function(t)
        frame(t, w, h, 10, 1.5, "#9cc4ff", 0.5)
        tab(t, 0, 130, 100, number, "PULL", color)
        label(t, 0, h - 44, "{b}" .. label_text .. "{/b}  ·  remove before use", 14, "#cfe0ff", 0.55, w, "center")
        for i = 0, 9 do
            rect(t, w * 0.62 + i * 4 + (i % 2) * 2, h - 80, 2, 22, "#cfe0ff", 0.4)
        end
        for _, p in { { 16, 16 }, { w - 16, 16 }, { w - 16, h - 16 }, { 16, h - 16 } } do
            rect(t, p[1] - 6, p[2], 12, 1, "#cfe0ff", 0.6)
            rect(t, p[1], p[2] - 6, 1, 12, "#cfe0ff", 0.6)
        end
    end)
end

-- The film on the side glass.
function Prints.side(parent)
    local f = Geom.side
    return glass_film(parent, f.width, f.height, "1", "SIDE PANEL", "#2f6fed")
end

-- The film on the front glass (narrow: the tab is smaller).
function Prints.front(parent)
    local f = Geom.front
    local w, h = f.width, f.height
    return make(parent, w, h, function(t)
        frame(t, w, h, 10, 1.5, "#9cc4ff", 0.5)
        rect(t, 0, 0, 70, 110, "#e0663a", 0.92)
        rect(t, 67, 0, 3, 110, "#ffffff", 0.45)
        rect(t, 0, 107, 70, 3, "#ffffff", 0.45)
        label(t, 10, 8, "{b}2{/b}", 38, "#ffffff")
        label(t, 10, 58, "{b}PULL{/b}", 16, "#ffffff")
        for i = 0, 2 do
            label(t, 76 + i * 18, 40, "›", 30, "#ffffff", 0.7 - 0.2 * i)
        end
        label(t, 0, h - 44, "{b}FRONT PANEL{/b}", 14, "#cfe0ff", 0.55, w, "center")
    end)
end

-- A spare protector to fit on the side glass.
function Prints.protector(parent)
    local f = Geom.side
    local w, h = f.width, f.height
    return make(parent, w, h, function(t)
        frame(t, w, h, 10, 1.5, "#9cf0cf", 0.5)
        label(t, 0, h - 44, "{b}NEW PROTECTOR{/b}  ·  smooth out the air bubbles", 14, "#d7fff0", 0.6, w, "center")
        for _, p in { { 16, 16 }, { w - 16, 16 }, { w - 16, h - 16 }, { 16, h - 16 } } do
            rect(t, p[1] - 6, p[2], 12, 1, "#d7fff0", 0.6)
            rect(t, p[1], p[2] - 6, 1, 12, "#d7fff0", 0.6)
        end
    end)
end

-- The sticker over the I/O ports on top. Drawn upside down: the top face's b axis runs away from
-- the viewer, so the texture's top row is at the front edge.
function Prints.sticker(parent)
    local f = Geom.top
    local w, h = f.width, f.height
    return make(parent, w, h, function(t)
        rect(t, 0, 0, w, h, "#f4f4f6")
        rect(t, 0, 0, w, 4, "#f0b429")
        rect(t, 0, h - 4, w, 4, "#f0b429")
        label(t, 8, 3, "{b}I/O{/b}", 22, "#1b1d26")
        label(t, 52, 5, "{b}REMOVE{/b}", 11, "#1b1d26")
        label(t, 52, 19, "before use", 9, "#555a6b")
        for i = 0, 3 do
            rect(t, w - 30 + i * 5, 8, 2.5, h - 16, "#1b1d26", 0.8)
        end
    end, true)
end

-- The bag's two pieces, the wall round the case and the closed top (sizes in face units from
-- lib/bag.luau): printed warnings, a heat seal along the hem, fading into the milky plastic.
function Prints.bag(parent, sizes)
    local out = {}
    for name, size in sizes do
        local w, h = size[1], size[2]
        out[name] = make(parent, w, h, function(t)
            if name == "top" then
                -- The welded seam across the closed end.
                rect(t, 0, h / 2 - 5, w, 10, "#ffffff", 0.25)
                return
            end
            -- The hem: a heat seal strip near the open end, with a perforation line.
            rect(t, 0, h - 40, w, 12, "#ffffff", 0.22)
            for x = 10, w - 10, 20 do
                rect(t, x, h - 52, 8, 1.5, "#ffffff", 0.45)
            end
            local cols = math.max(1, math.floor(w / 240))
            for row = 0, 3 do
                for col = 0, cols - 1 do
                    local x = 30 + col * (w - 60) / cols + (row % 2) * 40
                    local y = 50 + row * 110
                    label(t, x, y, "{b}THIS SIDE UP{/b}", 18, "#ffffff", 0.35)
                    label(t, x, y + 24, "keep dry  ·  handle with care", 12, "#ffffff", 0.3)
                end
            end
            if name == "wall" then
                label(t, 30, h - 120, "{b}WARNING{/b}  plastic bags can cause suffocation, keep away from children", 13, "#ffffff", 0.45, w - 60)
            end
        end)
    end
    return out
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
