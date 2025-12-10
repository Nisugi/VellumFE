# Color Codes Reference

Complete reference for colors in VellumFE.

## Color Formats

VellumFE supports multiple color format specifications:

### Hex Colors

6-digit or 3-digit hexadecimal:

```toml
color = "#ff0000"   # Red (6-digit)
color = "#f00"      # Red (3-digit shorthand)
color = "#FF0000"   # Case insensitive
```

### RGB Values

```toml
color = "rgb(255, 0, 0)"    # Red
color = "rgb(0, 128, 255)"  # Blue
```

### Named Colors

Standard web colors:

```toml
color = "red"
color = "darkblue"
color = "forestgreen"
```

### Preset Names

VellumFE specific presets:

```toml
color = "health"        # Health bar color
color = "mana"          # Mana bar color
color = "speech"        # Speech text color
```

## Standard Named Colors

### Basic Colors

| Name | Hex | Preview |
|------|-----|---------|
| `black` | `#000000` | ████ |
| `white` | `#ffffff` | ████ |
| `red` | `#ff0000` | ████ |
| `green` | `#00ff00` | ████ |
| `blue` | `#0000ff` | ████ |
| `yellow` | `#ffff00` | ████ |
| `cyan` | `#00ffff` | ████ |
| `magenta` | `#ff00ff` | ████ |

### Extended Colors

| Name | Hex | Preview |
|------|-----|---------|
| `orange` | `#ffa500` | ████ |
| `pink` | `#ffc0cb` | ████ |
| `purple` | `#800080` | ████ |
| `brown` | `#a52a2a` | ████ |
| `gray` / `grey` | `#808080` | ████ |
| `gold` | `#ffd700` | ████ |
| `silver` | `#c0c0c0` | ████ |
| `navy` | `#000080` | ████ |
| `teal` | `#008080` | ████ |
| `olive` | `#808000` | ████ |
| `maroon` | `#800000` | ████ |
| `lime` | `#00ff00` | ████ |
| `aqua` | `#00ffff` | ████ |
| `fuchsia` | `#ff00ff` | ████ |

### Dark Variants

| Name | Hex |
|------|-----|
| `darkred` | `#8b0000` |
| `darkgreen` | `#006400` |
| `darkblue` | `#00008b` |
| `darkcyan` | `#008b8b` |
| `darkmagenta` | `#8b008b` |
| `darkyellow` / `darkgoldenrod` | `#b8860b` |
| `darkgray` / `darkgrey` | `#a9a9a9` |
| `darkorange` | `#ff8c00` |
| `darkviolet` | `#9400d3` |

### Light Variants

| Name | Hex |
|------|-----|
| `lightred` / `lightcoral` | `#f08080` |
| `lightgreen` | `#90ee90` |
| `lightblue` | `#add8e6` |
| `lightcyan` | `#e0ffff` |
| `lightpink` | `#ffb6c1` |
| `lightyellow` | `#ffffe0` |
| `lightgray` / `lightgrey` | `#d3d3d3` |
| `lightsalmon` | `#ffa07a` |
| `lightseagreen` | `#20b2aa` |

### Bright Variants (Terminal)

| Name | Hex |
|------|-----|
| `bright_black` | `#555555` |
| `bright_red` | `#ff5555` |
| `bright_green` | `#55ff55` |
| `bright_yellow` | `#ffff55` |
| `bright_blue` | `#5555ff` |
| `bright_magenta` | `#ff55ff` |
| `bright_cyan` | `#55ffff` |
| `bright_white` | `#ffffff` |

## 256-Color Palette

### Standard Colors (0-15)

```
 0 Black       8 Bright Black
 1 Red         9 Bright Red
 2 Green      10 Bright Green
 3 Yellow     11 Bright Yellow
 4 Blue       12 Bright Blue
 5 Magenta    13 Bright Magenta
 6 Cyan       14 Bright Cyan
 7 White      15 Bright White
```

### Color Cube (16-231)

6x6x6 color cube:
- R: 0, 95, 135, 175, 215, 255
- G: 0, 95, 135, 175, 215, 255
- B: 0, 95, 135, 175, 215, 255

Index = 16 + 36×r + 6×g + b (where r,g,b are 0-5)

### Grayscale (232-255)

24 shades from dark to light:

| Range | Description |
|-------|-------------|
| 232-235 | Near black |
| 236-243 | Dark gray |
| 244-251 | Light gray |
| 252-255 | Near white |

