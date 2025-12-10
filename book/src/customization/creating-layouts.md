# Creating Layouts

Layouts define the arrangement of widgets on your screen. This guide walks you through creating custom layouts.

## Layout Basics

A layout is a collection of window definitions in `layout.toml`:

```toml
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = 80
height = 40
streams = ["main", "death"]

[[windows]]
name = "health"
type = "progress"
source = "health"
row = 0
col = 80
width = 30
height = 1
```

## Coordinate System

VellumFE uses a row/column grid:

```
(0,0)────────────────────────→ col
│
│    ┌─────────────┐
│    │  Widget     │
│    │  row=5      │
│    │  col=10     │
│    └─────────────┘
│
↓ row
```

- **row** - Vertical position (0 = top)
- **col** - Horizontal position (0 = left)
- **width** - Widget width in columns
- **height** - Widget height in rows

## Percentage-Based Sizing

Use percentages for responsive layouts:

```toml
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = "60%"    # 60% of screen width
height = "100%"  # Full screen height

[[windows]]
name = "sidebar"
type = "text"
row = 0
col = "60%"      # Start at 60% from left
width = "40%"    # Remaining 40%
height = "100%"
```

## Layout Planning

### Step 1: Sketch Your Layout

Draw your desired layout on paper or in a text editor:

```
┌─────────────────────┬──────────────┐
│                     │   Health     │
│                     ├──────────────┤
│    Main Window      │   Mana       │
│                     ├──────────────┤
│                     │   Compass    │
│                     ├──────────────┤
│                     │   Room       │
├─────────────────────┴──────────────┤
│         Command Input              │
└────────────────────────────────────┘
```

### Step 2: Calculate Dimensions

Determine sizes based on your terminal:

```
Terminal: 120 columns × 40 rows

Main window: 80 cols × 35 rows (0,0)
Sidebar: 40 cols × 35 rows (0,80)
Command: 120 cols × 5 rows (35,0)
```

### Step 3: Define Windows

Create `layout.toml`:

```toml
# Main game window
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = 80
height = 35
streams = ["main", "death"]
buffer_size = 2000

# Health bar
[[windows]]
name = "health"
type = "progress"
source = "health"
row = 0
col = 80
width = 40
height = 1

# Mana bar
[[windows]]
name = "mana"
type = "progress"
source = "mana"
row = 1
col = 80
width = 40
height = 1

# Compass
[[windows]]
name = "compass"
type = "compass"
row = 3
col = 80
width = 20
height = 5

# Room info
[[windows]]
name = "room"
type = "room"
row = 8
col = 80
width = 40
height = 15

# Command input
[[windows]]
name = "input"
type = "command_input"
row = 35
col = 0
width = 120
height = 5
```

## Common Layout Patterns

### Full-Width Main

```toml
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = "100%"
height = "90%"

[[windows]]
name = "input"
type = "command_input"
row = "90%"
col = 0
width = "100%"
height = "10%"
```

### Side Panel

```toml
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = "70%"
height = "100%"

[[windows]]
name = "sidebar"
type = "tabbedtext"
row = 0
col = "70%"
width = "30%"
height = "100%"
tabs = ["speech", "thoughts", "combat"]
```

### Three-Column

```toml
# Left column - Status
[[windows]]
name = "status"
type = "dashboard"
row = 0
col = 0
width = "20%"
height = "100%"

# Center column - Main
[[windows]]
name = "main"
type = "text"
row = 0
col = "20%"
width = "50%"
height = "100%"

# Right column - Info
[[windows]]
name = "info"
type = "tabbedtext"
row = 0
col = "70%"
width = "30%"
height = "100%"
```

### Top/Bottom Split

```toml
# Main game (top 70%)
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = "100%"
height = "70%"

# Combat log (bottom 30%)
[[windows]]
name = "combat"
type = "text"
row = "70%"
col = 0
width = "100%"
height = "30%"
streams = ["combat"]
```

