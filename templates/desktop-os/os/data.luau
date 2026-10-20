-- What the fake computer remembers: chat threads, notes, browsing history and story flags.
-- Everything here is plain data, saved with the story (see `data.save` / `data.load`).

local ui = kulisa.ui

local data = {}

-- Chat threads by id: { id, name, avatar, messages = { { from = "them" | "me", text?, image?,
-- time } }, unread }, and their order (most recent first).
data.threads = {}
data.thread_order = {}
-- Notes: { { title, text, time } }, newest first.
data.notes = {}
-- Pages visited (url -> true), the browser's bookmarks and flags stories set (desk.flag).
data.visited = {}
data.bookmarks = { "searchly.com" }
data.flags = {}
-- Bumps when any of the above changes, so screens that show it rebuild.
data.revision = ui.signal(0)

function data.changed()
    data.revision:set(data.revision:peek() + 1)
end

-- The thread `id`, made on first use from the story character `who` (name and avatar).
function data.thread(id: string, who)
    local t = data.threads[id]
    if t == nil then
        t = {
            id = id,
            name = (who and who.name) or id,
            avatar = (who and who.avatar) or "avatars/unknown.png",
            messages = {},
            unread = 0,
        }
        data.threads[id] = t
        table.insert(data.thread_order, 1, id)
    end
    return t
end

-- Adds a message and moves the thread to the top.
function data.post(id: string, msg)
    local t = data.thread(id)
    table.insert(t.messages, msg)
    local i = table.find(data.thread_order, id)
    if i then
        table.remove(data.thread_order, i)
    end
    table.insert(data.thread_order, 1, id)
    data.changed()
    return t
end

function data.unread_total(): number
    local n = 0
    for _, t in data.threads do
        n += t.unread
    end
    return n
end

function data.save()
    return {
        threads = data.threads,
        thread_order = data.thread_order,
        notes = data.notes,
        visited = data.visited,
        bookmarks = data.bookmarks,
        flags = data.flags,
    }
end

function data.load(d)
    d = d or {}
    data.threads = d.threads or {}
    data.thread_order = d.thread_order or {}
    data.notes = d.notes or {}
    data.visited = d.visited or {}
    data.bookmarks = d.bookmarks or { "searchly.com" }
    data.flags = d.flags or {}
    data.changed()
end

return data
