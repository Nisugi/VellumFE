# Loot Window

Dedicated window for tracking loot drops and treasure.

## Goal

Separate loot messages from main game text for easy tracking during hunting sessions.

## Configuration

### Basic Loot Window

```toml
[[widgets]]
type = "text"
name = "loot"
stream = "loot"
x = 85
y = 20
width = 35
height = 15
buffer_size = 500
show_border = true
title = "Loot"
```

### Stream Filter Setup

Configure what goes to the loot stream:

```toml
# In config.toml
[streams.loot]
patterns = [
    "drops? a",
    "falls to the ground",
    "\\d+ (silver|silvers|coins)",
    "You also see",
    "(gem|jewel|gold|platinum|diamond)",
    "(strongbox|chest|coffer|lockbox)"
]
```

## Loot Highlights

```toml
# highlights.toml

# Currency
[[highlights]]
pattern = "\\d+ (silver|silvers|coins)"
fg = "bright_yellow"
bold = true

# Valuable gems
[[highlights]]
pattern = "(diamond|emerald|ruby|sapphire|pearl)"
fg = "bright_cyan"
bold = true

# Boxes (lockpicking)
[[highlights]]
pattern = "(strongbox|chest|coffer|lockbox|box)"
fg = "cyan"

# Rare drops
[[highlights]]
pattern = "(rare|unusual|exceptional|perfect)"
fg = "bright_magenta"
bold = true

# Skins/trophies
[[highlights]]
pattern = "(skin|pelt|hide|claw|fang|horn)"
fg = "yellow"
```

## Loot Alerts

```toml
# triggers.toml

# Big silver drop
[[triggers]]
name = "big_silver"
pattern = "(\\d{4,}) silvers?"
tts = "Big silver drop"
sound = "coins.wav"

# Box drop
[[triggers]]
name = "box_drop"
pattern = "drops? a.*(strongbox|chest|coffer)"
sound = "box.wav"

# Rare item
[[triggers]]
name = "rare_drop"
pattern = "drops? a.*(rare|unusual|exceptional)"
tts = "Rare item"
sound = "rare.wav"
```

## Loot Summary Widget

Track session totals:

```toml
[[widgets]]
type = "dashboard"
name = "loot_summary"
x = 85
y = 35
width = 35
height = 3
title = "Session"

[widgets.loot_summary.components]
silver = { type = "counter", data = "session.silver", label = "Silver" }
boxes = { type = "counter", data = "session.boxes", label = "Boxes" }
kills = { type = "counter", data = "session.kills", label = "Kills" }
```

## Multi-Tab Loot Tracking

```toml
[[widgets]]
type = "tabbedtext"
name = "treasure"
x = 85
y = 15
width = 35
height = 20
show_border = true
tabs = [
    { name = "Loot", stream = "loot" },
    { name = "Boxes", stream = "boxes" },
    { name = "Gems", stream = "gems" }
]
```

## Keybinds for Looting

```toml
# keybinds.toml

# Quick loot
[[keybinds]]
key = "L"
action = "send"
command = "loot"

# Look for loot
[[keybinds]]
key = "Ctrl+L"
action = "send"
command = "look"

# Get all
[[keybinds]]
key = "G"
action = "send"
command = "get coins;get box"

# Appraise
[[keybinds]]
key = "Ctrl+A"
action = "send"
command = "appraise"
```

## Tips

1. **Keep loot window small** - Just needs to show recent drops
2. **Use sound alerts** for valuable drops so you don't miss them
3. **Color-code by value** - Instantly spot the good stuff
4. **Position near encumbrance** - Watch your load while looting

## See Also

- [Encumbrance Monitor](./encumbrance-monitor.md)
- [Hunting HUD](./hunting-hud.md)
- [Text Windows](../widgets/text-windows.md)
