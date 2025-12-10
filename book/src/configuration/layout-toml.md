# layout.toml Reference

The layout configuration file defines all windows, their positions, sizes, and visual properties.

## Location

`~/.vellum-fe/layout.toml`

---

## Structure Overview

```toml
# Global layout settings
[layout]
columns = 120
rows = 40

# Window definitions
[[windows]]
name = "main"
type = "text"
# ... window properties

[[windows]]
name = "room"
type = "room"
# ... window properties
```

---

## [layout] Section

Global layout dimensions and defaults.

```toml
[layout]
# Terminal grid size
columns = 120        # Total columns
rows = 40            # Total rows

# Default window settings
default_border = true
default_border_style = "rounded"
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `columns` | integer | `120` | Layout width in columns |
| `rows` | integer | `40` | Layout height in rows |
| `default_border` | boolean | `true` | Default border visibility |
| `default_border_style` | string | `"rounded"` | Default border style |

---

## [[windows]] Array

Each `[[windows]]` entry defines a single window.

### Common Properties

These properties apply to all window types:

```toml
[[windows]]
# Identity
name = "main"              # Unique identifier (required)
type = "text"              # Widget type (required)

# Position (choose one method)
row = 0                    # Row position
col = 0                    # Column position
# OR
x = 0                      # Pixel X position
y = 0                      # Pixel Y position

# Size
width = 80                 # Width in columns
height = 30                # Height in rows
# OR percentage
width = "60%"              # Percentage of parent width
height = "80%"             # Percentage of parent height

# Visual
show_border = true         # Show window border
border_style = "rounded"   # Border style
border_sides = "all"       # Which sides have borders
show_title = true          # Show title bar
title = "Main Window"      # Custom title (default: name)
transparent_background = false  # Transparent background

# Colors (override theme)
border_color = "#5588AA"   # Border color
background_color = "#000000"  # Background color
text_color = "#CCCCCC"     # Text color
```

### Position Properties

| Key | Type | Description |
|-----|------|-------------|
| `row` | integer | Row position (0 = top) |
| `col` | integer | Column position (0 = left) |
| `x` | integer | Pixel X position |
| `y` | integer | Pixel Y position |

**Note**: Use `row`/`col` for grid-based layouts, `x`/`y` for pixel-precise positioning.

### Size Properties

| Key | Type | Description |
|-----|------|-------------|
| `width` | integer or string | Width in columns or percentage |
| `height` | integer or string | Height in rows or percentage |

**Percentage sizing:**
```toml
width = "50%"    # 50% of layout width
height = "100%"  # Full layout height
```

### Border Properties

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `show_border` | boolean | `true` | Show border |
| `border_style` | string | `"rounded"` | Border style |
| `border_sides` | string | `"all"` | Which sides |
| `border_color` | string | theme | Border color |

**Border sides:**
- `"all"` - All four sides
- `"none"` - No borders
- `"top"` - Top only
- `"bottom"` - Bottom only
- `"left"` - Left only
- `"right"` - Right only
- `"top,bottom"` - Specific sides (comma-separated)
- `"horizontal"` - Top and bottom
- `"vertical"` - Left and right

---

## Window Types

### Text Window

Scrollable text display for game output.

```toml
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = 80
height = 30

# Text-specific options
stream = "main"           # Game stream to display
buffer_size = 2000        # Maximum lines to buffer
word_wrap = true          # Enable word wrapping
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `stream` | string | same as name | Game stream to display |
| `buffer_size` | integer | `2000` | Max buffered lines |
| `word_wrap` | boolean | `true` | Enable word wrap |

**Common streams:** `main`, `speech`, `thoughts`, `combat`, `death`, `logons`, `familiar`, `group`

### Tabbed Text Window

Multiple streams in tabbed interface.

```toml
[[windows]]
name = "channels"
type = "tabbedtext"
row = 0
col = 80
width = 40
height = 30

# Tab configuration
tabs = ["speech", "thoughts", "combat"]
default_tab = "speech"
show_tab_bar = true
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `tabs` | array | `[]` | List of stream names |
| `default_tab` | string | first tab | Initially active tab |
| `show_tab_bar` | boolean | `true` | Show tab headers |

### Command Input

Text input field for commands.

```toml
[[windows]]
name = "input"
type = "command_input"
row = 38
col = 0
width = 120
height = 2

# Input-specific options
prompt = "> "
history = true
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `prompt` | string | `"> "` | Input prompt |
| `history` | boolean | `true` | Enable command history |

