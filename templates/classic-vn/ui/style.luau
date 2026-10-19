-- Shared widget builders, so every screen looks the same. Change the look here.

local config = require("game/config")

local ui = kulisa.ui
local c = config.colors
local style = {}

-- Bigger touch targets on phones.
style.phone = kulisa.screen.profile == "phone"
style.scale = if style.phone then 1.35 else 1

function style.size(n: number): number
    return math.floor(n * style.scale + 0.5)
end

-- Padding for full-screen menus: `base` on every side plus the safe-area insets (notches,
-- system bars), as { top, right, bottom, left }.
function style.safe_padding(base: number)
    local vx, vy, vw, vh = kulisa.screen:visible()
    local sx, sy, sw, sh = kulisa.screen:safe()
    return {
        base + (sy - vy),
        base + (vx + vw) - (sx + sw),
        base + (vy + vh) - (sy + sh),
        base + (sx - vx),
    }
end

-- Full-screen screens use the insets: rebuild them when the window changes.
kulisa.on("resize", function()
    kulisa.ui.refresh()
end)

-- A text label.
function style.label(text: string, opts)
    opts = opts or {}
    opts.text = text
    opts.size = opts.size or style.size(30)
    opts.font = opts.font or config.font
    opts.color = opts.color or c.paper
    return ui.text(opts)
end

-- A text button: idle in `idle`, hover in the accent color, selected in paper.
function style.button(text: string, on_click, opts)
    opts = opts or {}
    local b = {
        on_click = on_click,
        selected = opts.selected,
        disabled = opts.disabled,
        padding = opts.padding or { style.size(6), style.size(10) },
        width = opts.width,
        anchor = opts.anchor,
        key = opts.key,
        id = opts.id,
        autofocus = opts.autofocus,
        ui.text({
            text = text,
            size = opts.size or style.size(30),
            font = config.font,
            align = opts.align,
            width = if opts.width then "fill" else nil,
            color = c.idle,
            hover_color = c.accent,
            selected_color = c.paper,
            disabled_color = c.disabled,
        }),
    }
    return ui.button(b)
end

-- A toggle or option: a small box that fills with the accent color when selected.
function style.option(text: string, selected: boolean, on_click)
    local box = style.size(18)
    return ui.button({
        on_click = on_click,
        selected = selected,
        padding = { style.size(6), style.size(10) },
        ui.row({
            gap = style.size(12),
            align = "center",
            ui.box({
                width = box,
                height = box,
                background = "gui/round.png",
                slice = 16,
                slice_scale = 0.4,
                tint = c.disabled,
                hover_tint = c.haze,
                selected_tint = c.accent,
            }),
            ui.text({
                text = text,
                size = style.size(26),
                font = config.font,
                color = c.idle,
                hover_color = c.accent,
                selected_color = c.paper,
            }),
        }),
    })
end

-- A boxed button (choices, save slots).
function style.card(on_click, opts, children)
    opts = opts or {}
    local b = {
        on_click = on_click,
        selected = opts.selected,
        disabled = opts.disabled,
        width = opts.width,
        height = opts.height,
        padding = opts.padding or style.size(14),
        anchor = opts.anchor,
        key = opts.key,
        id = opts.id,
        autofocus = opts.autofocus,
        background = "gui/round.png",
        slice = 16,
        tint = opts.tint or "#1d1c25e6",
        hover_tint = opts.hover_tint or "#3a2a2af0",
        pressed_tint = "#552f22f0",
        disabled_opacity = 0.5,
    }
    for _, child in children do
        table.insert(b, child)
    end
    return ui.button(b)
end

-- A slider with the template's track, bar and knob.
function style.slider(value: number, min: number, max: number, on_change, opts)
    opts = opts or {}
    return ui.slider({
        value = value,
        min = min,
        max = max,
        step = opts.step,
        on_change = on_change,
        width = opts.width or style.size(420),
        height = style.size(28),
        thumb_size = style.size(28),
        background = "gui/round.png",
        slice = 14,
        slice_scale = 0.5,
        tint = "#2c2b35",
        bar = { image = "gui/round.png", tint = config.colors.accent },
        thumb = { image = "gui/knob.png", tint = config.colors.paper },
        hover_thumb = { image = "gui/knob.png", tint = "#ffffff" },
        hover_tint = "#3a3945",
    })
end

-- A section heading in the game menu.
function style.heading(text: string)
    return style.label(`\{b}{text}\{/b}`, { size = style.size(26), color = c.accent })
end

return style
