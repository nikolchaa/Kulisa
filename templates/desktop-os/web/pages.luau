-- The fake web. Each page is { title, theme, blocks } where blocks are:
--   { "h1", text }, { "h2", text }, { "p", text }, { "meta", text }, { "quote", text },
--   { "img", path, caption?, aspect = h / w }, { "link", text, url, description? },
--   { "links", { { text, url }, ... } }, { "post", user = ..., avatar = ..., when = ..., text = ... },
--   { "search" }, { "space", units }, { "rule" }, { "widget", any kulisa.ui widget }
-- Text takes Kulisa markup. A page may be a function(url, data) returning that table, so it can
-- change with the story (data.flags). Add sites to `sites`; `search` maps queries to results.

local data = require("os/data")
local theme = require("os/theme")

local pages = {}

local LIGHT = {
    bg = "#f6f7fb", fg = "#1d2130", dim = "#6b7186", accent = "#4a5ee8", link = "#3346d6",
    link_hover = "#7b8cff", card = "#e9ecf5", card_hover = "#dfe4f2", rule = "#00000018", good = "#2f9466",
}
local NIGHT = {
    bg = "#0d1018", fg = "#d9dde8", dim = "#7d8499", accent = "#ffb454", link = "#ffcf7a",
    link_hover = "#fff1cc", card = "#171b26", card_hover = "#1f2433", rule = "#ffffff14", good = "#66c79a",
}
local NEWS = {
    bg = "#fbf8f2", fg = "#1b1a17", dim = "#76716a", accent = "#c2402a", link = "#a3331f",
    link_hover = "#e0634b", card = "#efe9de", rule = "#00000020", good = "#4c7a55",
}
local FORUM = {
    bg = "#e8edf0", fg = "#22303a", dim = "#68808e", accent = "#1f7a8c", link = "#1f7a8c",
    link_hover = "#3fb0c6", card = "#f8fbfc", rule = "#00000018", good = "#2f8f6a",
}

local sites = {}

sites["searchly.com"] = {
    title = "Searchly",
    theme = LIGHT,
    blocks = {
        { "space", 60 },
        { "h1", "{size=2.2}{color=#4a5ee8}S{/color}{color=#e8574a}e{/color}{color=#f2b13a}a{/color}{color=#4a5ee8}r{/color}{color=#3aa76d}c{/color}{color=#e8574a}h{/color}{color=#4a5ee8}ly{/color}{/size}" },
        { "meta", "Find anything. Probably." },
        { "space", 10 },
        { "search" },
        { "space", 20 },
        { "meta", "Trending tonight" },
        { "links", { { "Gull Point lighthouse", "searchly.com/search?q=gull point lighthouse" },
            { "coast weather", "searchly.com/search?q=coast weather" } } },
    },
}

sites["lighthousecam.net"] = function()
    local lit = data.flags.cam_lit
    return {
        title = "Gull Point Light — Live",
        theme = NIGHT,
        blocks = {
            { "meta", "{color=#ff5f6d}●{/color} LIVE   CAM 2 — Gull Point, north headland   " .. theme.time_text() },
            { "img", if lit then "web/img/cam_on.png" else "web/img/cam_off.png" },
            { "h1", "Gull Point Light" },
            { "p", if lit
                then "{color=#ffcf7a}The light is on.{/color} Rotation 1 flash every 10 seconds."
                else "Status: {color=#ff8f6b}no signal from the lantern room.{/color} The camera is fine; the light is not." },
            { "p", "Built 1887. Automated 1979, but kept by a resident keeper on request of the harbour board. Our volunteers keep this camera running so sailors and the curious can see the light from anywhere." },
            { "rule" },
            { "links", { { "Discuss on tidepool.forum", "tidepool.forum/t/gull-point" }, { "Local news", "coastnews.net" } } },
        },
    }
end

sites["coastnews.net"] = {
    title = "Coast News",
    theme = NEWS,
    blocks = {
        { "h1", "{color=#c2402a}COAST{/color} NEWS" },
        { "meta", "Tuesday edition — fog advisory until Thursday" },
        { "rule" },
        { "link", "Keeper missing at Gull Point", "coastnews.net/hale", "Search suspended overnight as fog rolls in." },
        { "link", "Harbour board votes on new breakwater", "coastnews.net/breakwater" },
        { "link", "Ferry times change for the winter", "coastnews.net/ferry" },
    },
}

sites["coastnews.net/hale"] = {
    title = "Keeper missing at Gull Point — Coast News",
    theme = NEWS,
    blocks = {
        { "meta", "{color=#c2402a}{b}LOCAL{/b}{/color}  ·  by M. Okafor  ·  updated 18:04" },
        { "h1", "Keeper missing at Gull Point" },
        { "img", "web/img/pier.png", "The north pier, closed since Tuesday morning." },
        { "p", "Edwin Hale, 71, keeper of the Gull Point light for forty years, has not been seen since Tuesday. His boat is still moored and the cottage door was found unlocked." },
        { "p", "Volunteers searched the headland and the shoreline until fog made it unsafe. The coastguard will resume at first light." },
        { "quote", "He never let the light go out. Not once. Not in the '96 storm.", },
        { "p", "Residents report that the light has been dark since Tuesday night. The harbour board says the lantern is automated and \"should not depend on anyone\"." },
        { "rule" },
        { "links", { { "Live camera", "lighthousecam.net" }, { "Front page", "coastnews.net" } } },
    },
}