## Widget Types

Choose the right widget type for each purpose:

| Type | Purpose | Example |
|------|---------|---------|
| `text` | General text display | Main window, logs |
| `tabbedtext` | Multiple streams in tabs | Speech, thoughts |
| `command_input` | Command entry | Input bar |
| `progress` | Bars (health, mana) | Vitals |
| `countdown` | Timers (RT, cast) | Roundtime |
| `compass` | Navigation | Exits |
| `room` | Room information | Room panel |
| `hand` | Held items | Hands display |
| `indicator` | Status icons | Conditions |
| `injury_doll` | Body injuries | Wounds |
| `active_effects` | Buffs/debuffs | Spells |
| `dashboard` | Combined status | Status panel |
| `performance` | Debug metrics | FPS, memory |

## Window Properties

### Common Properties

```toml
[[windows]]
name = "example"          # Unique name (required)
type = "text"             # Widget type (required)
row = 0                   # Position
col = 0
width = 40
height = 20

# Visual
title = "My Window"       # Custom title
show_title = true         # Show title bar
show_border = true        # Show border
border_style = "rounded"  # plain, rounded, double

# Colors
bg_color = "#000000"
text_color = "#FFFFFF"
border_color = "#00FFFF"

# Behavior
visible = true            # Initially visible
focusable = true          # Can receive focus
```

### Text Window Properties

```toml
[[windows]]
name = "main"
type = "text"
streams = ["main"]        # Stream IDs to display
buffer_size = 1000        # Max lines to keep
wordwrap = true           # Wrap long lines
timestamps = false        # Show timestamps
```

### Progress Bar Properties

```toml
[[windows]]
name = "health"
type = "progress"
source = "health"         # health, mana, spirit, stamina
show_label = true         # Show "Health"
show_numbers = true       # Show "85/100"
show_percent = false      # Show "85%"
bar_color = "#FF0000"
```

## Using the Layout Editor

The built-in editor is often easier:

```
.layout
```

Features:
- Visual widget placement
- Drag to reposition
- Resize handles
- Property editing
- Live preview

## Testing Your Layout

### Quick Test

1. Save `layout.toml`
2. Run `.reload layout`
3. Check appearance
4. Adjust and repeat

### Terminal Size Testing

Test at different sizes:

```bash
# Resize terminal and reload
resize -s 30 80    # Small
.reload layout

resize -s 40 120   # Medium
.reload layout

resize -s 50 160   # Large
.reload layout
```

## Troubleshooting

### Overlapping Windows

Windows are drawn in order. Later windows cover earlier ones:

```toml
# Background windows first
[[windows]]
name = "background"
# ...

# Foreground windows last
[[windows]]
name = "overlay"
# ...
```

### Windows Cut Off

Check terminal size vs layout dimensions:

```toml
# Use percentages for safety
width = "50%"   # Instead of: width = 80

# Or check minimum sizes
min_width = 20
min_height = 5
```

### Responsive Issues

Test with percentages:

```toml
# Good: Adapts to terminal size
row = 0
col = "60%"
width = "40%"
height = "100%"

# Risky: Fixed size may not fit
row = 0
col = 100
width = 60
height = 50
```

## Layout Examples

### Minimalist

```toml
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = "100%"
height = "95%"
show_border = false

[[windows]]
name = "input"
type = "command_input"
row = "95%"
col = 0
width = "100%"
height = "5%"
show_border = false
```

### Information Dense

See the [Hunting Setup Tutorial](../tutorials/hunting-setup.md) for a complex combat layout.

### Roleplay Focused

See the [Roleplay Setup Tutorial](../tutorials/roleplay-setup.md) for a communication-focused layout.

## See Also

- [Layout Configuration](../configuration/layout-toml.md) - Full reference
- [Widgets Reference](../widgets/README.md) - All widget types
- [Your First Layout](../tutorials/your-first-layout.md) - Tutorial

