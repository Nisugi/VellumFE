# Target Window

Display your current combat target with quick targeting controls.

## Goal

Show the currently targeted creature with health estimation and quick-switch capabilities.

## The Data

GemStone IV sends target data via:

```xml
<dialogData id='combat'>
  <dropDownBox id='dDBTarget' value="black valravn"
    cmd="target %dDBTarget%"
    content_text="none,black valravn,troll"
    content_value="target help,#534103532,#534103533" .../>
</dialogData>
```

And targeting messages:
```
You are now targeting an eyeless black valravn.
```

## Layout

```toml
[[widgets]]
type = "text"
name = "target"
stream = "target"
x = 85
y = 10
width = 35
height = 5
show_border = true
title = "Target"
```

## Target Highlights

```toml
# highlights.toml

# Currently targeting
[[highlights]]
pattern = "You are now targeting"
fg = "bright_cyan"
bold = true

# Target cleared
[[highlights]]
pattern = "You are no longer targeting"
fg = "gray"

# Target status
[[highlights]]
pattern = "appears (dead|stunned|webbed|prone)"
fg = "bright_yellow"
bold = true
```

## Targeting Keybinds

```toml
# keybinds.toml

# Target cycling
[[keybinds]]
key = "Tab"
action = "send"
command = "target next"

[[keybinds]]
key = "Shift+Tab"
action = "send"
command = "target previous"

# Clear target
[[keybinds]]
key = "Escape"
action = "send"
command = "target clear"

# Target random
[[keybinds]]
key = "Ctrl+T"
action = "send"
command = "target random"

# Attack current target
[[keybinds]]
key = "F5"
action = "send"
command = "attack"

# Target and attack
[[keybinds]]
key = "Ctrl+A"
action = "send"
command = "target random;attack"
```

## Room Objects Integration

Combine with room objects display:

```toml
[[widgets]]
type = "room"
name = "room_with_targets"
x = 0
y = 30
width = 85
height = 5
show_objects = true
show_players = true
highlight_targets = true  # Highlight targetable creatures
```

## Target Status Parsing

Track target condition from combat messages:

```toml
# highlights.toml

# Target health estimation
[[highlights]]
pattern = "(appears to be |seems |is )(uninjured|in good shape)"
fg = "green"

[[highlights]]
pattern = "(has minor |slight |a few )(wounds|injuries|scratches)"
fg = "yellow"

[[highlights]]
pattern = "(has moderate |significant |serious )(wounds|injuries)"
fg = "orange"

[[highlights]]
pattern = "(has severe |critical |grievous |terrible )(wounds|injuries)"
fg = "red"
bold = true

[[highlights]]
pattern = "(appears dead|lies dead|crumples|falls dead)"
fg = "bright_green"
bold = true
```

## Multiple Target Tracking

For hunting multiple creatures:

```toml
[[widgets]]
type = "text"
name = "creatures"
stream = "room_objects"
x = 85
y = 10
width = 35
height = 10
show_border = true
title = "Creatures"
filter = "monster"  # Only show creatures, not items
```

## Quick Target Macros

```toml
# Target specific creature types
[[keybinds]]
key = "Ctrl+1"
action = "send"
command = "target orc"

[[keybinds]]
key = "Ctrl+2"
action = "send"
command = "target troll"

[[keybinds]]
key = "Ctrl+3"
action = "send"
command = "target valravn"

# Target by condition
[[keybinds]]
key = "Ctrl+S"
action = "send"
command = "target stunned"

[[keybinds]]
key = "Ctrl+W"
action = "send"
command = "target webbed"
```

## Combat Dashboard with Target

```toml
[[widgets]]
type = "dashboard"
name = "combat_hud"
x = 85
y = 0
width = 35
height = 20
title = "Combat"

[widgets.combat_hud.components]
target = { type = "text", data = "target", height = 3 }
health = { type = "progress", data = "vitals.health" }
rt = { type = "countdown", data = "roundtime" }
stance = { type = "progress", data = "stance" }
status = { type = "indicator", items = ["stunned", "hidden"] }
```

## Tips

1. **Use Tab for cycling** - Natural feel for target switching
2. **Color-code target status** - Know at a glance if target is hurt
3. **Combine with room display** - See all potential targets
4. **Quick macros** - Target specific creature types you hunt often

## See Also

- [Hunting HUD](./hunting-hud.md)
- [Combat Alerts](./combat-alerts.md)
- [Room Window](../widgets/room-window.md)
