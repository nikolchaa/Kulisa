-- The story bridge: the .kulisa story drives the computer and the computer drives the story.
--
-- Characters with `chat = "thread"` talk in the chat app: they "type" for a moment, the message
-- arrives (a notification if the player is not looking), and the story goes on once the player
-- has seen it. `me = true` characters send messages in the current thread. Everyone else, and
-- narration, shows as a caption the player clicks through. A `menu` right after chat messages
-- becomes reply chips in that thread; other menus are dialogs. `$ desk.wait_visit(url, hint)`
-- followed by `pause` holds the story until the player opens that page in the browser.
--
-- The `desk` table is global so story `$` lines can call it (see README.md).

local config = require("os/config")
local data = require("os/data")
local shell = require("os/shell")
local theme = require("os/theme")
local wm = require("os/wm")

local ui = kulisa.ui

local bridge = {}
bridge.thread = nil -- the thread the conversation is in
bridge.typing = ui.signal(nil) -- id of the thread whose contact is typing
bridge.replies = ui.signal(nil) -- { thread, items } while a menu waits in a thread
bridge.started = false
-- What the story waits for: { kind = "typing" | "read" | "delay" | "caption" | "visit" | "open", ... }
local wait = nil
local pending = nil -- set by desk.wait_* for the next `pause`
local last_chat = false -- the last line was a chat message (menus become replies)
local restoring = false

-- Set by apps/chat: whether the player is looking at a thread.
bridge.viewing = function(_id: string): boolean
    return false
end

local function go()
    wait = nil
    if bridge.started and not kulisa.story.ended() then
        kulisa.story.next()
    end
end

