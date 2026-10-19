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
end
