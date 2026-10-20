-- Items: what they are called, their icon and description, and what using one does. Stories add
-- and remove them with give / take (game/rules.luau) and test them with has.

local items = {}

items.lantern = {
    name = "Lantern",
    icon = "items/lantern.png",
    text = "A tin lantern with a steady flame. Dark places are less dark.",
}
items.bread = {
    name = "Heel of bread",
    icon = "items/bread.png",
    text = "Hard, but it keeps you going. Restores 2 health.",
    use = function(rules)
        rules.heal(2)
        rules.take("bread")
        return "You eat the bread."
    end,
}
items.herb = {
    name = "Woundwort",
    icon = "items/herb.png",
    text = "A bitter healing herb. Restores 4 health.",
    use = function(rules)
        rules.heal(4)
        rules.take("herb")
        return "You chew the woundwort. The pain eases."
    end,
}
items.key = {
    name = "Rusty key",
    icon = "items/key.png",
    text = "Found in the mill cellar. It is warm, somehow.",
}
items.coins = {
    name = "Coin purse",
    icon = "items/coins.png",
    text = "Someone's savings. Adds 5 gold when opened.",
    use = function(rules)
        rules.take("coins")
        rules.pay(-5)
        return "Five coins clink into your pocket."
    end,
}
items.rope = {
    name = "Rope",
    icon = "items/rope.png",
    text = "Thirty feet of good hemp.",
}
items.map = {
    name = "Torn map",
    icon = "items/map.png",
    text = "Half a map of the valley. An X marks the old well.",
}
items.dagger = {
    name = "Dagger",
    icon = "items/dagger.png",
    text = "Light and sharp. +1 Might while you carry it.",
    might = 1,
}

return items
