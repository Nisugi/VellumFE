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
countdown_icon = "\u{f0c8}"
# Text selection settings
selection_enabled = true
selection_respect_window_boundaries = true
selection_bg_color = "#4a4a4a"
# Drag and drop settings
drag_modifier_key = "ctrl"
```

### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `command_echo_color` | string | `"#ffffff"` | Color for echoed commands in hex format |
| `countdown_icon` | string | `"\u{f0c8}"` | Unicode character for countdown timer fill (Nerd Font icon) |
| `selection_enabled` | boolean | `true` | Enable VellumFE text selection (click and drag to select) |
| `selection_respect_window_boundaries` | boolean | `true` | Prevent selection from spanning across multiple windows |
| `selection_bg_color` | string | `"#4a4a4a"` | Background color for selected text (for future visual highlighting) |
| `drag_modifier_key` | string | `"ctrl"` | Modifier key required for drag-and-drop: "ctrl", "alt", "shift", or "none" |

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

Highlights apply custom colors and styles to text matching specific patterns. Fully implemented with Aho-Corasick optimization for fast pattern matching and optional sound support.

For a complete guide with in-app management commands, see [Highlight Management](Highlight-Management.md).

```toml
[highlights]
# Example combat highlight
swing = { pattern = "You swing.*", fg = "#ff0000", bold = true }

# Highlight player names in magenta (FAST with Aho-Corasick!)
friends = { pattern = "Mandrill|Monolis", fg = "#ff00ff", bold = true, fast_parse = true }

# Highlight with sound
death_alert = { pattern = ".*dies.*", fg = "#ffffff", bg = "#ff0000", sound = "death.wav" }
```

### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `pattern` | string | Required | Regex pattern to match |
| `fg` | string | - | Foreground color |
| `bg` | string | - | Background color |
| `bold` | boolean | false | Apply bold styling |
| `color_entire_line` | boolean | false | Color entire line when pattern matches |
| `fast_parse` | boolean | false | Use Aho-Corasick for literal pattern matching (much faster) |
| `sound` | string | - | Sound file to play (in ~/.vellum-fe/sounds/) |
| `sound_volume` | float | 0.7 | Volume for this sound (0.0-1.0) |

---

## Keybinds

Keybinds map key combinations to commands or actions. Fully implemented with 24 built-in actions and macro support.

For a complete guide with in-app management commands, see [Keybind Management](Keybind-Management.md).

```toml
[keybinds]
# Built-in action
f12 = "toggle_performance_stats"

# Macro with \r for enter
num_8 = { macro_text = "n\r" }

# With modifiers
"ctrl+f" = "start_search"
```

### Options

Keybinds can be either:
- A string for built-in actions: `f12 = "toggle_performance_stats"`
- A table for macros: `num_8 = { macro_text = "n\r" }`

**Built-in Actions** (24 available):
See [Keybind Management](Keybind-Management.md#built-in-actions) for complete list.

**Macro format:**
- `macro_text` - Text to send, use `\r` for Enter key

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
| `background_color` | string | No | Background color in hex format (e.g., `#1a1a1a`) |
| `content_align` | string | No | Content alignment: `top-left`, `top`, `top-right`, `left`, `center`, `right`, `bottom-left`, `bottom`, `bottom-right` |

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

### Content Alignment

The `content_align` option positions widget content within the window area. This is especially useful when borders are removed and you want extra space around widgets.

**Supported Widgets:**
- **Compass** - 7x3 fixed size content
- **InjuryDoll** - 5x6 fixed size content
- **ProgressBar** - 1 row height (vertical alignment only)

**Alignment Options:**

| Value | Description |
|-------|-------------|
| `top-left` | Align to top-left corner (default) |
| `top` | Center horizontally, align to top |
| `top-right` | Align to top-right corner |
| `left` | Center vertically, align to left |
| `center` | Center both horizontally and vertically |
| `right` | Center vertically, align to right |
| `bottom-left` | Align to bottom-left corner |
| `bottom` | Center horizontally, align to bottom |
| `bottom-right` | Align to bottom-right corner |

**Examples:**

```toml
# Compass with no border, aligned to bottom-left with transparent space above
[[windows]]
name = "compass"
widget_type = "compass"
row = 65
col = 1
rows = 8
cols = 10
show_border = false
content_align = "bottom-left"

# Progress bars with 3-row height, bars aligned to bottom
[[windows]]
name = "health"
widget_type = "progress"
row = 30
col = 0
rows = 3
cols = 20
show_border = false
content_align = "bottom"
bar_color = "#00ff00"

# Injury doll centered in larger area
[[windows]]
name = "injuries"
widget_type = "injury_doll"
row = 40
col = 100
rows = 10
cols = 10
show_border = false
content_align = "center"
```

**Notes:**
- Content alignment only affects widgets when the window area is larger than the content
- When borders are enabled, alignment is relative to the inner area (inside the border)
- Transparent space around aligned content remains transparent unless `background_color` is set
- Use `.contentalign` command to change alignment at runtime

### Background Color

The `background_color` option fills the entire widget area with a color. Useful for making borderless windows more visible.

**Examples:**

```toml
# Command input with dark gray background
[[windows]]
name = "command"
widget_type = "text"
row = 70
col = 0
rows = 1
cols = 156
show_border = false
background_color = "#1a1a1a"

# Compass with dark blue background
[[windows]]
name = "compass"
widget_type = "compass"
row = 65
col = 1
rows = 5
cols = 10
show_border = false
content_align = "bottom-left"
background_color = "#000033"
```

**Notes:**
- Works with all widget types
- When not set, widgets have transparent backgrounds
- Background fills the entire widget area, not just the content
- Use `.background` command to change color at runtime

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
countdown_icon = "\u{f0c8}"
drag_modifier_key = "ctrl"

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
