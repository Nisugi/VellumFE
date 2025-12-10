# Creating Themes

Themes control colors throughout VellumFE. This guide shows how to create custom color schemes.

## Theme Basics

Themes are defined in `colors.toml`:

```toml
# UI Colors
[ui]
text_color = "#FFFFFF"
background_color = "#000000"
border_color = "#00FFFF"
focused_border_color = "#FFFF00"

# Text Presets
[presets.speech]
fg = "#53a684"

[presets.monsterbold]
fg = "#a29900"
```

## Color Formats

VellumFE supports multiple color formats:

| Format | Example | Description |
|--------|---------|-------------|
| Hex RGB | `#FF5500` | Standard hex |
| Named | `crimson` | From palette |
| Transparent | `"-"` | Clear background |

## UI Colors

### Configuration

```toml
[ui]
# Text
text_color = "#FFFFFF"           # Default text
command_echo_color = "#FFFFFF"   # Command echo

# Backgrounds
background_color = "#000000"     # Widget background
textarea_background = "-"        # Input background ("-" = transparent)

# Borders
border_color = "#00FFFF"         # Unfocused borders
focused_border_color = "#FFFF00" # Focused border

# Selection
selection_bg_color = "#4a4a4a"   # Text selection
```

### Example: Dark Theme

```toml
[ui]
text_color = "#E0E0E0"
background_color = "#1A1A1A"
border_color = "#333333"
focused_border_color = "#007ACC"
selection_bg_color = "#264F78"
```

### Example: Light Theme

```toml
[ui]
text_color = "#333333"
background_color = "#FFFFFF"
border_color = "#CCCCCC"
focused_border_color = "#0066CC"
selection_bg_color = "#ADD6FF"
```

## Text Presets

Presets color semantic text types from the game:

### Default Presets

```toml
[presets.speech]
fg = "#53a684"       # Player dialogue

[presets.monsterbold]
fg = "#a29900"       # Creature names

[presets.roomName]
fg = "#9BA2B2"
bg = "#395573"       # Room titles

[presets.links]
fg = "#477ab3"       # Clickable objects

[presets.whisper]
fg = "#60b4bf"       # Whispers

[presets.thought]
fg = "#FF8080"       # ESP/thoughts
```

### Custom Preset

```toml
[presets.my_custom]
fg = "#FF6600"
bg = "#1A1A1A"
```

## Prompt Colors

Color individual prompt characters:

```toml
[[prompt_colors]]
character = "R"         # Roundtime
color = "#FF0000"

[[prompt_colors]]
character = "S"         # Spell RT
color = "#FFFF00"

[[prompt_colors]]
character = "H"         # Hidden
color = "#9370DB"

[[prompt_colors]]
character = ">"         # Ready
color = "#A9A9A9"
```

## Spell Colors

Color spell bars by spell number:

```toml
# Minor Spirit (500s)
[[spell_colors]]
spells = [503, 506, 507, 508, 509]
color = "#5c0000"

# Major Spirit (900s)
[[spell_colors]]
spells = [905, 911, 913]
color = "#9370db"

# Ranger (600s)
[[spell_colors]]
spells = [601, 602, 604, 605]
color = "#1c731c"
```

## Color Palette

Define named colors for use in highlights:

```toml
[[color_palette]]
name = "danger"
color = "#FF0000"
category = "custom"

[[color_palette]]
name = "warning"
color = "#FFA500"
category = "custom"

[[color_palette]]
name = "success"
color = "#00FF00"
category = "custom"
```

Use in highlights:

```toml
[critical_hit]
pattern = "critical hit"
fg = "danger"      # Resolved from palette
```

## Theme Design Tips

### Color Harmony

Use complementary colors:
- **Analogous**: Colors next to each other on wheel
- **Complementary**: Opposite colors
- **Triadic**: Three evenly spaced colors

### Contrast

Ensure readable text:
- Light text on dark background
- Dark text on light background
- Minimum 4.5:1 contrast ratio

### Consistency

Use a limited color palette:
- 2-3 accent colors
- Neutral backgrounds
- Semantic colors (red=danger, green=success)

## Complete Theme Examples

### Nord Theme

```toml
[ui]
text_color = "#D8DEE9"
background_color = "#2E3440"
border_color = "#3B4252"
focused_border_color = "#88C0D0"
selection_bg_color = "#4C566A"

[presets.speech]
fg = "#A3BE8C"

[presets.monsterbold]
fg = "#D08770"

[presets.roomName]
fg = "#ECEFF4"
bg = "#434C5E"

[presets.links]
fg = "#81A1C1"

[presets.whisper]
fg = "#B48EAD"

[[prompt_colors]]
character = "R"
color = "#BF616A"

[[prompt_colors]]
character = ">"
color = "#616E88"
```

### Solarized Dark

```toml
[ui]
text_color = "#839496"
background_color = "#002B36"
border_color = "#073642"
focused_border_color = "#2AA198"
selection_bg_color = "#073642"

[presets.speech]
fg = "#859900"

[presets.monsterbold]
fg = "#CB4B16"

[presets.roomName]
fg = "#93A1A1"
bg = "#073642"

[presets.links]
fg = "#268BD2"

[[prompt_colors]]
character = "R"
color = "#DC322F"

[[prompt_colors]]
character = ">"
color = "#586E75"
```

### High Contrast

```toml
[ui]
text_color = "#FFFFFF"
background_color = "#000000"
border_color = "#FFFFFF"
focused_border_color = "#FFFF00"
selection_bg_color = "#0000FF"

[presets.speech]
fg = "#00FF00"

[presets.monsterbold]
fg = "#FF0000"

[presets.roomName]
fg = "#FFFFFF"
bg = "#0000AA"

[presets.links]
fg = "#00FFFF"

[[prompt_colors]]
character = "R"
color = "#FF0000"

[[prompt_colors]]
character = ">"
color = "#FFFFFF"
```

### Monokai

```toml
[ui]
text_color = "#F8F8F2"
background_color = "#272822"
border_color = "#49483E"
focused_border_color = "#F92672"
selection_bg_color = "#49483E"

[presets.speech]
fg = "#A6E22E"

[presets.monsterbold]
fg = "#FD971F"

[presets.roomName]
fg = "#F8F8F2"
bg = "#3E3D32"

[presets.links]
fg = "#66D9EF"

[[prompt_colors]]
character = "R"
color = "#F92672"

[[prompt_colors]]
character = ">"
color = "#75715E"
```

## Applying Themes

### Hot-Reload

```
.reload colors
```

### Per-Character

Place theme in character folder:

```
~/.vellum-fe/characters/MyChar/colors.toml
```

## Creating Variants

### Dark/Light Toggle

Create two theme files:

```
~/.vellum-fe/themes/dark.toml
~/.vellum-fe/themes/light.toml
```

Switch by copying:

```bash
cp ~/.vellum-fe/themes/dark.toml ~/.vellum-fe/colors.toml
```

### Profession Variants

Create profession-specific themes:

```
~/.vellum-fe/characters/Warrior/colors.toml   # Combat colors
~/.vellum-fe/characters/Empath/colors.toml    # Healing colors
```

## Sharing Themes

1. Export your `colors.toml`
2. Share via GitHub Gist, Discord, etc.
3. Others copy to their `~/.vellum-fe/` directory
4. Run `.reload colors`

## See Also

- [Colors Configuration](../configuration/colors-toml.md) - Full reference
- [Theme System Architecture](../architecture/theme-system.md) - How theming works
- [Accessibility](../tutorials/accessibility.md) - High contrast themes

