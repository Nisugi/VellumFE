# Themes and Colors Guide

VellumFE supports extensive color customization through presets, highlights, and widget colors. This guide covers color schemes, themes, and visual customization.

## Color Format

All colors in VellumFE use hexadecimal RGB format:

```
#RRGGBB
```

- **RR** = Red component (00-FF)
- **GG** = Green component (00-FF)
- **BB** = Blue component (00-FF)

**Examples:**
- `#ff0000` - Pure red
- `#00ff00` - Pure green
- `#0000ff` - Pure blue
- `#ffffff` - White
- `#000000` - Black
- `#808080` - Gray
- `#ffff00` - Yellow
- `#ff00ff` - Magenta
- `#00ffff` - Cyan

**Transparent/No Color:**
Use `-` for transparent background or no color.

## Color Customization Areas

### 1. Preset Colors

Presets define colors for game text styles sent by the server.

**Edit via Settings:**
```bash
.settings
# Navigate to Presets section
```

**Edit in config file:**
```toml
[[presets]]
id = "speech"           # Preset identifier
fg = "#53a684"          # Foreground (text) color
bg = "-"                # Background color (- = transparent)
```

**Common presets:**
- `speech` - Player speech
- `thought` - Character thoughts
- `whisper` - Whispers
- `room` - Room descriptions
- `watching` - Familiar messages
- `penalty` - Penalties/debuffs
- `bonus` - Bonuses/buffs
- `roomName` - Room names
- `link` - Clickable links

### 2. Spell Colors

Override colors for specific spell numbers.

```toml
[[spell_colors]]
spell_number = 906
color = "#ff0000"
```

### 3. Prompt Colors

Customize prompt indicators (R/C/S/L/M in command prompt).

```toml
[[ui.prompt_colors]]
character = "R"         # R = roundtime
color = "#ff0000"       # Red

[[ui.prompt_colors]]
character = "C"         # C = casting
color = "#0000ff"       # Blue

[[ui.prompt_colors]]
character = "S"         # S = stunned
color = "#ffff00"       # Yellow
```

### 4. Widget Colors

**Progress bars:**
```toml
bar_color = "#00ff00"       # Bar fill
bar_bg_color = "#003300"    # Bar background
text_color = "#ffffff"      # Text overlay
```

**Countdown timers:**
```toml
bar_color = "#ff0000"       # Timer fill color
```

**Compass:**
```toml
compass_active_color = "#00ff00"     # Active exits
compass_inactive_color = "#333333"   # Inactive exits
```

**Hands:**
```toml
text_color = "#ffffff"      # Text color
```

### 5. Tab Colors

```toml
tab_active_color = "#ffff00"       # Active tab
tab_inactive_color = "#808080"     # Inactive tabs
tab_unread_color = "#ffffff"       # Tabs with unread messages
```

### 6. Border Colors

```bash
.border main single #ff0000
```

### 7. Highlights

Custom text matching with colors (see [Highlights](Highlights.md)).

```toml
[[highlights]]
name = "my_attacks"
pattern = "^You swing"
fg_color = "#ffff00"
bg_color = "#333300"
bold = true
```

## Pre-Made Themes

### Classic ProfanityFE

Matches traditional ProfanityFE colors:

```toml
[ui]
command_echo_color = "#ffffff"

[[presets]]
id = "speech"
fg = "#53a684"
bg = "-"

[[presets]]
id = "thought"
fg = "#9BA2B2"
bg = "#395573"

[[presets]]
id = "whisper"
fg = "#ff00ff"
bg = "-"

[[presets]]
id = "room"
fg = "#3ec9db"
bg = "-"

[[presets]]
id = "roomName"
fg = "#7CB7E3"
bg = "-"

[[presets]]
id = "watching"
fg = "#ffaa00"
bg = "-"

[[presets]]
id = "bonus"
fg = "#00ff00"
bg = "-"

[[presets]]
id = "penalty"
fg = "#ff0000"
bg = "-"
```

### High Contrast

Bright, vibrant colors for visibility:

```toml
[[presets]]
id = "speech"
fg = "#00ff00"
bg = "-"

[[presets]]
id = "thought"
fg = "#ffff00"
bg = "-"

[[presets]]
id = "whisper"
fg = "#ff00ff"
bg = "-"

[[presets]]
id = "room"
fg = "#00ffff"
bg = "-"

[[presets]]
id = "bonus"
fg = "#00ff00"
bg = "#003300"

[[presets]]
id = "penalty"
fg = "#ff0000"
bg = "#330000"
```

### Dark Theme

