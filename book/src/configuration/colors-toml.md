# colors.toml Reference

The colors configuration file defines the visual theme including presets, palettes, and UI colors.

## Location

`~/.vellum-fe/colors.toml`

---

## Structure Overview

```toml
# Named presets (game text styling)
[presets]
speech = { fg = "#53a684" }
monsterbold = { fg = "#a29900" }

# Color palette (named colors)
[palette]
bright_red = "#FF5555"
bright_green = "#55FF55"

# UI element colors
[ui]
border = "#5588AA"
background = "#000000"

# Prompt colors
[prompt]
default = "#808080"
```

---

## [presets] Section

Presets define colors for game text elements:

```toml
[presets]
# Speech/dialogue
speech = { fg = "#53a684" }

# Creature names (monsterbold)
monsterbold = { fg = "#a29900" }

# Clickable links
links = { fg = "#477ab3" }

# Command echoes
commands = { fg = "#477ab3" }

# Whispers
whisper = { fg = "#4682B4" }

# Room name
roomName = { fg = "#9BA2B2", bg = "#395573" }

# Room description
roomDesc = { fg = "#CCCCCC" }

# Thoughts/ESP
thought = { fg = "#FF00FF" }

# Combat messages
combat = { fg = "#FF6600" }
```

### Preset Format

Each preset can have:

```toml
[presets]
name = { fg = "#RRGGBB", bg = "#RRGGBB", bold = true }
```

| Property | Type | Description |
|----------|------|-------------|
| `fg` | string | Foreground color |
| `bg` | string | Background color |
| `bold` | boolean | Bold styling |

### Standard Presets

These presets are used by the game protocol:

| Preset | Used For |
|--------|----------|
| `speech` | Player dialogue |
| `monsterbold` | Creature names |
| `links` | Clickable game objects |
| `commands` | Command echoes |
| `whisper` | Whispered text |
| `roomName` | Room titles |
| `roomDesc` | Room descriptions |
| `thought` | ESP/thoughts stream |

---

## [palette] Section

Named colors for use elsewhere in configuration:

```toml
[palette]
# Base colors
black = "#000000"
white = "#FFFFFF"
red = "#AA0000"
green = "#00AA00"
blue = "#0000AA"
yellow = "#AAAA00"
cyan = "#00AAAA"
magenta = "#AA00AA"

# Bright variants
bright_black = "#555555"
bright_white = "#FFFFFF"
bright_red = "#FF5555"
bright_green = "#55FF55"
bright_blue = "#5555FF"
bright_yellow = "#FFFF55"
bright_cyan = "#55FFFF"
bright_magenta = "#FF55FF"

# Custom colors
health_high = "#00FF00"
health_mid = "#FFFF00"
health_low = "#FF0000"
mana_color = "#0088FF"
```

### Using Palette Colors

Reference palette colors in other config files:

```toml
# In highlights.toml
[[highlights]]
name = "damage"
pattern = "damage"
fg = "bright_red"  # Uses palette.bright_red

# In layout.toml
[[windows]]
name = "health"
type = "progress"
bar_color = "health_high"  # Uses palette.health_high
```

---

## [ui] Section

Colors for UI elements:

```toml
[ui]
# Borders
border = "#5588AA"
border_focused = "#88AACC"

# Backgrounds
background = "#000000"
window_background = "#0A0A0A"

# Text
text = "#CCCCCC"
text_dim = "#808080"
text_highlight = "#FFFFFF"

# Selection
selection_bg = "#264F78"
selection_fg = "#FFFFFF"

# Scrollbar
scrollbar_track = "#1A1A1A"
scrollbar_thumb = "#404040"

# Menu
menu_bg = "#1A1A1A"
menu_fg = "#CCCCCC"
menu_selected_bg = "#264F78"
menu_selected_fg = "#FFFFFF"

# Input
input_bg = "#0A0A0A"
input_fg = "#FFFFFF"
input_cursor = "#FFFFFF"
```

### UI Color Reference