## VellumFE Presets

### Vitals Presets

| Preset | Default | Usage |
|--------|---------|-------|
| `health` | `#00ff00` | Health bar (full) |
| `health_low` | `#ffff00` | Health bar (low) |
| `health_critical` | `#ff0000` | Health bar (critical) |
| `mana` | `#0080ff` | Mana bar |
| `stamina` | `#ff8000` | Stamina bar |
| `spirit` | `#ff00ff` | Spirit bar |

### Stream Presets

| Preset | Default | Usage |
|--------|---------|-------|
| `main` | `#ffffff` | Main game text |
| `room` | `#ffff00` | Room descriptions |
| `combat` | `#ff4444` | Combat messages |
| `speech` | `#00ffff` | Character speech |
| `whisper` | `#ff00ff` | Private messages |
| `thoughts` | `#00ff00` | Mental communication |

### UI Presets

| Preset | Default | Usage |
|--------|---------|-------|
| `background` | `#000000` | Window background |
| `text` | `#c0c0c0` | Default text |
| `text_dim` | `#808080` | Dimmed text |
| `border` | `#404040` | Widget borders |
| `border_focused` | `#ffffff` | Focused widget border |

### Status Presets

| Preset | Default | Usage |
|--------|---------|-------|
| `hidden` | `#00ff00` | Hidden indicator |
| `invisible` | `#00ffff` | Invisible indicator |
| `stunned` | `#ffff00` | Stunned indicator |
| `webbed` | `#ff00ff` | Webbed indicator |
| `prone` | `#00ffff` | Prone indicator |
| `kneeling` | `#ff8000` | Kneeling indicator |
| `sitting` | `#808080` | Sitting indicator |
| `dead` | `#ff0000` | Dead indicator |

## Color Modes

### True Color (24-bit)

16.7 million colors:
```toml
[display]
color_mode = "truecolor"
```

### 256 Colors

256-color palette:
```toml
[display]
color_mode = "256"
```

Colors are mapped to nearest palette entry.

### 16 Colors

Basic terminal colors:
```toml
[display]
color_mode = "16"
```

### No Color

Monochrome:
```toml
[display]
color_mode = "none"
```

## Color Math

### Brightness/Luminance

Calculate perceived brightness:
```
L = 0.299×R + 0.587×G + 0.114×B
```

### Contrast Ratio

For accessibility:
```
Ratio = (L1 + 0.05) / (L2 + 0.05)
```

WCAG guidelines:
- 4.5:1 minimum for normal text
- 3:1 minimum for large text
- 7:1 enhanced contrast

### Color Mixing

Linear interpolation:
```
mixed = color1 × (1-t) + color2 × t
```

## Accessibility Colors

### High Contrast Pairs

| Background | Foreground | Ratio |
|------------|------------|-------|
| `#000000` | `#ffffff` | 21:1 |
| `#000000` | `#ffff00` | 19.6:1 |
| `#000000` | `#00ffff` | 16.7:1 |
| `#000000` | `#00ff00` | 15.3:1 |
| `#1a1a1a` | `#ffffff` | 16.9:1 |
| `#1a1a1a` | `#e0e0e0` | 12.6:1 |

### Color Blindness Friendly

Avoid relying solely on red/green distinction:

```toml
# Instead of red/green for health
[theme]
health = "#00ff00"
health_critical = "#ff00ff"  # Magenta instead of red

# Or use brightness difference
health = "#80ff80"           # Bright green
health_critical = "#800000"  # Dark red (brightness contrast)
```

## Terminal Compatibility

### ANSI Escape Sequences

16 colors:
```
\e[30m - \e[37m   # Foreground colors
\e[40m - \e[47m   # Background colors
\e[90m - \e[97m   # Bright foreground
\e[100m - \e[107m # Bright background
```

256 colors:
```
\e[38;5;Nm  # Foreground (N = 0-255)
\e[48;5;Nm  # Background
```

True color:
```
\e[38;2;R;G;Bm  # Foreground (R,G,B = 0-255)
\e[48;2;R;G;Bm  # Background
```

### Terminal Detection

```bash
# Check color support
echo $TERM
echo $COLORTERM
tput colors
```

## See Also

- [Colors Configuration](../configuration/colors-toml.md)
- [Creating Themes](../customization/creating-themes.md)
- [Preset Colors](../reference/preset-colors.md)

