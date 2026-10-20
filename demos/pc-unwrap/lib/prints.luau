-- Printed layers of the films, the sticker and the bag. They are drawn once with ordinary nodes
-- into render targets, and the targets' textures are then the images of the film meshes (the
-- shader composites them as the print). Rendering stops once the glyphs have been drawn.

local Prints = {}

local FONT: string

function Prints.init(font: string)
    FONT = font
end

local pending = {}

-- A transparent target of width x height, to be filled by `draw(parent)`.
local function make(parent, width: number, height: number, draw, flip: boolean?)
    local target = kulisa.target({ parent = parent, width = width, height = height, clear = "#00000000" })
    if flip then
        -- On the top face, v runs away from the viewer (up the screen): draw the print upside down.
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

-- The tab you grab: a colored corner patch with a step number and a hint.
local function tab(parent, w, h, number: string, text: string, color: string, flip_x: boolean?)
    rect(parent, 0, 0, w, h, color, 0.92)
    rect(parent, 0, h - 3, w, 3, "#ffffff", 0.5)
    rect(parent, w - 3, 0, 3, h, "#ffffff", 0.5)
    label(parent, 12, 6, "{b}" .. number .. "{/b}", 34, "#ffffff")
    label(parent, 46, 16, "{b}" .. text .. "{/b}", 17, "#ffffff")
    label(parent, 46, 38, "peel diagonally", 12, "#e8f0ff", 0.85)
end

-- The glass film: 760 x 440 face pixels.
function Prints.glass(parent)
    return make(parent, 760, 440, function(t)
        frame(t, 760, 440, 10, 1.5, "#9cc4ff", 0.55)
        tab(t, 110, 70, "1", "PEEL", "#2f6fed")
        label(t, 0, 392, "{b}PROTECTIVE FILM{/b}  ·  remove before use", 14, "#cfe0ff", 0.55, 760, "center")
        for i = 0, 9 do
            rect(t, 560 + i * 4 + (i % 2) * 2, 400, 2, 22, "#cfe0ff", 0.4)
        end
        -- Tiny registration marks in the corners.
        for _, p in { { 744, 14 }, { 744, 426 }, { 16, 426 } } do
            rect(t, p[1] - 6, p[2], 12, 1, "#cfe0ff", 0.6)
            rect(t, p[1], p[2] - 6, 1, 12, "#cfe0ff", 0.6)
        end
    end)
end

-- The top film: 760 x 380 face pixels.
function Prints.top(parent)
    return make(parent, 760, 380, function(t)
        frame(t, 760, 380, 10, 1.5, "#9cc4ff", 0.55)
        tab(t, 110, 70, "2", "PEEL", "#e0663a")
        label(t, 0, 330, "{b}TOP PANEL FILM{/b}  ·  remove before use", 14, "#cfe0ff", 0.55, 760, "center")
    end, true)
end

-- A spare protector to fit on the glass: 760 x 440 face pixels.
function Prints.protector(parent)
    return make(parent, 760, 440, function(t)
        frame(t, 760, 440, 10, 1.5, "#9cf0cf", 0.5)
        label(t, 0, 386, "{b}NEW PROTECTOR{/b}  ·  smooth out the air bubbles", 14, "#d7fff0", 0.6, 760, "center")
        for _, p in { { 16, 16 }, { 744, 16 }, { 744, 424 }, { 16, 424 } } do
            rect(t, p[1] - 6, p[2], 12, 1, "#d7fff0", 0.6)
            rect(t, p[1], p[2] - 6, 1, 12, "#d7fff0", 0.6)
        end
    end)
end

-- The sticker over the front I/O ports: 190 x 64 face pixels.
function Prints.sticker(parent)
    return make(parent, 190, 64, function(t)
        rect(t, 0, 0, 190, 64, "#f4f4f6")
        rect(t, 0, 0, 190, 6, "#f0b429")
        rect(t, 0, 58, 190, 6, "#f0b429")
        label(t, 12, 8, "{b}I/O{/b}", 40, "#1b1d26")
        label(t, 82, 12, "{b}REMOVE{/b}", 16, "#1b1d26")
        label(t, 82, 33, "before use", 13, "#555a6b")
        for i = 0, 3 do
            rect(t, 156 + i * 7, 14, 4, 34, "#1b1d26", 0.8)
        end
    end, true)
end

-- The bag: printed warnings, repeated, fading into the plastic. 1000 x 640.
function Prints.bag(parent)
    return make(parent, 1000, 640, function(t)
        frame(t, 1000, 640, 8, 2, "#ffffff", 0.35)
        -- A heat-sealed strip along the left (open) end, with a tear notch line.
        rect(t, 36, 0, 14, 640, "#ffffff", 0.22)
        for y = 14, 626, 22 do
            rect(t, 24, y, 8, 1.5, "#ffffff", 0.5)
        end
        for row = 0, 3 do
            for col = 0, 2 do
                local x = 110 + col * 300 + (row % 2) * 90
                local y = 60 + row * 150
                label(t, x, y, "{b}THIS SIDE UP{/b}", 20, "#ffffff", 0.35)
                label(t, x, y + 26, "keep dry  ·  handle with care", 13, "#ffffff", 0.3)
            end
        end
        label(t, 600, 560, "{b}WARNING{/b}  plastic bag can cause suffocation, keep away from children", 14, "#ffffff", 0.45, 380)
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
