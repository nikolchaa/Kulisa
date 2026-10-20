-- Sound and haptics. The recorded sounds are short and synthesized (see README), played in
-- bursts whose rate and loudness follow how fast the player is dragging, the way crumpling
-- film sounds: the faster the motion, the more crackles.

local Sfx = {}

Sfx.enabled = true

local last: { [string]: number } = {}
local PAN_WIDTH = 640

local function ready(kind: string, interval: number): boolean
    local now = kulisa.now()
    if now - (last[kind] or -1) < interval then
        return false
    end
    last[kind] = now
    return true
end

local function play(name: string, volume: number, rate: number?, x: number?)
    if not Sfx.enabled then
        return
    end
    local pan = if x then math.clamp((x - PAN_WIDTH) / PAN_WIDTH, -1, 1) * 0.45 else 0
    kulisa.audio.play("sound", `audio/{name}.wav`, { volume = volume, rate = rate or 1, pan = pan })
end

local function pick(base: string, count: number): string
    return `{base}{math.random(1, count)}`
end

local function pulse(strength: number, seconds: number)
    if Sfx.enabled then
        kulisa.haptics.pulse(math.clamp(strength, 0, 1), seconds)
    end
end

-- Crinkling plastic under a moving hand; `speed` in stage px/s, `x` the stage x (for panning).
function Sfx.crinkle(speed: number, x: number)
    local s = math.clamp(speed / 1300, 0, 1)
    if s < 0.06 then
        return
    end
    if ready("crinkle", 0.2 - 0.15 * s + math.random() * 0.04) then
        play(pick("crinkle", 6), 0.12 + 0.55 * s ^ 0.8, 0.82 + 0.4 * math.random(), x)
    end
    if ready("crinkle_haptic", 0.05) then
        pulse(0.06 + 0.22 * s, 0.014)
    end
end

-- A bag rustling as it is dragged.
function Sfx.rustle(speed: number, x: number)
    local s = math.clamp(speed / 1300, 0, 1)
    if s < 0.05 then
        return
    end
    if ready("rustle", 0.24 - 0.12 * s) then
        play(pick("rustle", 3), 0.2 + 0.55 * s, 0.9 + 0.25 * math.random(), x)
    end
    if ready("rustle_haptic", 0.06) then
        pulse(0.05 + 0.18 * s, 0.015)
    end
end

-- The fold jumps forward: adhesive letting go.
function Sfx.slip(strength: number, x: number)
    play(pick("slip", 3), 0.35 + 0.5 * strength, 0.9 + 0.3 * math.random(), x)
    pulse(0.3 + 0.45 * strength, 0.03)
end

function Sfx.stick(x: number)
    play("tap", 0.5, 1, x)
    pulse(0.3, 0.025)
end

function Sfx.tear(x: number)
    play("tear", 0.9, 1, x)
    pulse(0.75, 0.09)
end

function Sfx.whoosh(speed: number, x: number)
    play("whoosh", math.clamp(0.25 + speed / 1500, 0.25, 0.9), 0.9 + 0.2 * math.random(), x)
end

function Sfx.squeak(speed: number, x: number)
    local s = math.clamp(speed / 900, 0, 1)
    if s < 0.08 then
        return
    end
    if ready("squeak", 0.3 - 0.1 * s) then
        play(if math.random() < 0.35 then pick("squeak", 2) else pick("rub", 2), 0.2 + 0.5 * s, 0.85 + 0.3 * math.random(), x)
    end
    if ready("squeak_haptic", 0.06) then
        pulse(0.05 + 0.2 * s, 0.015)
    end
end

function Sfx.pop(size: number, x: number)
    play(pick("pop", 3), 0.3 + 0.4 * size, 1.25 - 0.5 * size + 0.15 * math.random(), x)
    pulse(0.25 + 0.4 * size, 0.025)
end

function Sfx.chime()
    play("chime", 0.7)
    pulse(0.5, 0.06)
end

return Sfx
