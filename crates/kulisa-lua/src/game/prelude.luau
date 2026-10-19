-- Kulisa Luau prelude: coroutine scheduling. Runs before the sandbox is enabled, so it can
-- still add fields to the `kulisa` table.

local tasks = {}

local function resume(co, ...)
    local ok, err = coroutine.resume(co, ...)
    if not ok then
        err = debug.traceback(co, tostring(err))
        kulisa._report(err)
    end
    return ok, err
end

-- Runs `f(...)` as a task that may `kulisa.wait` and `kulisa.await`. Returns whether its
-- first step succeeded, and the error (with a traceback) if not.
function kulisa.spawn(f, ...)
    local co = coroutine.create(f)
    local ok, err = resume(co, ...)
    if ok and coroutine.status(co) ~= "dead" then
        table.insert(tasks, co)
    end
    return ok, if ok then nil else err
end

-- Waits `secs` seconds (0 = until the next frame). Only inside a task.
function kulisa.wait(secs)
    local until_ = kulisa.now() + (secs or 0)
    repeat
        coroutine.yield()
    until kulisa.now() >= until_
end

-- Waits until `handle.done` (tweens) or `fn()` returns true. Only inside a task.
function kulisa.await(handle)
    if type(handle) == "function" then
        while not handle() do
            coroutine.yield()
        end
    else
        while not handle.done do
            coroutine.yield()
        end
    end
end

-- Timers: run `f` once after `secs` (kulisa.after) or every `secs` (kulisa.every). The handle
-- has `cancel()` and `done` (true once cancelled, or after an `after` fired), so tasks can
-- `kulisa.await` it. Callbacks run once per frame at most; time is game time.
local timers = {}

local function timer(secs, f, repeating)
    if type(secs) ~= "number" or secs < 0 then
        error("the delay must be a number of seconds, 0 or more", 3)
    end
    if type(f) ~= "function" then
        error("the second argument must be a function", 3)
    end
    local t = { due = kulisa.now() + secs, secs = secs, f = f, repeating = repeating }
    local handle = { done = false }
    function handle.cancel()
        handle.done = true
    end
    t.handle = handle
    table.insert(timers, t)
    return handle
end

function kulisa.after(secs, f)
    return timer(secs, f, false)
end

function kulisa.every(secs, f)
    return timer(math.max(secs, 0.001), f, true)
end

local function run_timers()
    local now = kulisa.now()
    local i = 1
    while i <= #timers do
        local t = timers[i]
        if not t.handle.done and now >= t.due then
            if t.repeating then
                repeat t.due += t.secs until t.due > now
            else
                t.handle.done = true
            end
            local ok, err = xpcall(t.f, debug.traceback)
            if not ok then
                kulisa._report(tostring(err))
            end
        end
        if t.handle.done then
            table.remove(timers, i)
        else
            i += 1
        end
    end
end

-- Called by the engine once per frame.
function kulisa._tick(dt)
    local i = 1
    while i <= #tasks do
        local co = tasks[i]
        if coroutine.status(co) == "suspended" then
            resume(co)
        end
        if coroutine.status(co) == "dead" then
            table.remove(tasks, i)
        else
            i += 1
        end
    end
    run_timers()
end

-- UI widget constructors: plain tables tagged with their kind. Children go in the list part.
do
    local ui = kulisa.ui
    local function kind(k)
        return function(t)
            t = t or {}
            t._kind = k
            return t
        end
    end
    for _, k in { "box", "row", "column", "grid", "image", "button", "slider", "scroll", "node" } do
        ui[k] = kind(k)
    end
    -- ui.text("Hello", { size = 30 }) or ui.text{ text = "Hello", size = 30 }
    function ui.text(t, opts)
        if type(t) ~= "table" then
            opts = opts or {}
            opts.text = t
            t = opts
        end
        t._kind = "text"
        return t
    end
    -- Empty space that takes what is left in a row or column.
    function ui.spacer(t)
        t = t or {}
        t._kind = "box"
        t.width = t.width or "fill"
        t.height = t.height or "fill"
        return t
    end
end