Muted colors on dark backgrounds:

```toml
[[presets]]
id = "speech"
fg = "#53a684"
bg = "#0a1a14"

[[presets]]
id = "thought"
fg = "#6b7d9c"
bg = "#0f1419"

[[presets]]
id = "whisper"
fg = "#a366d6"
bg = "#1a0a1a"

[[presets]]
id = "room"
fg = "#4a9da6"
bg = "#0a1214"

[[presets]]
id = "roomName"
fg = "#7CB7E3"
bg = "#0a0f14"
```

### Solarized Dark

Based on the Solarized color scheme:

```toml
# Base colors:
# base03  = #002b36
# base02  = #073642
# base01  = #586e75
# base00  = #657b83
# base0   = #839496
# base1   = #93a1a1
# base2   = #eee8d5
# base3   = #fdf6e3
# yellow  = #b58900
# orange  = #cb4b16
# red     = #dc322f
# magenta = #d33682
# violet  = #6c71c4
# blue    = #268bd2
# cyan    = #2aa198
# green   = #859900

[[presets]]
id = "speech"
fg = "#2aa198"          # cyan
bg = "-"

[[presets]]
id = "thought"
fg = "#268bd2"          # blue
bg = "-"

[[presets]]
id = "whisper"
fg = "#d33682"          # magenta
bg = "-"

[[presets]]
id = "room"
fg = "#839496"          # base0
bg = "-"

[[presets]]
id = "bonus"
fg = "#859900"          # green
bg = "-"

[[presets]]
id = "penalty"
fg = "#dc322f"          # red
bg = "-"
```

### Nord Theme

Based on the Nord color palette:

```toml
# Polar Night
# nord0  = #2e3440
# nord1  = #3b4252
# nord2  = #434c5e
# nord3  = #4c566a
# Snow Storm
# nord4  = #d8dee9
# nord5  = #e5e9f0
# nord6  = #eceff4
# Frost
# nord7  = #8fbcbb
# nord8  = #88c0d0
# nord9  = #81a1c1
# nord10 = #5e81ac
# Aurora
# nord11 = #bf616a (red)
# nord12 = #d08770 (orange)
# nord13 = #ebcb8b (yellow)
# nord14 = #a3be8c (green)
# nord15 = #b48ead (purple)

[[presets]]
id = "speech"
fg = "#a3be8c"          # green
bg = "-"

[[presets]]
id = "thought"
fg = "#81a1c1"          # frost blue
bg = "-"

[[presets]]
id = "whisper"
fg = "#b48ead"          # purple
bg = "-"

[[presets]]
id = "room"
fg = "#88c0d0"          # frost cyan
bg = "-"

[[presets]]
id = "bonus"
fg = "#a3be8c"          # green
bg = "-"

[[presets]]
id = "penalty"
fg = "#bf616a"          # red
bg = "-"
```

### Monokai

Based on the Monokai color scheme:

```toml
[[presets]]
id = "speech"
fg = "#a6e22e"          # green
bg = "-"

[[presets]]
id = "thought"
fg = "#66d9ef"          # cyan
bg = "-"

[[presets]]
id = "whisper"
fg = "#ae81ff"          # purple
bg = "-"

[[presets]]
id = "room"
fg = "#f8f8f2"          # foreground
bg = "-"

[[presets]]
id = "bonus"
fg = "#a6e22e"          # green
bg = "-"

[[presets]]
id = "penalty"
fg = "#f92672"          # red
bg = "-"
```

### Dracula

Based on the Dracula color scheme:

```toml
# Background = #282a36
# Foreground = #f8f8f2
# Comment    = #6272a4
# Cyan       = #8be9fd
# Green      = #50fa7b
# Orange     = #ffb86c
# Pink       = #ff79c6
# Purple     = #bd93f9
# Red        = #ff5555
# Yellow     = #f1fa8c

[[presets]]
id = "speech"
fg = "#50fa7b"          # green
bg = "-"

[[presets]]
id = "thought"
fg = "#bd93f9"          # purple
bg = "-"

[[presets]]
id = "whisper"
fg = "#ff79c6"          # pink
bg = "-"

[[presets]]
id = "room"
fg = "#8be9fd"          # cyan
bg = "-"

[[presets]]
id = "bonus"
fg = "#50fa7b"          # green
bg = "-"

[[presets]]
id = "penalty"
fg = "#ff5555"          # red
bg = "-"
```

## Creating Custom Themes

### Step 1: Plan Your Palette

Choose 5-10 colors for your theme:

