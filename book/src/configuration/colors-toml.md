# colors.toml

Custom color palette definitions.

## Basic Format

```toml
[colors]
my_red = "#FF0000"
soft_blue = "#5588AA"
dark_bg = "#1a1a1a"
```

## Using Custom Colors

Reference in other config files:

```toml
# In highlights.toml
[[highlights]]
name = "danger"
pattern = "attacks you"
foreground = "my_red"           # Uses color from colors.toml

# In layout.toml
[[windows]]
name = "combat"
border_color = "soft_blue"
background_color = "dark_bg"
```

## Preset Colors

VellumFE recognizes these named colors without definition:

| Name | Hex |
|------|-----|
| `black` | #000000 |
| `red` | #FF0000 |
| `green` | #00FF00 |
| `yellow` | #FFFF00 |
| `blue` | #0000FF |
| `magenta` | #FF00FF |
| `cyan` | #00FFFF |
| `white` | #FFFFFF |
| `gray` / `grey` | #808080 |
| `orange` | #FFA500 |
| `purple` | #800080 |
| `pink` | #FFC0CB |

## Example Palette

```toml
[colors]
# UI colors
border_normal = "#404040"
border_focused = "#5588AA"
border_alert = "#FF4040"

# Text colors
text_normal = "#CCCCCC"
text_dim = "#666666"
text_bright = "#FFFFFF"

# Creature states
dead = "#00FF00"
stunned = "#FFFF00"
frozen = "#00FFFF"

# Communication
speech = "#87CEEB"
whisper = "#DDA0DD"
thoughts = "#9370DB"

# Combat
damage_minor = "#FFFF00"
damage_major = "#FFA500"
damage_critical = "#FF0000"
```

## Color Formats

All colors must be specified as hex:

```toml
# Supported formats
color1 = "#RGB"                 # 3-digit (expanded to 6)
color2 = "#RRGGBB"              # 6-digit (most common)

# Not supported
color3 = "rgb(255, 0, 0)"       # CSS format - won't work
color4 = "255, 0, 0"            # Raw values - won't work
```