-- Text fields: ui.input{ id, value = signal, placeholder, max_length, password, on_change,
-- on_submit, ...button and text look properties }. Clicking (or Enter / A on) a field edits
-- it: typing inserts at the caret, Backspace / Delete / arrows / Home / End edit, Enter
-- submits, Escape stops. `value` is a signal that always holds the text. Built on
-- kulisa.input.start_text and the `text` / `key` events, so games can write their own.
do
    local ui = kulisa.ui
    local active -- { id, value, opts } for the field being edited
    local caret = 0 -- insert position, in characters
    local active_id = ui.signal(nil)
    local blink = ui.signal(true)
    local revision = ui.signal(0)
    local blinker

    local function chars(s: string): number
        return utf8.len(s) or #s
    end

    -- The byte offset after `n` characters.
    local function offset(s: string, n: number): number
        if n <= 0 then
            return 0
        end
        return (utf8.offset(s, n + 1) or #s + 1) - 1
    end

    local function touch()
        revision:set(revision:peek() + 1)
        blink:set(true)
    end

    local function stop()
        if active then
            active = nil
            active_id:set(nil)
            if blinker then
                blinker.cancel()
                blinker = nil
            end
            kulisa.input.stop_text()
        end
    end

    local function start(id: string, value, opts)
        stop()
        active = { id = id, value = value, opts = opts }
        caret = chars(value:peek() or "")
        active_id:set(id)
        kulisa.input.start_text()
        blinker = kulisa.every(0.5, function()
            blink:set(not blink:peek())
        end)
        touch()
    end

    local function replace(text: string)
        active.value:set(text)
        if active.opts.on_change then
            active.opts.on_change(text)
        end
        touch()
    end

    kulisa.on("text", function(s: string)
        if not active then
            return
        end
        local text = active.value:peek() or ""
        local max = active.opts.max_length
        if max and chars(text) + chars(s) > max then
            s = s:sub(1, offset(s, max - chars(text)))
            if s == "" then
                return
            end
        end
        local at = offset(text, caret)
        caret += chars(s)
        replace(text:sub(1, at) .. s .. text:sub(at + 1))
    end)

    kulisa.on("key", function(key: string)
        if not active then
            return
        end
        local text = active.value:peek() or ""
        if key == "backspace" and caret > 0 then
            local from, to = offset(text, caret - 1), offset(text, caret)
            caret -= 1
            replace(text:sub(1, from) .. text:sub(to + 1))
        elseif key == "delete" and caret < chars(text) then
            local from, to = offset(text, caret), offset(text, caret + 1)
            replace(text:sub(1, from) .. text:sub(to + 1))
        elseif key == "left" then
            caret = math.max(caret - 1, 0)
            touch()
        elseif key == "right" then
            caret = math.min(caret + 1, chars(text))
            touch()
        elseif key == "home" then
            caret = 0
            touch()
        elseif key == "end" then
            caret = chars(text)
            touch()
        elseif key == "enter" or key == "escape" then
            local submit = active.opts.on_submit
            -- Stop after this frame's handlers, so the game's own Enter / Escape handlers
            -- still see that a field was being edited.
            local field = active
            kulisa.after(0, function()
                if active == field then
                    stop()
                end
            end)
            if key == "enter" and submit then
                submit(text)
            end
        end
    end)

    local function escape(s: string): string
        return (s:gsub("{", "{{"):gsub("}", "}}"))
    end

    local CONTROL = { value = true, placeholder = true, max_length = true, password = true, on_change = true,
        on_submit = true, placeholder_color = true, size = true, font = true, color = true, align = true }

    function ui.input(opts)
        opts = opts or {}
        local id = opts.id
        if type(id) ~= "string" then
            error("ui.input needs an `id` string, so it can tell which field is being edited", 2)
        end
        local value = opts.value
        if type(value) ~= "table" and type(value) ~= "userdata" then
            error("ui.input needs `value = signal`", 2)
        end
        local text = value() or ""
        local editing = active_id() == id
        revision() -- rebuild on caret moves
        local shown
        if text == "" and not editing then
            shown = escape(opts.placeholder or "")
        else
            local masked = if opts.password then string.rep("*", chars(text)) else text
            if editing then
                local at = offset(masked, caret)
                local mark = if blink() then "|" else " "
                shown = escape(masked:sub(1, at)) .. mark .. escape(masked:sub(at + 1))
            else
                shown = escape(masked)
            end
        end
        local field = {
            _kind = "button",
            id = id,
            on_click = function()
                start(id, value, opts)
            end,
            selected = editing,
        }
        for k, v in opts do
            if not CONTROL[k] then
                field[k] = v
            end
        end
        local dim = text == "" and not editing
        table.insert(field, ui.text{
            text = shown,
            size = opts.size,
            font = opts.font,
            align = opts.align,
            width = "fill",
            color = if dim then opts.placeholder_color or "#8a8a96" else opts.color,
        })
        return field
    end
end
