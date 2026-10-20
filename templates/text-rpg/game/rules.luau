-- Game rules stories call from `$` lines and conditions: inventory, health, gold and dice checks.
-- They read and write story variables (story/*.kulisa `default`s), so saves keep everything.
--
--   $ give("herb")            $ take("bread", 1)          if has("key"):
--   $ hurt(3)  $ heal(2)      $ pay(4)  (negative earns)  $ passed = roll("wit", 12)

local items = require("game/items")
local log = require("game/log")

local rules = {}

local function vars()
    return kulisa.story.vars()
end

function rules.count(id: string): number
    local bag = vars().bag
    return (bag and bag[id]) or 0
end

function rules.has(id: string, n: number?): boolean
    return rules.count(id) >= (n or 1)
end

function rules.give(id: string, n: number?)
    local def = items[id]
    assert(def, `unknown item {id}; add it to game/items.luau`)
    local v = vars()
    v.bag = v.bag or {}
    v.bag[id] = rules.count(id) + (n or 1)
    log.add({ kind = "item", text = `Gained: {def.name}` .. (if (n or 1) > 1 then ` ×{n}` else ""), icon = def.icon })
end

function rules.take(id: string, n: number?)
    local v = vars()
    local left = rules.count(id) - (n or 1)
    v.bag[id] = if left > 0 then left else nil
end

function rules.heal(n: number)
    local v = vars()
    v.hp = math.min(v.max_hp, v.hp + n)
end

function rules.hurt(n: number)
    local v = vars()
    v.hp = math.max(0, v.hp - n)
    log.add({ kind = "note", text = `You lose {n} health.`, color = "bad" })
end

-- Spends gold (negative amounts earn it). Returns false if there is not enough.
function rules.pay(n: number): boolean
    local v = vars()
    if v.gold < n then
        return false
    end
    v.gold -= n
    return true
end

-- A stat's value including what carried items add (e.g. the dagger's Might).
function rules.stat(key: string): number
    local v = vars()[key] or 0
    for id, n in vars().bag or {} do
        local def = items[id]
        if def and def[key] and n > 0 then
            v += def[key]
        end
    end
    return v
end

-- A d20 check: d20 + stat against `target`. Writes the roll to the log and returns success.
function rules.roll(key: string, target: number): boolean
    local die = math.random(1, 20)
    local bonus = rules.stat(key)
    local total = die + bonus
    local ok = die == 20 or (die ~= 1 and total >= target)
    local label = key:sub(1, 1):upper() .. key:sub(2)
    log.add({
        kind = "roll",
        icon = "items/die.png",
        text = `{label} check: rolled {die} + {bonus} = {total} against {target}: ` .. (if ok then "success" else "failure"),
        color = if ok then "good" else "bad",
    })
    return ok
end

-- Uses an item from the inventory (outside the story's flow). Returns the message.
function rules.use(id: string): string?
    local def = items[id]
    if not def or not def.use or not rules.has(id) then
        return nil
    end
    local msg = def.use(rules)
    log.add({ kind = "note", text = msg })
    return msg
end

-- Globals for stories.
give, take, has, heal, hurt, pay, roll = rules.give, rules.take, rules.has, rules.heal, rules.hurt, rules.pay, rules.roll

return rules
