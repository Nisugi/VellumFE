# Widgets

Widgets are the visual building blocks of VellumFE. Each widget type displays a specific kind of game information.

## Overview

VellumFE provides **19 widget types**:

| Widget | Type | Purpose |
|--------|------|---------|
| [Text Window](./text-windows.md) | `text` | Scrollable game text |
| [Tabbed Text](./tabbed-text.md) | `tabbedtext` | Multiple streams in tabs |
| [Command Input](./command-input.md) | `command_input` | Command entry field |
| [Progress Bar](./progress-bars.md) | `progress` | Health, mana, etc. |
| [Countdown](./countdowns.md) | `countdown` | RT, cast time |
| [Compass](./compass.md) | `compass` | Available exits |
| [Hand](./hands.md) | `hand` | Items in hands |
| [Indicator](./indicators.md) | `indicator` | Status conditions |
| [Injury Doll](./injury-doll.md) | `injury_doll` | Body injuries |
| [Active Effects](./active-effects.md) | `active_effects` | Buffs/debuffs |
| [Room Window](./room-window.md) | `room` | Room description |
| [Inventory](./inventory.md) | `inventory` | Item list |
| [Spells](./spells.md) | `spells` | Known spells |
| [Dashboard](./dashboard.md) | `dashboard` | Composite status |
| [Performance](./performance.md) | `performance` | Debug metrics |
| Targets | `targets` | Combat target list |
| Players | `players` | Players in room |
| Spacer | `spacer` | Layout spacing (1×1 min) |
| Map | `map` | Map display (future) |

---

## Common Properties

All widgets share these properties in `layout.toml`:

### Identity

```toml
[[windows]]
name = "my_widget"     # Unique identifier (required)
type = "text"          # Widget type (required)
```

### Position

```toml
# Grid-based positioning
row = 0                # Row (0 = top)
col = 0                # Column (0 = left)

# OR pixel positioning
x = 100                # X coordinate
y = 50                 # Y coordinate
```

### Size

```toml
# Fixed size
width = 40             # Columns wide
height = 10            # Rows tall

# OR percentage
width = "50%"          # Half of layout width
height = "100%"        # Full layout height
```

### Borders

```toml
show_border = true           # Display border
border_style = "rounded"     # Style: plain, rounded, double, thick
border_sides = "all"         # Which sides: all, none, top, bottom, etc.
border_color = "#5588AA"     # Border color
```

### Title

```toml
show_title = true            # Show title bar
title = "Custom Title"       # Override default title
```

### Colors

```toml
background_color = "#000000" # Background
text_color = "#CCCCCC"       # Default text
transparent_background = false  # See-through background
```

---

## Widget Categories

### Text Displays

Widgets that show game text:
- **Text Window** - Main game output, speech, thoughts
- **Tabbed Text** - Multiple streams in one widget
- **Room Window** - Room name, description, exits

### Status Displays

Widgets that show character status:
- **Progress Bar** - Health, mana, stamina, spirit
- **Countdown** - Roundtime, cast time
- **Hand** - Left hand, right hand, spell
- **Indicator** - Stunned, hidden, webbed, etc.
- **Injury Doll** - Body part injuries
- **Active Effects** - Spells, buffs, cooldowns
- **Dashboard** - Configurable multi-indicator panel

### Navigation

Widgets for game navigation:
- **Compass** - Available exits with directions

### Lists

Widgets that display lists:
- **Inventory** - Items carried
- **Spells** - Known spells

### Entity Displays

Widgets showing creatures and players:
- **Targets** - Combat targets with current-target highlighting
- **Players** - Other players in room with stance/status suffixes

### Utility

Layout and debugging widgets:
- **Spacer** - Layout spacing widget (minimum 1×1)
- **Performance** - Frame rate, memory, network stats

---

## Stream Mapping

Text-based widgets receive data from game streams:

| Stream | Content | Default Widget |
|--------|---------|----------------|
| `main` | Primary game output | Main text window |
| `speech` | Player dialogue | Speech tab/window |
| `thoughts` | ESP/telepathy | Thoughts tab/window |
| `combat` | Combat messages | Combat tab/window |
| `death` | Death messages | Death window |
| `logons` | Login/logout | Arrivals window |
| `familiar` | Familiar messages | Familiar window |
| `group` | Group information | Group window |
| `room` | Room data | Room window |
| `inv` | Inventory data | Inventory window |

Configure stream mapping in layout:

```toml
[[windows]]
name = "my_window"
type = "text"
stream = "speech"    # Display speech stream
```

---

## Creating Custom Layouts

Combine widgets to create your ideal interface:

```toml
[layout]
columns = 120
rows = 40

# Main game text (left side)
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = 80
height = 35

# Channels on right
[[windows]]
name = "channels"
type = "tabbed_text"
tabs = ["speech", "thoughts", "combat"]
row = 0
col = 80
width = 40
height = 20

# Status area
[[windows]]
name = "health"
type = "progress"
stat = "health"
row = 20
col = 80
width = 40
height = 1

# Command input at bottom
[[windows]]
name = "input"
type = "command_input"
row = 38
col = 0
width = 120
height = 2
```

---

## Widget Interaction

### Focus

- Click a widget to focus it
- Use `Ctrl+Tab` to cycle focus
- Focused widget has highlighted border

### Scrolling

- `Page Up` / `Page Down` - Scroll focused text widget
- Mouse wheel - Scroll widget under cursor
- `Home` / `End` - Jump to top/bottom

### Selection

- Click and drag to select text
- `Ctrl+C` to copy selection
- Double-click to select word

### Links

If `links = true` in config:
- Click game objects to interact
- Right-click for context menu

---

## Performance Considerations

### Buffer Sizes

Text widgets buffer a limited number of lines:

```toml
[[windows]]
name = "main"
type = "text"
buffer_size = 2000    # Max lines (default)
```

Larger buffers use more memory. Reduce for secondary windows.

### Widget Count

Each widget has rendering overhead. For performance:
- Use tabbed windows instead of multiple text windows
- Disable unused widgets
- Reduce buffer sizes on secondary windows

---

## See Also

- [layout.toml Reference](../configuration/layout-toml.md) - Layout syntax
- [Creating Layouts](../customization/creating-layouts.md) - Design guide
- [Performance](../architecture/performance.md) - Optimization tips
