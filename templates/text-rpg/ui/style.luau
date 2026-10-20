-- Shared widget builders: ornamented panels, gold buttons, bars. Change the look here.

local config = require("game/config")

local ui = kulisa.ui
local c = config.colors
local style = {}

style.phone = kulisa.screen.profile == "phone"
-- Phones lay out a 1080-wide portrait design: text a little larger relative to it.
style.scale = if style.phone then 1.15 else 1

function style.size(n: number): number
    return math.floor(n * style.scale + 0.5)
end
local S = style.size

function style.label(text: string, opts)
    opts = opts or {}
    opts.text = text
    opts.size = opts.size or S(24)
    opts.font = opts.font or config.font
    opts.color = opts.color or c.text
    return ui.text(opts)
end

function style.heading(text: string, opts)
    opts = opts or {}
    opts.color = opts.color or c.gold
    opts.size = opts.size or S(20)
    return style.label(`\{b}{string.upper(text)}\{/b}`, opts)
end

-- A dark panel with a gold border and corner ornaments.
function style.panel(props)
    props.background = "gui/panel.png"
    props.slice = 24
    props.padding = props.padding or S(26)
    return ui.box(props)
end

function style.divider()
    return ui.image({ image = "gui/divider.png", width = "fill", height = S(16), tint = "#ffffffcc" })
end

-- A text button in gold on dark leather.
function style.button(text: string, on_click, opts)
    opts = opts or {}
    return ui.button({
        on_click = on_click,
        id = opts.id,
        key = opts.key,
        width = opts.width,
        disabled = opts.disabled,
        autofocus = opts.autofocus,
        padding = opts.padding or { S(10), S(20) },
        background = "gui/choice.png",
        slice = 14,
        slice_scale = 0.8,
        tint = "#2b201a",
        hover_tint = "#4a3524",
        pressed_tint = "#5c4128",
        disabled_opacity = 0.4,
        cursor = "pointer",
        style.label(text, {
            size = opts.size or S(22),
            color = c.gold,
            hover_color = c.gold_hi,
            align = opts.align or "center",
            width = if opts.width then "fill" else nil,
        }),
    })
end

-- A labelled bar (health): `value` of `max`, in `color`.
function style.bar(label: string, value: number, max: number, color: string, opts)
    opts = opts or {}
    local f = if max > 0 then math.clamp(value / max, 0, 1) else 0
    return ui.column({
        width = "fill",
        gap = S(6),
        ui.row({
            width = "fill",
            style.label(label, { size = S(19), color = c.dim, width = "fill" }),
            style.label(`\{b}{value}\{/b} / {max}`, { size = S(19) }),
        }),
        ui.box({
            width = "fill",
            height = opts.height or S(16),
            background = "gui/bar.png",
            slice = 8,
            tint = "#0a0706",
            f > 0 and ui.box({
                width = string.format("%.2f%%", f * 100),
                height = "fill",
                background = "gui/bar.png",
                slice = 8,
                tint = color,
            }),
        }),
    })
end

-- A small stat pill: label above value.
function style.stat(label: string, value: number)
    return ui.column({
        width = "fill",
        align = "center",
        padding = S(8),
        background = "gui/round.png",
        slice = 8,
        tint = "#00000055",
        style.label(`\{b}{value}\{/b}`, { size = S(30), color = c.paper }),
        style.label(label, { size = S(16), color = c.dim }),
    })
end

return style
