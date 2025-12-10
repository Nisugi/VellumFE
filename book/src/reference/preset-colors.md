# Preset Colors

Reference of all named color presets.

## Basic Colors

| Name | Hex Code | Preview |
|------|----------|---------|
| `black` | `#000000` | ■ Black |
| `red` | `#800000` | ■ Red |
| `green` | `#008000` | ■ Green |
| `yellow` | `#808000` | ■ Yellow |
| `blue` | `#000080` | ■ Blue |
| `magenta` | `#800080` | ■ Magenta |
| `cyan` | `#008080` | ■ Cyan |
| `white` | `#c0c0c0` | ■ White |

## Bright Colors

| Name | Hex Code | Preview |
|------|----------|---------|
| `bright_black` / `gray` | `#808080` | ■ Gray |
| `bright_red` | `#ff0000` | ■ Bright Red |
| `bright_green` | `#00ff00` | ■ Bright Green |
| `bright_yellow` | `#ffff00` | ■ Bright Yellow |
| `bright_blue` | `#0000ff` | ■ Bright Blue |
| `bright_magenta` | `#ff00ff` | ■ Bright Magenta |
| `bright_cyan` | `#00ffff` | ■ Bright Cyan |
| `bright_white` | `#ffffff` | ■ Bright White |

## Alternative Names

| Alternative | Maps To |
|-------------|---------|
| `grey` | `gray` |
| `dark_gray` | `bright_black` |
| `light_gray` | `white` |

## Semantic Colors

### Vitals

| Name | Default | Purpose |
|------|---------|---------|
| `health` | `#00ff00` | Health bar |
| `health_low` | `#ffff00` | Low health warning |
| `health_critical` | `#ff0000` | Critical health |
| `mana` | `#0080ff` | Mana bar |
| `stamina` | `#ff8000` | Stamina bar |
| `spirit` | `#ff00ff` | Spirit bar |

### Status Indicators

| Name | Default | Purpose |
|------|---------|---------|
| `hidden` | `#00ff00` | Hidden status |
| `invisible` | `#00ffff` | Invisible status |
| `stunned` | `#ffff00` | Stunned status |
| `webbed` | `#ff00ff` | Webbed status |
| `prone` | `#00ffff` | Prone status |
| `kneeling` | `#ff8000` | Kneeling status |
| `sitting` | `#808080` | Sitting status |
| `dead` | `#ff0000` | Dead status |

### Streams

| Name | Default | Purpose |
|------|---------|---------|
| `main` | `#ffffff` | Main text |
| `room` | `#ffff00` | Room descriptions |
| `combat` | `#ff4444` | Combat messages |
| `speech` | `#00ffff` | Player speech |
| `whisper` | `#ff00ff` | Whispers |
| `thoughts` | `#00ff00` | ESP/thoughts |

### UI Elements

| Name | Default | Purpose |
|------|---------|---------|
| `border` | `#404040` | Widget borders |
| `border_focused` | `#ffffff` | Focused widget border |
| `background` | `#000000` | Background |
| `text` | `#ffffff` | Default text |
| `text_dim` | `#808080` | Dimmed text |

## Game Presets

Colors matching game highlight presets:

| Preset ID | Name | Color |
|-----------|------|-------|
| `speech` | Speech | `#00ffff` |
| `thought` | Thoughts | `#00ff00` |
| `whisper` | Whispers | `#ff00ff` |
| `bold` | Bold text | Inherits + bold |
| `roomName` | Room name | `#ffff00` |
| `roomDesc` | Room description | `#c0c0c0` |

## Using Colors

### In colors.toml

```toml
[theme]
# Use preset names
health = "bright_green"
mana = "bright_blue"

# Or hex codes
border = "#333333"
text = "#e0e0e0"
```

### In highlights.toml

```toml
[[highlights]]
pattern = "something"
fg = "bright_red"        # Preset name
bg = "#000080"           # Hex code
```

### In widgets

```toml
[[widgets]]
type = "progress"
color = "health"         # Semantic color
border_color = "cyan"    # Basic color
```

## Hex Color Format

| Format | Example | Notes |
|--------|---------|-------|
| 6-digit | `#ff0000` | Full RGB |
| 3-digit | `#f00` | Shorthand (expands to `#ff0000`) |

## Color Selection Tips

### High Contrast

For accessibility, use high-contrast combinations:

```toml
# Good contrast
text = "#ffffff"
background = "#000000"

# Also good
text = "#000000"
background = "#ffffff"
```

### Colorblind-Friendly

Avoid relying solely on red/green distinction:

```toml
# Instead of red/green
health = "#00ff00"     # Green
danger = "#ff0000"     # Red

# Use additional cues
health = "#00ffff"     # Cyan
danger = "#ff00ff"     # Magenta
```

### Thematic Palettes

**Nord Theme**:
```toml
background = "#2e3440"
text = "#eceff4"
red = "#bf616a"
green = "#a3be8c"
blue = "#81a1c1"
```

**Solarized Dark**:
```toml
background = "#002b36"
text = "#839496"
red = "#dc322f"
green = "#859900"
blue = "#268bd2"
```

**Dracula**:
```toml
background = "#282a36"
text = "#f8f8f2"
red = "#ff5555"
green = "#50fa7b"
blue = "#8be9fd"
```

## Color Inheritance

Some colors inherit or derive from others:

```toml
# Base color
text = "#ffffff"

# Derived (if not specified)
text_dim = text * 0.5    # Dimmer version
```

## See Also

- [Creating Themes](../customization/creating-themes.md)
- [Colors Configuration](../configuration/colors-toml.md)
- [Accessibility Setup](../tutorials/accessibility.md)