local function delay_for(text: string?): number
    return config.read_delay.base + (if text then #text else 0) * config.read_delay.per_char
end

local function deliver(id: string, msg)
    local t = data.post(id, msg)
    theme.sound("pop", 0.6)
    if bridge.viewing(id) then
        return true
    end
    t.unread += 1
    data.changed()
    shell.notify({
        title = t.name,
        text = msg.text or "Sent a photo",
        icon = t.avatar,
        silent = true,
        on_click = function()
            bridge.open_thread(id)
        end,
    })
    theme.sound("notify", 0.45)
    return false
end

-- Opens the chat app on a thread (notifications, desk.open).
bridge.open_thread = function(_id: string) end

kulisa.on("say", function(line)
    local who = line.who
    local text = line.text
    if who and (who.chat or who.me) then
        local id = who.chat or bridge.thread or "chat"
        bridge.thread = id
        last_chat = true
        local t = data.thread(id, if who.me then nil else who)
        local last = t.messages[#t.messages]
        local from = if who.me then "me" else "them"
        if restoring and last and last.text == text and last.from == from then
            -- After a load the current line comes again; it is already in the thread.
            restoring = false
            wait = { kind = "read", id = id, text = text }
            return
        end
        restoring = false
        if who.me then
            data.post(id, { from = "me", text = text, time = theme.time_text() })
            theme.sound("send", 0.5)
            wait = { kind = "delay", at = kulisa.now() + 0.6 }
        else
            local d = math.clamp(#text * config.typing.per_char, config.typing.min, config.typing.max)
            bridge.typing:set(id)
            wait = { kind = "typing", id = id, text = text, at = kulisa.now() + d }
        end
    else
        restoring = false
        last_chat = false
        shell.caption(text, who and who.name)
        wait = { kind = "caption" }
    end
end)

shell.on_caption_done = function()
    if wait and wait.kind == "caption" then
        go()
    end
end

function bridge.reply(index: number)
    bridge.replies:set(nil)
    kulisa.story.choose(index)
end

kulisa.on("menu", function(items, caption)
    if caption then
        shell.caption(caption.text, caption.who and caption.who.name)
    end
    if last_chat and bridge.thread then
        bridge.replies:set({ thread = bridge.thread, items = items })
        data.changed()
        return
    end
    local buttons = {}
    for _, it in items do
        table.insert(buttons, { it.text, function()
            shell.caption(nil)
            kulisa.story.choose(it.index)
        end })
    end
    shell.dialog({ title = "Choose", buttons = buttons })
end)

kulisa.on("pause", function(secs)
    if secs then
        wait = { kind = "delay", at = kulisa.now() + secs }
    elseif pending then
        wait = pending
        pending = nil
    else
        wait = { kind = "delay", at = kulisa.now() }
    end
end)

kulisa.on("update", function()
    if wait == nil then
        return
    end
    local now = kulisa.now()
    if wait.kind == "typing" and now >= wait.at then
        bridge.typing:set(nil)
        local seen = deliver(wait.id, { from = "them", text = wait.text, time = theme.time_text() })
        if seen then
            wait = { kind = "delay", at = now + delay_for(wait.text) }
        else
            wait = { kind = "read", id = wait.id, text = wait.text }
        end
    elseif wait.kind == "read" and bridge.viewing(wait.id) then
        wait = { kind = "delay", at = now + delay_for(wait.text) }
    elseif wait.kind == "delay" and now >= wait.at then
        go()
    elseif wait.kind == "open" and wm.is_focused(wait.app) then
        shell.hint(nil)
        wait = { kind = "delay", at = now + 0.6 }
    end
end)

-- The browser reports every page it shows.
function bridge.visited(url: string)
    data.visited[url] = true
    if wait and wait.kind == "visit" and url == wait.url then
        shell.hint(nil)
        wait = { kind = "delay", at = kulisa.now() + 1.2 }
    end
end

-- Space or Enter: finish a caption, or skip a short wait the player is looking at.
function bridge.advance(): boolean
    if shell.caption_shown() then
        shell.advance_caption()
        return true
    end
    if wait and wait.kind == "delay" then
        go()
        return true
    end
    return false
end

kulisa.on("end", function()
    bridge.started = false
    kulisa.after(1.5, function()
        shell.dialog({
            title = "The end",
            icon = "apps/start.png",
            text = "That is the whole sample story. Edit story/script.kulisa to write yours.",
            buttons = { { "Keep exploring" }, { "Start over", bridge.restart, true } },
        })
    end)
end)

kulisa.on("restore", function()
    -- A load: windows and data come back with the registered state (main.luau).
    wait = nil
    pending = nil
    restoring = true
    bridge.typing:set(nil)
    bridge.replies:set(nil)
    shell.caption(nil)
    shell.hint(nil)
end)

function bridge.start()
    bridge.started = true
    wait = nil
    kulisa.story.start()
end

-- main.luau sets this: reset everything and start again.
bridge.restart = function() end

-- The story API ------------------------------------------------------------------------------

desk = {}

-- Opens an app; the browser takes a URL, the chat app a thread id.
function desk.open(app: string, arg)
    wm.open(app, arg)
end

-- A notification: desk.notify(title, text, app?).
function desk.notify(title: string, text: string, app: string?)
    shell.notify({
        title = title,
        text = text,
        app = app,
        on_click = app and function()
            wm.open(app)
        end,
    })
end

-- Holds the next `pause` until the player opens `url` in the browser; shows `hint` meanwhile.
function desk.wait_visit(url: string, hint: string?)
    pending = { kind = "visit", url = url }
    if hint then
        shell.hint(hint)
    end
end

-- Holds the next `pause` until the player focuses an app's window.
function desk.wait_open(app: string, hint: string?)
    pending = { kind = "open", app = app }
    if hint then
        shell.hint(hint)
    end
end

-- Shows (or with nil, hides) the objective hint at the top of the screen.
function desk.hint(text: string?)
    shell.hint(text)
end

-- Adds a note to the Notes app.
function desk.note(title: string, text: string)
    table.insert(data.notes, 1, { title = title, text = text, time = theme.time_text() })
    data.changed()
    shell.notify({
        title = "Notes",
        text = `New note: {title}`,
        app = "notes",
        on_click = function()
            wm.open("notes")
        end,
    })
end

-- A photo message from a thread's contact (images are paths in the game).
function desk.photo(thread: string, image: string)
    bridge.thread = thread
    deliver(thread, { from = "them", image = image, time = theme.time_text() })
end

-- Story flags pages and apps can read (data.flags).
function desk.flag(name: string, value)
    data.flags[name] = if value == nil then true else value
    data.changed()
end

function desk.bookmark(url: string)
    if not table.find(data.bookmarks, url) then
        table.insert(data.bookmarks, url)
        data.changed()
    end
end

-- Sets the computer's clock (it keeps running from there).
function desk.clock(h: number, m: number)
    theme.clock = { minutes = h * 60 + m, at = kulisa.now() }
end

function desk.wallpaper(name: string)
    shell.set_wallpaper(name)
end

return bridge
