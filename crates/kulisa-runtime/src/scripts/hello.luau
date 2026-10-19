-- M0 smoke test: proves Luau runs on every target by driving the quad each frame.
kulisa.log("hello from " .. _VERSION .. " on Kulisa " .. kulisa.version)

local M = {}

function M.update(t: number)
    local a = 0.85 + 0.15 * math.sin(t * 2)
    -- premultiplied tint (r, g, b, a) and rotation in radians
    return a, a, a, a, math.sin(t * 0.8) * 0.12
end

return M
