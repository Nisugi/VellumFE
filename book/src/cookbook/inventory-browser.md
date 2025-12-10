# Inventory Browser

Navigate and display your inventory with container support.

## Goal

Display inventory contents, open containers, and track worn items.

## The Data

GemStone IV sends inventory via the `inv` stream:

```xml
<streamWindow id='inv' title='My Inventory' target='wear' ifClosed='' resident='true'/>
<clearStream id='inv' ifClosed=''/>
<pushStream id='inv'/>Your worn items are:
  a patchwork dwarf skin backpack
  an enruned urglaes band
<popStream/>
```

Container contents use `<container>` and `<inv>` tags:

```xml
<container id="535703780">
  <inv id="123">a silver wand</inv>
  <inv id="124">a gold ring</inv>
</container>
```

## Basic Inventory Window

```toml
[[widgets]]
type = "text"
name = "inventory"
stream = "inv"
x = 85
y = 0
width = 35
height = 20
buffer_size = 200
show_border = true
title = "Inventory"
clickable = true  # Enable clicking items
```

## Tabbed Inventory View

```toml
[[widgets]]
type = "tabbedtext"
name = "inventory_tabs"
x = 85
y = 0
width = 35
height = 25
show_border = true
tabs = [
    { name = "Worn", stream = "inv" },
    { name = "Pack", stream = "container_pack" },
    { name = "Disk", stream = "container_disk" },
    { name = "Locker", stream = "container_locker" }
]
```

## Inventory Highlights

```toml
# highlights.toml

# Containers
[[highlights]]
pattern = "(backpack|satchel|cloak|pouch|sack|bag)"
fg = "cyan"

# Weapons
[[highlights]]
pattern = "(sword|dagger|bow|staff|axe|mace|falchion)"
fg = "bright_red"

# Armor
[[highlights]]
pattern = "(armor|shield|helm|gauntlet|greave|vambrace)"
fg = "bright_blue"

# Magic items
[[highlights]]
pattern = "(wand|rod|amulet|ring|crystal|orb)"
fg = "bright_magenta"

# Valuable
[[highlights]]
pattern = "(gold|silver|platinum|mithril|vultite|ora)"
fg = "bright_yellow"
```

## Inventory Commands

```toml
# keybinds.toml

# Check inventory
[[keybinds]]
key = "I"
action = "send"
command = "inventory"

# Check specific containers
[[keybinds]]
key = "Ctrl+I"
action = "send"
command = "look in my pack"

# Quick wear/remove
[[keybinds]]
key = "Ctrl+W"
action = "send"
command = "wear my"

[[keybinds]]
key = "Ctrl+R"
action = "send"
command = "remove my"
```

## Container Browser Widget

```toml
[[widgets]]
type = "container"
name = "container_view"
x = 85
y = 0
width = 35
height = 25
show_border = true
title = "Container"
show_count = true      # Show item count
show_capacity = true   # Show if nearly full
nested = true          # Support nested containers
```

## Inventory Alerts

```toml
# triggers.toml

# Container full
[[triggers]]
name = "container_full"
pattern = "(can't fit|won't fit|is full|no room)"
tts = "Container full"
sound = "warning.wav"

# Item picked up
[[triggers]]
name = "item_got"
pattern = "You (pick up|get|grab)"
sound = "pickup.wav"
cooldown = 100
```

## Hands Display Integration

```toml
[[widgets]]
type = "hands"
name = "hands"
x = 85
y = 26
width = 35
height = 4
show_spell = true
show_border = true
title = "Hands"
```

## Full Inventory Layout

```toml
# Complete inventory management layout

[[widgets]]
type = "hands"
name = "hands"
x = 85
y = 0
width = 35
height = 3
show_spell = true
title = "Hands"

[[widgets]]
type = "text"
name = "worn"
stream = "inv"
x = 85
y = 3
width = 35
height = 15
title = "Worn Items"

[[widgets]]
type = "text"
name = "container"
stream = "container"
x = 85
y = 18
width = 35
height = 15
title = "Container"

[[widgets]]
type = "progress"
name = "encum"
data_source = "encumbrance"
x = 85
y = 33
width = 35
height = 2
title = "Load"
```

## Tips

1. **Use tabs** for different containers
2. **Enable clicking** to interact with items
3. **Color-code item types** for quick scanning
4. **Track encumbrance** alongside inventory
5. **Set alerts** for full containers

## See Also

- [Encumbrance Monitor](./encumbrance-monitor.md)
- [Hands Widget](../widgets/hands.md)
- [Text Windows](../widgets/text-windows.md)
