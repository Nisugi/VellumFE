# Compass

The compass widget displays available room exits as directional indicators.

## Overview

The compass:
- Shows all available exits from current room
- Updates automatically when you move
- Supports graphical and text display modes
- Highlights cardinal and special directions

## Configuration

```toml
[[windows]]
name = "compass"
type = "compass"

# Position and size
row = 0
col = 100
width = 11
height = 5

# Compass-specific options
style = "graphical"       # "graphical" or "text"
show_labels = true        # Show N/S/E/W labels
compact = false           # Compact display mode

# Colors
active_color = "#FFFFFF"  # Available exit color
inactive_color = "#333333"  # Unavailable direction
special_color = "#00FFFF" # Special exits (out, up, down)
```

## Properties

### style

Display style:

| Style | Description |
|-------|-------------|
| `graphical` | Visual compass rose |
| `text` | Text list of exits |

### show_labels

Show direction labels:

```toml
show_labels = true    # Shows N, S, E, W, etc.
show_labels = false   # Icons/indicators only
```

### compact

Use compact layout:

```toml
compact = false   # Full 5x11 compass (default)
compact = true    # Smaller 3x7 compass
```

## Direction Mapping

The compass recognizes these directions:

| Direction | Abbreviation | Position |
|-----------|--------------|----------|
| north | n | Top center |
| south | s | Bottom center |
| east | e | Right center |
| west | w | Left center |
| northeast | ne | Top right |
| northwest | nw | Top left |
| southeast | se | Bottom right |
| southwest | sw | Bottom left |
| up | u | Top (special) |
| down | d | Bottom (special) |
| out | out | Center or special |

## Display Modes

### Graphical Mode (Default)

```
    [N]
[NW]   [NE]
[W]  +  [E]
[SW]   [SE]
    [S]
```

Available exits are highlighted, unavailable are dimmed.

### Text Mode

```
Exits: n, e, sw, out
```

Simple text list of available directions.

### Compact Mode

```
 N
W+E
 S
```

Smaller footprint, cardinal directions only.

## Examples

### Standard Compass

```toml
[[windows]]
name = "compass"
type = "compass"
row = 5
col = 100
width = 11
height = 5
style = "graphical"
show_labels = true
border_style = "rounded"
title = "Exits"
```

### Compact Compass

```toml
[[windows]]
name = "compass"
type = "compass"
row = 0
col = 110
width = 7
height = 3
style = "graphical"
compact = true
show_border = false
```

### Text List

```toml
[[windows]]
name = "exits"
type = "compass"
row = 10
col = 80
width = 25
height = 1
style = "text"
show_border = false
```

### Colored Compass

```toml
[[windows]]
name = "compass"
type = "compass"
row = 5
col = 100
width = 11
height = 5
active_color = "#00FF00"      # Green for available
inactive_color = "#1A1A1A"    # Very dark for unavailable
special_color = "#FFFF00"     # Yellow for up/down/out
```

## Interaction

The compass is display-only by default. With `links = true`:

| Input | Action |
|-------|--------|
| Click direction | Move that direction |
| Hover | Highlight direction |

## Data Source

The compass receives data from `<compass>` XML elements:

```xml
<compass>
  <dir value="n"/>
  <dir value="e"/>
  <dir value="out"/>
</compass>
```

The `<nav>` element provides additional room information:

```xml
<nav rm='123456'/>
```

## Troubleshooting

### Compass not updating

1. Verify you're receiving compass data (check main window)
2. Some areas don't send compass info
3. Check widget is properly configured

### Exits showing incorrectly

1. The game controls compass data
2. Some rooms have unusual exit names
3. Special exits may not appear in compass

### Display issues

1. Check `width` and `height` are sufficient
2. Verify style matches your preference
3. Check colors are visible against background

## See Also

- [Room Window](./room-window.md) - Full room information
- [layout.toml Reference](../configuration/layout-toml.md) - Layout syntax
- [Navigation Tutorial](../tutorials/your-first-layout.md) - Layout examples
