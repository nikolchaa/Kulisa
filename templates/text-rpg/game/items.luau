-- Items: what they are called, their icon and description, and what using one does. Stories add
-- and remove them with give / take (game/rules.luau) and test them with has.

local mark = kulisa.lang.mark
local items = {}

items.lantern = {
    name = mark("Lantern"),
    icon = "items/lantern.png",
    text = mark("A tin lantern with a steady flame. Dark places are less dark."),
}
items.bread = {
    name = mark("Heel of bread"),
    icon = "items/bread.png",
    text = mark("Hard, but it keeps you going. Restores 2 health."),
    use = function(rules)
        rules.heal(2)
        rules.take("bread")
        return kulisa.t("You eat the bread.")
    end,
}
items.herb = {
    name = mark("Woundwort"),
    icon = "items/herb.png",
    text = mark("A bitter healing herb. Restores 4 health."),
    use = function(rules)
        rules.heal(4)
        rules.take("herb")
        return kulisa.t("You chew the woundwort. The pain eases.")
    end,
}
items.key = {
    name = mark("Rusty key"),
    icon = "items/key.png",
    text = mark("Found in the mill cellar. It is warm, somehow."),
}
items.coins = {
    name = mark("Coin purse"),
    icon = "items/coins.png",
    text = mark("Someone's savings. Adds 5 gold when opened."),
    use = function(rules)
        rules.take("coins")
        rules.pay(-5)
        return kulisa.t("Five coins clink into your pocket.")
    end,
}
items.rope = {
    name = mark("Rope"),
    icon = "items/rope.png",
    text = mark("Thirty feet of good hemp."),
}
items.map = {
    name = mark("Torn map"),
    icon = "items/map.png",
    text = mark("Half a map of the valley. An X marks the old well."),
}
items.dagger = {
    name = mark("Dagger"),
    icon = "items/dagger.png",
    text = mark("Light and sharp. +1 Might while you carry it."),
    might = 1,
}

return items
