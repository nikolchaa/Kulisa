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