### Progress Bar

Displays a value as a filled bar.

```toml
[[windows]]
name = "health"
type = "progress"
row = 0
col = 80
width = 20
height = 1

# Progress-specific options
stat = "health"           # Stat to track
show_value = true         # Show "425/500"
show_percentage = false   # Show "85%"
bar_color = "#00FF00"     # Bar fill color
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `stat` | string | required | Stat ID (`health`, `mana`, etc.) |
| `show_value` | boolean | `true` | Show numeric value |
| `show_percentage` | boolean | `false` | Show percentage |
| `bar_color` | string | theme | Bar fill color |

**Stat IDs:** `health`, `mana`, `spirit`, `stamina`, `encumbrance`

### Countdown

Countdown timer display.

```toml
[[windows]]
name = "roundtime"
type = "countdown"
row = 2
col = 80
width = 20
height = 1

# Countdown-specific options
countdown_type = "roundtime"  # "roundtime" or "casttime"
show_seconds = true
icon = "⏱"
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `countdown_type` | string | required | `roundtime` or `casttime` |
| `show_seconds` | boolean | `true` | Show remaining seconds |
| `icon` | string | `""` | Icon to display |

### Compass

Directional compass display.

```toml
[[windows]]
name = "compass"
type = "compass"
row = 5
col = 100
width = 10
height = 5

# Compass-specific options
style = "graphical"       # "graphical" or "text"
show_labels = true        # Show N/S/E/W labels
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `style` | string | `"graphical"` | Display style |
| `show_labels` | boolean | `true` | Show direction labels |

### Hand

Left/right hand or spell display.

```toml
[[windows]]
name = "right_hand"
type = "hand"
row = 10
col = 100
width = 15
height = 1

# Hand-specific options
hand_type = "right"       # "left", "right", or "spell"
icon = "🤚"
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `hand_type` | string | required | `left`, `right`, or `spell` |
| `icon` | string | `""` | Icon to display |

### Indicator

Status indicator widget.

```toml
[[windows]]
name = "status"
type = "indicator"
row = 12
col = 100
width = 20
height = 1

# Indicator-specific options
indicators = ["stunned", "hidden", "poisoned"]
style = "icons"           # "icons" or "text"
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `indicators` | array | all | Which indicators to show |
| `style` | string | `"icons"` | Display style |

**Indicator IDs:** `stunned`, `hidden`, `webbed`, `poisoned`, `diseased`, `bleeding`, `prone`, `kneeling`, `sitting`

### Room Window

Room description display.

```toml
[[windows]]
name = "room"
type = "room"
row = 0
col = 80
width = 40
height = 10

# Room-specific options
show_title = true         # Show room name
show_description = true   # Show room desc
show_exits = true         # Show obvious exits
show_objects = true       # Show items/creatures
show_players = true       # Show other players
```

### Injury Doll

Body injury display.

```toml
[[windows]]
name = "injuries"
type = "injury_doll"
row = 15
col = 100
width = 15
height = 10

# Injury-specific options
show_labels = true
compact = false
```

### Performance Monitor

Performance metrics display.

```toml
[[windows]]
name = "performance"
type = "performance"
row = 0
col = 0
width = 30
height = 15

# Which metrics to show
show_fps = true
show_frame_times = true
show_render_times = true
show_net = true
show_parse = true
show_memory = true
show_lines = true
show_uptime = true
```

---

## Layout Examples

### Basic Two-Column Layout

```toml
[layout]
columns = 120
rows = 40

[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = 80
height = 38

[[windows]]
name = "room"
type = "room"
row = 0
col = 80
width = 40
height = 15

[[windows]]
name = "vitals"
type = "progress"
stat = "health"
row = 15
col = 80
width = 40
height = 1

[[windows]]
name = "input"
type = "command_input"
row = 38
col = 0
width = 120
height = 2
```

### Hunting Layout

See [Hunting Setup Tutorial](../tutorials/hunting-setup.md) for a complete example.

### Minimal Layout

```toml
[layout]
columns = 80
rows = 24

[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = 80
height = 22
show_border = false

[[windows]]
name = "input"
type = "command_input"
row = 22
col = 0
width = 80
height = 2
show_border = false
```

---

## See Also

- [Creating Layouts](../customization/creating-layouts.md) - Layout design guide
- [Widget Reference](../widgets/README.md) - Detailed widget documentation
- [Tutorials](../tutorials/README.md) - Complete layout examples
