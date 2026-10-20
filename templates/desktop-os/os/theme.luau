-- Shared widget builders and sizes, so every app looks the same. Change the look here.

local config = require("os/config")

local ui = kulisa.ui
local c = config.colors
local theme = {}

-- Phones get bigger touch targets (and maximized windows, see os/wm).
theme.phone = kulisa.screen.profile == "phone"
theme.scale = if theme.phone then 1.45 else 1

function theme.size(n: number): number
    return math.floor(n * theme.scale + 0.5)
end

local S = theme.size

-- Insets of the visible screen (notches, system bars), as { top, right, bottom, left }.
function theme.insets()
    local vx, vy, vw, vh = kulisa.screen:visible()
    local sx, sy, sw, sh = kulisa.screen:safe()
    return { sy - vy, (vx + vw) - (sx + sw), (vy + vh) - (sy + sh), sx - vx }
end

-- Height of the taskbar (without the bottom inset).
function theme.taskbar_height(): number
    return S(60)
end

-- The part of the screen windows live in: the visible screen minus the taskbar and insets.
function theme.work_area(): (number, number, number, number)
    local vx, vy, vw, vh = kulisa.screen:visible()
    local i = theme.insets()
    return vx + i[4], vy + i[1], vw - i[2] - i[4], vh - i[1] - i[3] - theme.taskbar_height()
end

function theme.label(text: string, opts)
    opts = opts or {}
    opts.text = text
    opts.size = opts.size or S(20)
    opts.font = opts.font or config.font
    opts.color = opts.color or c.text
    return ui.text(opts)
end

function theme.bold(text: string, opts)
    return theme.label(`\{b}{text}\{/b}`, opts)
end

-- A white glyph from icons/ tinted with `color`.
function theme.glyph(name: string, size: number?, color: string?)
    local s = size or S(22)
    return ui.image({ image = `icons/{name}.png`, width = s, height = s, tint = color or c.text })
end

-- A square button showing a glyph, with a soft hover background.
function theme.icon_button(name: string, on_click, opts)
    opts = opts or {}
    local s = opts.size or S(36)
    return ui.button({
        on_click = on_click,
        id = opts.id,
        key = opts.key,
        disabled = opts.disabled,
        width = opts.width or s,
        height = opts.height or s,
        background = "gui/round.png",
        slice = 8,
        tint = opts.tint or "#00000000",
        hover_tint = opts.hover_tint or c.hover,
        pressed_tint = c.pressed,
        disabled_opacity = 0.35,
        cursor = "pointer",
        ui.box({ anchor = "center", theme.glyph(name, opts.glyph or S(20), opts.color) }),
    })
end

-- A rounded text button. `primary` fills it with the accent color.
function theme.button(text: string, on_click, opts)
    opts = opts or {}
    return ui.button({
        on_click = on_click,
        id = opts.id,
        key = opts.key,
        disabled = opts.disabled,
        selected = opts.selected,
        autofocus = opts.autofocus,
        width = opts.width,
        padding = opts.padding or { S(9), S(18) },
        background = "gui/round.png",
        slice = 8,
        tint = if opts.primary then c.accent else c.panel_hi,
        hover_tint = if opts.primary then "#a9b8ff" else "#323850",
        pressed_tint = if opts.primary then "#7186f0" else "#3b4260",
        selected_tint = c.accent,
        disabled_opacity = 0.4,
        cursor = "pointer",
        theme.label(text, {
            size = opts.size or S(19),
            color = if opts.primary then "#10131f" else c.text,
            selected_color = "#10131f",
            align = "center",
            width = if opts.width then "fill" else nil,
        }),
    })
end

-- A pill-shaped choice chip (chat replies, bookmarks).
function theme.chip(text: string, on_click, opts)
    opts = opts or {}
    return ui.button({
        on_click = on_click,
        key = opts.key,
        id = opts.id,
        autofocus = opts.autofocus,
        padding = opts.padding or { S(10), S(18) },
        background = "gui/pill.png",
        slice = 15,
        slice_scale = 0.8,
        tint = opts.tint or "#2f3650",
        hover_tint = opts.hover_tint or c.accent,
        pressed_tint = "#7186f0",
        cursor = "pointer",
        theme.label(text, { size = opts.size or S(19), color = opts.color or c.text, hover_color = "#10131f" }),
    })
end

-- A circular image (avatars are round already; this sizes them).
function theme.avatar(path: string, size: number)
    return ui.image({ image = path, width = size, height = size })
end

-- A thin divider line.
function theme.divider(vertical: boolean?)
    if vertical then
        return ui.box({ width = 1, height = "fill", background = c.line })
    end
    return ui.box({ width = "fill", height = 1, background = c.line })
end

-- A small round badge with a number (unread counts).
function theme.badge(n: number)
    return ui.box({
        min_width = S(22),
        height = S(22),
        padding = { 0, S(6) },
        background = "gui/pill.png",
        slice = 15,
        slice_scale = 0.7,
        tint = c.accent2,
        theme.label(`\{b}{n}\{/b}`, { size = S(13), color = "#1a0f16", anchor = "center" }),
    })
end

-- Sounds (audio/): played on the sound channel.
function theme.sound(name: string, volume: number?)
    kulisa.audio.play("sound", `audio/{name}.wav`, { volume = volume or 0.7 })
end

-- Time as the fake computer shows it ("23:41"). The story sets the clock with desk.clock; it
-- then runs at the real speed.
theme.clock = { minutes = 23 * 60 + 41, at = 0 }

function theme.time_now(): number
    return theme.clock.minutes + (kulisa.now() - theme.clock.at) / 60
end

function theme.time_text(minutes: number?): string
    local m = math.floor(minutes or theme.time_now()) % (24 * 60)
    return string.format("%02d:%02d", m // 60, m % 60)
end

return theme