1. **Primary** - Main text (room descriptions)
2. **Secondary** - Less important text
3. **Accent 1** - Speech
4. **Accent 2** - Thoughts
5. **Accent 3** - Whispers
6. **Success** - Bonuses, good things
7. **Warning** - Cautions
8. **Error** - Penalties, bad things
9. **Info** - System messages

### Step 2: Test Colors

Use an online color picker to test combinations:
- Ensure sufficient contrast
- Test with your terminal's background
- Consider color blindness accessibility

**Tools:**
- https://colorhunt.co/ - Color palettes
- https://coolors.co/ - Color scheme generator
- https://webaim.org/resources/contrastchecker/ - Contrast checker

### Step 3: Apply to Presets

Edit config file or use `.settings`:

```toml
[[presets]]
id = "speech"
fg = "#YOUR_COLOR"
bg = "-"

[[presets]]
id = "thought"
fg = "#YOUR_COLOR"
bg = "-"

# ... repeat for all presets
```

### Step 4: Test in Game

1. Launch VellumFE
2. Play for a bit
3. Note which colors need adjustment
4. Tweak and repeat

### Step 5: Share Your Theme

Export your config and share with community!

## Progress Bar Color Schemes

### Traffic Light

```toml
# Health (green)
bar_color = "#00ff00"
bar_bg_color = "#003300"

# Mana (blue)
bar_color = "#0000ff"
bar_bg_color = "#000033"

# Stamina (yellow)
bar_color = "#ffff00"
bar_bg_color = "#333300"

# Spirit (purple)
bar_color = "#ff00ff"
bar_bg_color = "#330033"
```

### Gradient

```toml
# Health (bright green)
bar_color = "#00ff00"
bar_bg_color = "#001100"

# Mana (cyan)
bar_color = "#00ffff"
bar_bg_color = "#001111"

# Stamina (yellow-green)
bar_color = "#88ff00"
bar_bg_color = "#111100"

# Spirit (blue)
bar_color = "#0088ff"
bar_bg_color = "#000f1f"
```

### Muted

```toml
# Health (muted green)
bar_color = "#559955"
bar_bg_color = "#1a2a1a"

# Mana (muted blue)
bar_color = "#5599ff"
bar_bg_color = "#1a1a2a"

# Stamina (muted yellow)
bar_color = "#999955"
bar_bg_color = "#2a2a1a"

# Spirit (muted purple)
bar_color = "#9955ff"
bar_bg_color = "#2a1a2a"
```

## Color Accessibility

### Color Blindness Considerations

**Protanopia (red-blind):**
- Avoid red/green for critical distinctions
- Use blue/yellow or blue/orange instead

**Deuteranopia (green-blind):**
- Similar to protanopia
- Use blue/yellow combinations

**Tritanopia (blue-blind):**
- Avoid blue/yellow combinations
- Use red/green instead

### High Contrast

For visibility impairment:
- Use very bright foreground colors
- Use dark or black backgrounds
- Maximize contrast ratio (aim for 7:1 or higher)

### Readable Combinations

**Good contrast examples:**
- White on black: `#ffffff` / `#000000`
- Yellow on black: `#ffff00` / `#000000`
- Cyan on black: `#00ffff` / `#000000`
- Bright green on black: `#00ff00` / `#000000`

**Poor contrast examples:**
- Dark blue on black: `#000088` / `#000000`
- Gray on black: `#444444` / `#000000`
- Dark green on black: `#004400` / `#000000`

## Terminal Color Limitations

### 256-Color Mode

Most terminals support 256 colors, which should work with all hex colors.

### True Color (24-bit)

Modern terminals support 24-bit true color:
- Windows Terminal ✓
- iTerm2 ✓
- Alacritty ✓
- Kitty ✓
- GNOME Terminal ✓

### Legacy Terminals

Older terminals (8/16 colors) may not display hex colors correctly. Upgrade to a modern terminal for full color support.

## Color Tips

1. **Consistency** - Use similar colors for related text types
2. **Contrast** - Ensure text is readable on background
3. **Hierarchy** - Use brightness/saturation to indicate importance
4. **Testing** - Test colors in actual game play, not just config
5. **Terminal theme** - Consider your terminal's background color
6. **Lighting** - Colors appear different in bright vs dark rooms
7. **Fatigue** - Very bright colors can cause eye strain over time

## See Also

- [Configuration](Configuration.md) - Config file reference
- [Highlights](Highlights.md) - Custom text coloring
- [Window Types](Window-Types.md) - Widget color properties
- [Getting Started](Getting-Started.md#settings) - Using settings editor
