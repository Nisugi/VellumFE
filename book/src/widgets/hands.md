# Hands

Hand widgets display items held in your character's hands and prepared spells.

## Overview

Hand widgets:
- Show left hand, right hand, or spell contents
- Update automatically when items change
- Support clickable links to items
- Display custom icons

## Configuration

```toml
[[windows]]
name = "right_hand"
type = "hand"

# Position and size
row = 10
col = 100
width = 20
height = 1

# Hand-specific options
hand_type = "right"       # "left", "right", or "spell"

# Display options
icon = "🤚"               # Icon before text
icon_color = "#FFFFFF"    # Icon color
text_color = "#CCCCCC"    # Item text color

# Visual options
show_border = true
border_style = "rounded"
```

## Properties

### hand_type (required)

Which hand/slot to display:

| Type | Content |
|------|---------|
| `left` | Left hand item |
| `right` | Right hand item |
| `spell` | Prepared spell |

### icon

Icon displayed before content:

```toml
# Hand icons
icon = "🤚"       # Generic hand
icon = "👋"       # Wave (left)
icon = "✋"       # Palm (right)
icon = "✨"       # Sparkle (spell)

# Text icons
icon = "[L]"      # Left bracket
icon = "[R]"      # Right bracket
icon = "[S]"      # Spell bracket

# No icon
icon = ""
```

### icon_color

Color of the icon:

```toml
icon_color = "#FFFFFF"    # White
icon_color = "bright_cyan"  # Palette color
icon_color = "@links"     # Preset color
```

### text_color

Color of the item text:

```toml
text_color = "#CCCCCC"    # Default gray
text_color = "@links"     # Link color from presets
```

## Display Modes

### With Icon

```
🤚 a gleaming vultite sword
```

### Text Only

```
[R] a gleaming vultite sword
```

### Compact

```toml
show_border = false
icon = ""
width = 25
height = 1
```

Result: `a gleaming vultite sword`

## Interaction

With `links = true` in config.toml:

| Input | Action |
|-------|--------|
| Click item | Primary action |
| Right-click | Context menu |

Actions depend on the item type:
- Weapons: Attack commands
- Containers: Open/search
- General: Look/inspect

## Examples

### Right Hand

```toml
[[windows]]
name = "right_hand"
type = "hand"
hand_type = "right"
row = 5
col = 80
width = 25
height = 1
icon = "✋"
icon_color = "#AAAAAA"
title = "Right"
```

### Left Hand

```toml
[[windows]]
name = "left_hand"
type = "hand"
hand_type = "left"
row = 6
col = 80
width = 25
height = 1
icon = "🤚"
icon_color = "#AAAAAA"
title = "Left"
```

### Spell Hand

```toml
[[windows]]
name = "spell"
type = "hand"
hand_type = "spell"
row = 7
col = 80
width = 25
height = 1
icon = "✨"
icon_color = "#FFFF00"
title = "Spell"
text_color = "#FF88FF"
```

### Stacked Hands

```toml
# All three hands vertically
[[windows]]
name = "right_hand"
type = "hand"
hand_type = "right"
row = 10
col = 100
width = 20
height = 1
icon = "R:"
show_border = false

[[windows]]
name = "left_hand"
type = "hand"
hand_type = "left"
row = 11
col = 100
width = 20
height = 1
icon = "L:"
show_border = false

[[windows]]
name = "spell"
type = "hand"
hand_type = "spell"
row = 12
col = 100
width = 20
height = 1
icon = "S:"
show_border = false
```

### Bordered Group

```toml
[[windows]]
name = "hands"
type = "container"
row = 10
col = 100
width = 22
height = 5
title = "Hands"
border_style = "rounded"

[[windows]]
name = "right_hand"
type = "hand"
hand_type = "right"
row = 11
col = 101
width = 20
height = 1
show_border = false

[[windows]]
name = "left_hand"
type = "hand"
hand_type = "left"
row = 12
col = 101
width = 20
height = 1
show_border = false

[[windows]]
name = "spell"
type = "hand"
hand_type = "spell"
row = 13
col = 101
width = 20
height = 1
show_border = false
```

## Data Source

Hand widgets receive data from XML elements:

```xml
<right exist="123456" noun="sword">a gleaming vultite sword</right>
<left exist="789012" noun="shield">a battered wooden shield</left>
<spell>Minor Sanctuary</spell>
```

The `exist` and `noun` attributes enable link functionality.

## Empty Hands

When a hand is empty:
- Display shows "Empty" or nothing
- Configure empty display:

```toml
empty_text = "Empty"      # Show "Empty"
empty_text = "-"          # Show dash
empty_text = ""           # Show nothing
```

## Troubleshooting

### Hand not updating

1. Check `hand_type` is correct
2. Verify game is sending hand data
3. Check widget is positioned correctly

### Links not working

1. Enable `links = true` in config.toml
2. Verify item has exist ID
3. Check for click handler conflicts

### Text truncated

1. Increase widget `width`
2. Remove icon to save space
3. Use smaller font if available

## See Also

- [Indicators](./indicators.md) - Status indicators
- [Dashboard](./dashboard.md) - Composite status
- [Link Configuration](../configuration/config-toml.md) - Link settings
