-- Post-processing as plain effect chains (docs/API.md, Shaders): each look is a list of effects;
-- the demo concatenates the ones switched on and puts them on the world layer, so the UI on top
-- stays crisp. Bloom and the tilt-shift blur use scaled stages and read the chain's source.

local Fx = {}

local S = {}
for _, name in { "bright", "down", "up", "bloom", "tiltshift", "grade", "crt", "grain" } do
    S[name] = kulisa.load_shader(`shaders/fx/{name}.wgsl`)
end

-- Grading presets: lift rgb, gamma rgb, gain rgb, (saturation, contrast, temperature, mix).
Fx.grades = {
    { name = "Warm film", params = { 0.02, 0.01, 0.0, 0, 1.0, 0.97, 0.92, 0, 1.08, 1.0, 0.92, 0, 1.12, 1.08, 0.6, 1 } },
    { name = "Moonlight", params = { 0.0, 0.02, 0.06, 0, 0.95, 1.0, 1.08, 0, 0.88, 0.97, 1.1, 0, 0.75, 1.1, -0.8, 1 } },
    { name = "Bleach", params = { 0.03, 0.03, 0.03, 0, 1.05, 1.05, 1.05, 0, 1.05, 1.05, 1.05, 0, 0.45, 1.25, 0.1, 1 } },
}
Fx.grade = 1

Fx.on = { bloom = kulisa.quality.tier ~= "low", tiltshift = false, grade = true, crt = false, grain = true }

-- A wide blur: down to 1/16 and back up, starting from the image the chain is at.
local function blur_chain(list, start_scale: number)
    local scales = { start_scale, start_scale / 2, start_scale / 4, start_scale / 2, start_scale }
    for i, s in scales do
        table.insert(list, { shader = if i <= 3 then S.down else S.up, scale = s, params = { 1 } })
    end
end

function Fx.chain()
    local list = {}
    if Fx.on.bloom then
        table.insert(list, { shader = S.bright, scale = 0.5, params = { 0.72, 0.25 } })
        blur_chain(list, 0.25)
        table.insert(list, { shader = S.bloom, mask = "source", params = { 0.9, 1.0, 0.92, 0.82 } })
    end
    if Fx.on.tiltshift then
        local start = #list -- the image before the blur
        blur_chain(list, 0.5)
        table.insert(list, {
            shader = S.tiltshift,
            mask = if start == 0 then "source" else start,
            params = { 1.0, 0.62, 0.22 },
        })
    end
    if Fx.on.grade then
        table.insert(list, { shader = S.grade, params = Fx.grades[Fx.grade].params })
    end
    if Fx.on.crt then
        table.insert(list, { shader = S.crt, params = { 0.18, 0.6, 0.5, 2.5 } })
    end
    if Fx.on.grain then
        table.insert(list, { shader = S.grain, params = { 0.045, 1.5, 0.35 } })
    end
    return list
end

-- Applies the chain to `layer`.
function Fx.apply(layer)
    layer.effects = Fx.chain()
end

return Fx
