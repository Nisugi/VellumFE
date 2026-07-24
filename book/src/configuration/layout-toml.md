# layout.toml

Defines window positions, sizes, and properties for the **TUI**. (The
[Desktop GUI](../frontends/gui.md) keeps its own layout separately.)

Layouts are saved with `.savelayout [name]` to `~/.vellum-fe/layouts/`,
and the current layout auto-saves per character to
`profiles/<name>/layout.toml`.

## Basic Structure

```toml
terminal_width = 120
terminal_height = 40

[[windows]]
name = "main"
widget_type = "text"
row = 0
col = 0
rows = 37
cols = 120
```

## Window Properties

### Required

| Property | Type | Description |
|----------|------|-------------|
| `name` | string | Unique identifier |
| `widget_type` | string | Widget type (see [Widgets](../widgets/README.md)) |
| `row` | integer | Top row position (0 = top) |
| `col` | integer | Left column position (0 = left) |
| `rows` | integer | Height in rows |
| `cols` | integer | Width in columns |

### Optional

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `visible` | bool | `true` | Show window |
| `show_border` | bool | `true` | Draw border |
| `border_style` | string | `"single"` | `single`, `double`, `rounded`, `thick`, `quadrant_inside`, `quadrant_outside` |
| `border_color` | string | `"#808080"` | Border color |
| `border_sides` | array | all sides | Which sides to draw, e.g. `["top", "bottom"]`; `[]` for none |
| `title` | string | - | Custom title |
| `show_title` | bool | `true` | Show title in border |
| `title_position` | string | `"top-left"` | Where the title sits on the border |
| `buffer_size` | integer | 10000 | Lines to keep (text windows) |
| `background_color` | string | - | Background color |
| `text_color` | string | - | Default text color |
| `transparent_background` | bool | `false` | See-through background |

### Size Constraints

```toml
[[windows]]
name = "compass"
widget_type = "compass"
min_rows = 3
max_rows = 5
min_cols = 7
max_cols = 15
```

## Widget-Specific Properties

### Text Windows

```toml
[[windows]]
name = "main"
widget_type = "text"
streams = ["main"]              # Streams to display
buffer_size = 10000
compact = false                 # Remove blank lines
```

### Tabbed Text

```toml
[[windows]]
name = "channels"
widget_type = "tabbedtext"
buffer_size = 5000

[[windows.tabs]]
name = "Speech"
streams = ["speech"]
show_timestamps = true

[[windows.tabs]]
name = "Thoughts"
streams = ["thoughts"]
```

### Progress Bars

```toml
[[windows]]
name = "health"
widget_type = "progress"
id = "health"                   # health, mana, stamina, spirit, concentration, pbarStance
color = "#00FF00"
numbers_only = false            # true: show only current/max numbers
```

### Countdowns

```toml
[[windows]]
name = "roundtime"
widget_type = "countdown"
id = "roundtime"                # roundtime, casttime, stuntime
```

### Room Window

```toml
[[windows]]
name = "room"
widget_type = "room"
show_desc = true
show_objs = true
show_players = true
show_exits = true
show_name = true
```

## Example Layout

```toml
terminal_width = 160
terminal_height = 50

# Main game text - left side
[[windows]]
name = "main"
widget_type = "text"
streams = ["main"]
row = 0
col = 0
rows = 45
cols = 100
buffer_size = 10000

# Channels - right side
[[windows]]
name = "channels"
widget_type = "tabbedtext"
row = 0
col = 100
rows = 30
cols = 60
buffer_size = 2000

[[windows.tabs]]
name = "Speech"
streams = ["speech"]

[[windows.tabs]]
name = "Thoughts"
streams = ["thoughts"]

# Status bars
[[windows]]
name = "health"
widget_type = "progress"
id = "health"
row = 30
col = 100
rows = 1
cols = 60

# Command input - bottom
[[windows]]
name = "command_input"
widget_type = "command_input"
row = 47
col = 0
rows = 3
cols = 160
```

## Hidden Windows

Set `visible = false` to define windows that can be shown later via the menu:

```toml
[[windows]]
name = "society"
widget_type = "text"
streams = ["society"]
visible = false
# ... position and size still required
```

Show via: Menu → Windows → Add Window → Text Windows → Society
