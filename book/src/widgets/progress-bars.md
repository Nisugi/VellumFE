# Progress Bars

Progress bars display character statistics as visual bars with optional numeric values.

## Overview

Progress bars:
- Show health, mana, stamina, spirit, encumbrance
- Update automatically from game data
- Support custom colors and styling
- Can show value, percentage, or both

## Configuration

```toml
[[windows]]
name = "health"
type = "progress"

# Position and size
row = 0
col = 80
width = 30
height = 1

# Progress-specific options
stat = "health"           # Stat to track (required)
show_value = true         # Show "425/500"
show_percentage = false   # Show "85%"
show_label = true         # Show stat name

# Colors
bar_color = "#00FF00"     # Bar fill color
bar_empty_color = "#333333"  # Empty portion
text_color = "#FFFFFF"    # Text color
```

## Properties

### stat (required)

The character statistic to display:

| Stat | Description |
|------|-------------|
| `health` | Health points |
| `mana` | Mana points |
| `spirit` | Spirit points |
| `stamina` | Stamina points |
| `encumbrance` | Encumbrance level |

### show_value

Display the numeric value:

```toml
show_value = true     # Shows "425/500"
show_value = false    # Bar only
```

### show_percentage

Display percentage instead of/with value:

```toml
show_percentage = true    # Shows "85%"
show_percentage = false   # Shows value or nothing
```

### show_label

Display the stat name:

```toml
show_label = true     # Shows "HP: 425/500"
show_label = false    # Shows "425/500"
```

### bar_color

Color of the filled portion:

```toml
bar_color = "#00FF00"     # Green
bar_color = "health"      # From palette
bar_color = "@health"     # From preset
```

### Dynamic Colors

Use different colors based on value:

```toml
[[windows]]
name = "health"
type = "progress"
stat = "health"

# Color thresholds (checked in order)
[windows.color_thresholds]
75 = "#00FF00"    # Green above 75%
50 = "#FFFF00"    # Yellow 50-75%
25 = "#FF8800"    # Orange 25-50%
0 = "#FF0000"     # Red below 25%
```

## Display Formats

### Compact (1 row)

```toml
[[windows]]
name = "health"
type = "progress"
stat = "health"
width = 20
height = 1
show_label = true
show_value = true
```

Result: `HP ████████░░ 80%`

### Minimal (Bar only)

```toml
[[windows]]
name = "health"
type = "progress"
stat = "health"
width = 15
height = 1
show_label = false
show_value = false
```

Result: `████████████░░░`

### Vertical Bar

```toml
[[windows]]
name = "health"
type = "progress"
stat = "health"
width = 3
height = 10
orientation = "vertical"
```

## Examples

### Health Bar (Green)

```toml
[[windows]]
name = "health"
type = "progress"
stat = "health"
row = 0
col = 80
width = 25
height = 1
bar_color = "#00FF00"
show_value = true
```

### Mana Bar (Blue)

```toml
[[windows]]
name = "mana"
type = "progress"
stat = "mana"
row = 1
col = 80
width = 25
height = 1
bar_color = "#0088FF"
show_value = true
```

### Spirit Bar (Cyan)

```toml
[[windows]]
name = "spirit"
type = "progress"
stat = "spirit"
row = 2
col = 80
width = 25
height = 1
bar_color = "#00FFFF"
show_value = true
```

### Stamina Bar (Yellow)

```toml
[[windows]]
name = "stamina"
type = "progress"
stat = "stamina"
row = 3
col = 80
width = 25
height = 1
bar_color = "#FFFF00"
show_value = true
```

### Encumbrance Bar

```toml
[[windows]]
name = "encumbrance"
type = "progress"
stat = "encumbrance"
row = 4
col = 80
width = 25
height = 1
bar_color = "#888888"
show_percentage = true
```

### Stacked Vitals

```toml
# All vitals in one area
[[windows]]
name = "health"
type = "progress"
stat = "health"
row = 0
col = 100
width = 20
height = 1

[[windows]]
name = "mana"
type = "progress"
stat = "mana"
row = 1
col = 100
width = 20
height = 1

[[windows]]
name = "spirit"
type = "progress"
stat = "spirit"
row = 2
col = 100
width = 20
height = 1

[[windows]]
name = "stamina"
type = "progress"
stat = "stamina"
row = 3
col = 100
width = 20
height = 1
```

## Data Source

Progress bars receive data from `<progressBar>` XML elements:

```xml
<progressBar id='health' value='85' text='health 425/500' />
```

The parser extracts:
- `id`: Stat type
- `value`: Percentage (0-100)
- `text`: Parse current/max values

## Troubleshooting

### Bar not updating

1. Verify `stat` matches a valid stat ID
2. Check game is sending progress bar data
3. Ensure widget is enabled

### Wrong colors

1. Check `bar_color` syntax
2. Verify palette/preset references exist
3. Check color threshold order

### Value display issues

1. Ensure `show_value` or `show_percentage` is true
2. Check width is sufficient for text
3. Verify text_color is visible against background

## See Also

- [Dashboard](./dashboard.md) - Composite status widget
- [Countdowns](./countdowns.md) - RT/cast time display
- [Color Configuration](../configuration/colors-toml.md) - Color settings
