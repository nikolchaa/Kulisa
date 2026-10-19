-- State shared by the template's modules. Signals make the screens that read them rebuild.

local ui = kulisa.ui

local state = {
    -- The story is running (false on the main menu).
    started = false,
    -- "normal", "skip" or "auto".
    mode = ui.signal("normal"),
    -- The text box is up (window auto: down after `scene`, up again at the next line).
    window = ui.signal(false),
    -- The player hid the interface (H, middle click).
    hidden = ui.signal(false),
    -- The click-to-continue arrow.
    ctc = ui.signal(false),
    -- The open game menu section ("save", "load", "prefs", "history"), or nil.
    menu = ui.signal(nil),
    -- The current line: { who = Character or nil, text, seen, n }.
    line = nil,
    -- The current choices: { items, caption } or nil.
    choices = nil,
    -- Show the next line at once (after rollback or load).
    instant = false,
    -- The running transition handle, and whether one is running (the text box waits for it,
    -- as `with` does in Ren'Py).
    transition = nil,
    busy = ui.signal(false),
    window_after = false,
    -- The screen captured when the game menu opened (save thumbnails).
    shot = nil,
    -- Lines shown so far, for history.
    count = 0,
}

return state