sites["tidepool.forum"] = {
    title = "tidepool.forum",
    theme = FORUM,
    blocks = {
        { "h1", "tidepool" },
        { "meta", "a forum for the north coast" },
        { "rule" },
        { "link", "[Gull Point] the light is out again (44 replies)", "tidepool.forum/t/gull-point" },
        { "link", "best tide pools after a storm?", "tidepool.forum/t/tides" },
        { "link", "selling: 14ft dinghy, needs paint", "tidepool.forum/t/dinghy" },
    },
}

sites["tidepool.forum/t/gull-point"] = {
    title = "[Gull Point] the light is out again — tidepool",
    theme = FORUM,
    blocks = {
        { "h1", "[Gull Point] the light is out again" },
        { "meta", "44 replies · 3 viewing now" },
        { "post", user = "saltwife", when = "2 days ago", avatar = "avatars/juno.png",
            text = "Third night dark. Has anyone gone up there? Hale's lights are always on by now." },
        { "post", user = "gullwatcher", when = "2 days ago", avatar = "avatars/gull.png",
            text = "Went to the gate. Cottage open, kettle cold. Lantern room door locked from the inside." },
        { "post", user = "harbourmaster_k", when = "yesterday",
            text = "Please don't go up there. Leave it to the coastguard." },
        { "post", user = "gullwatcher", when = "40 min ago", avatar = "avatars/gull.png",
            text = "Whatever you do, don't keep the cam open at night. I mean it. Close the tab." },
        { "links", { { "saltwife's photos", "driftwood.blog" }, { "Back to tidepool", "tidepool.forum" } } },
    },
}

sites["driftwood.blog"] = {
    title = "driftwood — photos by saltwife",
    theme = NIGHT,
    blocks = {
        { "h1", "driftwood" },
        { "meta", "photos from the north coast, by saltwife" },
        { "img", "web/img/coast.png", "Last light from the point. Mr Hale waved from the gallery rail." },
        { "p", "Taken from the rocks below the light, a week ago. He always waves. He says the light needs someone to watch it, even now it turns by itself." },
        { "links", { { "tidepool thread", "tidepool.forum/t/gull-point" } } },
    },
}

sites["404"] = {
    title = "Not found",
    theme = LIGHT,
    blocks = {
        { "space", 80 },
        { "h1", "Hmm. Nothing here." },
        { "p", "The page could not be found. Check the address, or search for it." },
        { "search" },
    },
}

-- Search results by keyword.
local INDEX = {
    { words = { "lighthouse", "gull", "light", "cam", "point" }, title = "Gull Point Light — Live camera", url = "lighthousecam.net",
        text = "Watch the Gull Point light live, all night." },
    { words = { "hale", "keeper", "missing", "gull", "point", "news" }, title = "Keeper missing at Gull Point — Coast News", url = "coastnews.net/hale",
        text = "Edwin Hale, keeper of the Gull Point light for forty years, has not been seen since Tuesday." },
    { words = { "gull", "point", "forum", "light", "out" }, title = "[Gull Point] the light is out again — tidepool", url = "tidepool.forum/t/gull-point",
        text = "Third night dark. Has anyone gone up there?" },
    { words = { "coast", "photos", "sunset", "saltwife", "driftwood" }, title = "driftwood — photos by saltwife", url = "driftwood.blog",
        text = "Last light from the point." },
    { words = { "coast", "news", "weather", "fog" }, title = "Coast News", url = "coastnews.net", text = "Fog advisory until Thursday." },
}

local function search(q: string)
    local words = {}
    for w in q:lower():gmatch("%w+") do
        table.insert(words, w)
    end
    local results = {}
    for _, e in INDEX do
        local score = 0
        for _, w in words do
            if table.find(e.words, w) then
                score += 1
            end
        end
        if score > 0 then
            table.insert(results, { score = score, e = e })
        end
    end
    table.sort(results, function(a, b)
        return a.score > b.score
    end)
    local blocks = { { "search" }, { "meta", `About {#results} results` }, { "space", 4 } }
    for _, r in results do
        table.insert(blocks, { "link", r.e.title, r.e.url, r.e.text })
    end
    if #results == 0 then
        table.insert(blocks, { "p", `No results for "{q}". Try fewer words.` })
    end
    return { title = `{q} — Searchly`, theme = LIGHT, blocks = blocks }
end

-- The page for `url` (never nil: unknown addresses get the 404 page).
function pages.get(url: string)
    local q = url:match("^searchly%.com/search%?q=(.*)$")
    if q then
        return search(q)
    end
    local photo = url:match("^photo:(.*)$")
    if photo then
        return { title = "Photo", theme = NIGHT, blocks = { { "img", photo }, { "meta", photo } } }
    end
    local p = sites[url] or sites["404"]
    if type(p) == "function" then
        p = p(url, data)
    end
    return p
end

return pages
