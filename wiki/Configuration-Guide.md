# Configuration Guide

This guide covers all configuration options for vellum-fe, including the config file structure, window definitions, colors, presets, highlights, and keybinds.

## Table of Contents

- [Configuration Files](#configuration-files)
- [Connection Settings](#connection-settings)
- [UI Settings](#ui-settings)
- [Preset Colors](#preset-colors)
- [Highlights](#highlights)
- [Keybinds](#keybinds)
- [Window Definitions](#window-definitions)
- [Color Format Reference](#color-format-reference)
- [Complete Example](#complete-example)

---

## Configuration Files

### Location

**Main Config:** `~/.vellum-fe/config.toml`
**Layouts:** `~/.vellum-fe/layouts/<name>.toml`
**Debug Logs:** `~/.vellum-fe/debug.log`

On Windows, `~` expands to your user directory (e.g., `C:\Users\YourName`)

### First Run

On first launch, vellum-fe creates a default `config.toml` with sensible defaults. You can edit this file with any text editor.

### Reloading Configuration

Most configuration changes require restarting the application. However, some changes can be made at runtime using dot commands (see [Commands Reference](Commands-Reference.md)).

---

## Connection Settings

Configure the connection to the Lich server.

```toml
[connection]
host = "127.0.0.1"
port = 8000
```

### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `host` | string | `"127.0.0.1"` | Hostname or IP address of Lich server |
| `port` | integer | `8000` | Port number for Lich detached mode |

### Example

```toml
[connection]
host = "localhost"
port = 8000
```

---

## UI Settings

General UI configuration options.

```toml
[ui]
command_echo_color = "#ffffff"
mouse_mode_toggle_key = "F11"
countdown_icon = "\u{f0c8}"
```

### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `command_echo_color` | string | `"#ffffff"` | Color for echoed commands in hex format |
| `mouse_mode_toggle_key` | string | `"F11"` | Key to toggle mouse mode on/off |
| `countdown_icon` | string | `"\u{f0c8}"` | Unicode character for countdown timer fill (Nerd Font icon) |

### Prompt Colors

Configure colors for different characters in the game prompt.

```toml
[[ui.prompt_colors]]
character = "R"
color = "#ff0000"

[[ui.prompt_colors]]
character = ">"
color = "#00ff00"
```

**Common Prompt Characters:**
- `R` - Roundtime indicator (red)
- `*` - Stunned indicator (yellow)
- `>` - Ready prompt (green)

---

## Preset Colors

Presets define colors for different types of game text. These map to `<preset id="...">` XML tags in the game stream.

```toml
[[presets]]
id = "speech"
fg = "#53a684"

[[presets]]
id = "whisper"
fg = "#80d4ff"

[[presets]]
id = "thought"
fg = "#d7ff80"
```

### Options

| Option | Type | Required | Description |
|--------|------|----------|-------------|
| `id` | string | Yes | Preset identifier (matches XML tag) |
| `fg` | string | No | Foreground color in hex format |
| `bg` | string | No | Background color in hex format |

### Common Presets

```toml
[[presets]]
id = "speech"
fg = "#53a684"

[[presets]]
id = "whisper"
fg = "#80d4ff"

[[presets]]
id = "thought"
fg = "#d7ff80"

[[presets]]
id = "roomName"
fg = "#ffffff"
bg = "#0000ff"

[[presets]]
id = "bold"
fg = "#ffffff"

[[presets]]
id = "damage"
fg = "#ff0000"

[[presets]]
id = "heal"
fg = "#00ff00"
```

---

## Highlights

**Note:** Highlighting is not yet fully implemented. This section describes the planned configuration format.

Highlights apply custom colors and styles to text matching specific patterns.

```toml
[[highlights]]
pattern = "^You.*"
fg = "#ffff00"
bold = true

[[highlights]]
pattern = "\\d+ silver"
fg = "#c0c0c0"

[[highlights]]
pattern = "\\[.*?\\]"
fg = "#00ffff"
```

### Options (Planned)

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `pattern` | string | Required | Regex pattern to match |
| `fg` | string | - | Foreground color |
| `bg` | string | - | Background color |
| `bold` | boolean | false | Apply bold styling |
| `underline` | boolean | false | Apply underline |
| `priority` | integer | 0 | Priority for overlapping highlights (higher = wins) |

---

## Keybinds

**Note:** Keybinds are not yet fully implemented. This section describes the planned configuration format.

Keybinds map key combinations to commands or actions.

```toml
[[keybinds]]
key = "f1"
command = "look"

[[keybinds]]
key = "ctrl+r"
command = "recall"

[[keybinds]]
key = "num_8"
command = "north"
```

### Options (Planned)

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `key` | string | Required | Key combination (see Key Reference below) |
| `command` | string | Required | Command to execute |
| `modifier` | string | - | Modifier keys: `ctrl`, `alt`, `shift` |

### Key Reference

**Function Keys:** `f1`, `f2`, ..., `f12`
**Number Pad:** `num_0` through `num_9`, `num_.`, `num_+`, `num_-`, `num_*`, `num_/`
**Arrow Keys:** `up`, `down`, `left`, `right`
**Special Keys:** `enter`, `backspace`, `delete`, `tab`, `esc`, `space`, `home`, `end`, `page_up`, `page_down`
**Modifiers:** Prefix with `ctrl+`, `alt+`, or `shift+`

**Examples:**
- `f1` - Function key 1
- `ctrl+c` - Ctrl + C
- `alt+f4` - Alt + F4
- `shift+tab` - Shift + Tab
- `num_8` - Number pad 8

---

## Window Definitions

Windows are defined in the `[[ui.windows]]` array. Each window specifies its type, position, size, and behavior.

```toml
[[ui.windows]]
name = "main"
widget_type = "text"
streams = ["main"]
row = 0
col = 0
rows = 30
cols = 120
buffer_size = 10000
show_border = true
border_style = "single"
title = "Main"
```

### Common Options

| Option | Type | Required | Description |
|--------|------|----------|-------------|
| `name` | string | Yes | Unique identifier for the window |
| `widget_type` | string | Yes | Widget type: `text`, `progress`, `countdown`, `compass`, `injuries`, `indicator`, `dashboard`, `activeeffects`, `hand`, `hands`, `scrollable` |
| `row` | integer | Yes | Top-left row position (0-based) |
| `col` | integer | Yes | Top-left column position (0-based) |
| `rows` | integer | Yes | Height in rows |
| `cols` | integer | Yes | Width in columns |
| `show_border` | boolean | No | Show window border (default: true) |
| `border_style` | string | No | Border style: `single`, `double`, `rounded`, `thick`, `none` |
| `border_color` | string | No | Border color in hex format |
| `title` | string | No | Window title (shown in border) |
| `transparent_background` | boolean | No | Transparent background (default: true) |

### Text Window Options

For `widget_type = "text"`:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `streams` | array | `["main"]` | Stream names to route to this window |
| `buffer_size` | integer | 1000 | Maximum lines to keep in scrollback |

**Example:**
```toml
[[ui.windows]]
name = "thoughts"
widget_type = "text"
streams = ["thoughts"]
row = 0
col = 80
rows = 20
cols = 40
buffer_size = 5000
show_border = true
border_style = "rounded"
title = "Thoughts"
```

### Progress Bar Options

For `widget_type = "progress"`:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `bar_color` | string | `"#00ff00"` | Color of filled portion |
| `bar_background_color` | string | `"#333333"` | Color of unfilled portion |

**Example:**
```toml
[[ui.windows]]
name = "health"
widget_type = "progress"
row = 30
col = 0
rows = 1
cols = 20
bar_color = "#00ff00"
bar_background_color = "#330000"
show_border = false
```

### Countdown Timer Options

For `widget_type = "countdown"`:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `bar_color` | string | - | Color of countdown fill |

**Example:**
```toml
[[ui.windows]]
name = "roundtime"
widget_type = "countdown"
row = 31
col = 0
rows = 1
cols = 20
bar_color = "#ff0000"
show_border = false
```

### Compass Options

For `widget_type = "compass"`:

No additional options required.

**Example:**
```toml
[[ui.windows]]
name = "compass"
widget_type = "compass"
row = 0
col = 60
rows = 7
cols = 15
show_border = true
border_style = "double"
title = "Compass"
```

### Injury Doll Options

For `widget_type = "injuries"`:

No additional options required.

**Example:**
```toml
[[ui.windows]]
name = "injuries"
widget_type = "injuries"
row = 8
col = 60
rows = 16
cols = 30
show_border = true
border_style = "rounded"
title = "Injuries"
```

### Indicator Options

For `widget_type = "indicator"`:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `indicator_colors` | object | - | Colors for active/inactive states |

**Example:**
```toml
[[ui.windows]]
name = "poisoned"
widget_type = "indicator"
row = 32
col = 0
rows = 1
cols = 12
show_border = false
title = "POISONED"

[ui.windows.indicator_colors]
active = "#00ff00"
inactive = "#333333"
```

### Dashboard Options

For `widget_type = "dashboard"`:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `dashboard_layout` | string | `"horizontal"` | Layout: `horizontal` or `vertical` |
| `dashboard_indicators` | array | `[]` | List of indicator names to include |
| `dashboard_spacing` | integer | 1 | Space between indicators |
| `dashboard_hide_inactive` | boolean | false | Hide inactive indicators |

**Example:**
```toml
[[ui.windows]]
name = "status"
widget_type = "dashboard"
row = 32
col = 0
rows = 1
cols = 60
dashboard_layout = "horizontal"
dashboard_indicators = ["poisoned", "diseased", "bleeding", "stunned", "webbed"]
dashboard_spacing = 2
dashboard_hide_inactive = true
show_border = false
```

### Active Effects Options

For `widget_type = "activeeffects"`:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `visible_count` | integer | - | Maximum number of effects to display |
| `effect_category` | string | - | Filter by category: `all`, `buffs`, `debuffs` |

**Example:**
```toml
[[ui.windows]]
name = "activeeffects"
widget_type = "activeeffects"
row = 0
col = 100
rows = 15
cols = 30
visible_count = 20
show_border = true
border_style = "rounded"
title = "Active Effects"
```

---

## Color Format Reference

All colors in vellum-fe use hexadecimal RGB format.

### Format

`#RRGGBB` where:
- `RR` = Red component (00-FF)
- `GG` = Green component (00-FF)
- `BB` = Blue component (00-FF)

### Examples

```toml
fg = "#ff0000"  # Pure red
fg = "#00ff00"  # Pure green
fg = "#0000ff"  # Pure blue
fg = "#ffffff"  # White
fg = "#000000"  # Black
fg = "#808080"  # Gray
fg = "#ffff00"  # Yellow
fg = "#00ffff"  # Cyan
fg = "#ff00ff"  # Magenta
fg = "#ffa500"  # Orange
fg = "#800080"  # Purple
```

### Tips

- Use online color pickers to generate hex codes
- Test colors with `.setbarcolor` command
- Terminal color support varies by terminal emulator
- Some terminals support full 24-bit color (16.7 million colors)

---

## Complete Example

Here's a complete `config.toml` with common configurations:

```toml
[connection]
host = "127.0.0.1"
port = 8000

[ui]
command_echo_color = "#ffffff"
mouse_mode_toggle_key = "F11"
countdown_icon = "\u{f0c8}"

[[ui.prompt_colors]]
character = "R"
color = "#ff0000"

[[ui.prompt_colors]]
character = "*"
color = "#ffff00"

[[ui.prompt_colors]]
character = ">"
color = "#00ff00"

# Preset colors
[[presets]]
id = "speech"
fg = "#53a684"

[[presets]]
id = "whisper"
fg = "#80d4ff"

[[presets]]
id = "thought"
fg = "#d7ff80"

[[presets]]
id = "roomName"
fg = "#ffffff"
bg = "#0000ff"

# Main text window
[[ui.windows]]
name = "main"
widget_type = "text"
streams = ["main"]
row = 0
col = 0
rows = 28
cols = 100
buffer_size = 10000
show_border = true
border_style = "single"
title = "Main"

# Thoughts window
[[ui.windows]]
name = "thoughts"
widget_type = "text"
streams = ["thoughts"]
row = 0
col = 100
rows = 28
cols = 40
buffer_size = 5000
show_border = true
border_style = "rounded"
title = "Thoughts"

# Health bar
[[ui.windows]]
name = "health"
widget_type = "progress"
row = 28
col = 0
rows = 1
cols = 20
bar_color = "#00ff00"
bar_background_color = "#330000"
show_border = false

# Mana bar
[[ui.windows]]
name = "mana"
widget_type = "progress"
row = 28
col = 20
rows = 1
cols = 20
bar_color = "#0000ff"
bar_background_color = "#000033"
show_border = false

# Roundtime
[[ui.windows]]
name = "roundtime"
widget_type = "countdown"
row = 29
col = 0
rows = 1
cols = 20
bar_color = "#ff0000"
show_border = false
```

---

[← Previous: Commands Reference](Commands-Reference.md) | [Next: Stream Routing →](Stream-Routing.md)