| Color | Used For |
|-------|----------|
| `border` | Window borders |
| `border_focused` | Focused window border |
| `background` | Global background |
| `window_background` | Window backgrounds |
| `text` | Default text |
| `text_dim` | Dimmed/secondary text |
| `text_highlight` | Highlighted text |
| `selection_bg` | Selection background |
| `selection_fg` | Selection foreground |

---

## [prompt] Section

Colors for the game prompt:

```toml
[prompt]
# Default prompt color
default = "#808080"

# Combat/RT prompt
combat = "#FF0000"

# Safe prompt
safe = "#00FF00"
```

---

## [spell] Section

Colors for spell circle indicators:

```toml
[spell]
# Major Elemental
major_elemental = "#FF6600"

# Minor Elemental
minor_elemental = "#FFAA00"

# Major Spiritual
major_spiritual = "#00AAFF"

# Minor Spiritual
minor_spiritual = "#0066FF"

# Bard
bard = "#FF00FF"

# Wizard
wizard = "#AA00FF"

# Sorcerer
sorcerer = "#660066"

# Ranger
ranger = "#00AA00"

# Paladin
paladin = "#FFFF00"

# Cleric
cleric = "#FFFFFF"

# Empath
empath = "#00FFAA"
```

---

## Color Formats

### Hex RGB

```toml
color = "#FF5500"    # 6-digit hex
color = "#F50"       # 3-digit hex (expanded to #FF5500)
```

### Named Colors

Reference palette entries:

```toml
color = "bright_red"     # From [palette]
color = "health_high"    # Custom palette entry
```

### Preset References

Reference preset colors with `@`:

```toml
color = "@speech"        # Uses presets.speech.fg
color = "@monsterbold"   # Uses presets.monsterbold.fg
```

---

## Complete Theme Example

```toml
# VellumFE Dark Theme

[presets]
speech = { fg = "#53a684" }
monsterbold = { fg = "#e8b923", bold = true }
links = { fg = "#477ab3" }
commands = { fg = "#477ab3" }
whisper = { fg = "#4682B4" }
roomName = { fg = "#9BA2B2", bg = "#395573" }
roomDesc = { fg = "#B0B0B0" }
thought = { fg = "#DA70D6" }

[palette]
# Base colors
black = "#000000"
white = "#FFFFFF"
red = "#CC0000"
green = "#00CC00"
blue = "#0066CC"
yellow = "#CCCC00"
cyan = "#00CCCC"
magenta = "#CC00CC"

# Bright variants
bright_red = "#FF5555"
bright_green = "#55FF55"
bright_blue = "#5588FF"
bright_yellow = "#FFFF55"
bright_cyan = "#55FFFF"
bright_magenta = "#FF55FF"

# Custom
health = "#00FF00"
mana = "#0088FF"
spirit = "#00FFFF"
stamina = "#FFFF00"

[ui]
border = "#3A5F7A"
border_focused = "#5588AA"
background = "#0A0A0A"
window_background = "#0F0F0F"
text = "#CCCCCC"
text_dim = "#666666"
text_highlight = "#FFFFFF"
selection_bg = "#264F78"
selection_fg = "#FFFFFF"
menu_bg = "#1A1A1A"
menu_fg = "#CCCCCC"
menu_selected_bg = "#264F78"
menu_selected_fg = "#FFFFFF"

[prompt]
default = "#666666"
combat = "#FF4444"
safe = "#44FF44"
```

---

## Theme Switching

VellumFE supports hot theme switching:

1. Edit `colors.toml`
2. Press `F5` to reload
3. New colors apply immediately

Or create multiple theme files and switch by copying:

```bash
cp themes/solarized.toml ~/.vellum-fe/colors.toml
# Then press F5 in VellumFE
```

---

## See Also

- [Creating Themes](../customization/creating-themes.md) - Theme authoring guide
- [Preset Colors Reference](../reference/preset-colors.md) - All preset names
- [Color Codes](../appendices/color-codes.md) - Hex color reference
