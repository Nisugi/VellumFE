# Theme System

VellumFE uses a layered color system for flexible theming and per-character customization.

## Overview

The theme system provides:

1. **Presets** - Named color schemes for game text (speech, monsterbold, etc.)
2. **Prompt Colors** - Character-based prompt coloring (R, S, H, >)
3. **Spell Colors** - Spell-specific bar colors in active effects
4. **UI Colors** - Border, text, background colors for the interface
5. **Color Palette** - Named colors for highlights and quick access

## Configuration Files

```
~/.vellum-fe/
├── colors.toml              # Global colors
└── characters/
    └── CharName/
        └── colors.toml      # Per-character overrides
```

### Loading Order

1. Load character-specific `colors.toml` if it exists
2. Fall back to global `~/.vellum-fe/colors.toml`
3. Fall back to embedded defaults

## Presets

Presets define colors for semantic text types from the game server.

### Default Presets

```toml
[presets.links]
fg = "#477ab3"

[presets.commands]
fg = "#477ab3"

[presets.speech]
fg = "#53a684"

[presets.roomName]
fg = "#9BA2B2"
bg = "#395573"

[presets.monsterbold]
fg = "#a29900"

[presets.familiar]
fg = "#767339"

[presets.thought]
fg = "#FF8080"

[presets.whisper]
fg = "#60b4bf"
```

### Preset Structure

```toml
[presets.my_preset]
fg = "#RRGGBB"     # Foreground color (hex)
bg = "#RRGGBB"     # Background color (optional)
```

### How Presets Are Applied

The parser applies presets when encountering specific XML tags:

| Preset | Triggered By | Example |
|--------|--------------|---------|
| `speech` | `<preset id="speech">` | Player dialogue |
| `monsterbold` | `<pushBold/>` | Monster names |
| `roomName` | `<style id="roomName">` | Room titles |
| `links` | `<a>` tags | Clickable objects |
| `commands` | `<d>` tags | Direct commands |
| `whisper` | `<preset id="whisper">` | Whispers |
| `thought` | `<preset id="thought">` | ESP/thoughts |
| `familiar` | `<preset id="familiar">` | Familiar messages |

## Prompt Colors

Prompt colors apply to individual characters in the game prompt.

### Configuration

```toml
[[prompt_colors]]
character = "R"
color = "#ff0000"

[[prompt_colors]]
character = "S"
color = "#ffff00"

[[prompt_colors]]
character = "H"
color = "#9370db"

[[prompt_colors]]
character = ">"
color = "#a9a9a9"
```

### Common Prompt Characters

| Character | Meaning | Suggested Color |
|-----------|---------|-----------------|
| `R` | Roundtime | Red |
| `S` | Spell roundtime | Yellow |
| `H` | Hidden | Purple |
| `>` | Ready | Gray |
| `K` | Kneeling | Custom |
| `P` | Prone | Custom |

## UI Colors

UI colors control the application interface appearance.

### Configuration

```toml
[ui]
command_echo_color = "#ffffff"      # Echo of typed commands
border_color = "#00ffff"            # Unfocused widget borders
focused_border_color = "#ffff00"    # Focused widget borders
text_color = "#ffffff"              # Default text color
background_color = "#000000"        # Widget background
selection_bg_color = "#4a4a4a"      # Text selection background
textarea_background = "-"           # Input field background ("-" = transparent)
```

### Transparent Background

Set `textarea_background = "-"` to use a transparent background for the command input area.

## Spell Colors

Spell colors assign colors to specific spell numbers in the Active Spells widget.

### Configuration

```toml
[[spell_colors]]
spells = [503, 506, 507, 508, 509]    # Spell numbers
color = "#5c0000"                      # Bar color
bar_color = "#5c0000"                  # Same as color
text_color = "#909090"                 # Text on the bar
bg_color = "#000000"                   # Background behind bar
```

### Spell Circle Examples

```toml
# Minor Spirit (500s) - Dark Red
[[spell_colors]]
spells = [503, 506, 507, 508, 509, 513, 515, 520, 525, 535, 540]
color = "#5c0000"

# Major Spirit (900s) - Purple
[[spell_colors]]
spells = [905, 911, 913, 918, 919, 920, 925, 930, 940]
color = "#9370db"

# Ranger (600s) - Green
[[spell_colors]]
spells = [601, 602, 604, 605, 606, 608, 612, 613, 617, 618, 620, 625, 640, 650]
color = "#1c731c"

# Sorcerer (700s) - Indigo
[[spell_colors]]
spells = [701, 703, 705, 708, 712, 713, 715, 720, 725, 730, 735, 740]
color = "#4b0082"
```

## Color Palette

The color palette provides named colors for highlights and quick access.

### Structure

```toml
[[color_palette]]
name = "red"
color = "#FF0000"
category = "red"

[[color_palette]]
name = "crimson"
color = "#DC143C"
category = "red"

[[color_palette]]
name = "forestgreen"
color = "#228B22"
category = "green"
```

### Default Categories

- **red** - Red, crimson, darkred, firebrick, indianred
- **orange** - Orange, darkorange, coral, tomato
- **yellow** - Yellow, gold, khaki, lightyellow
- **green** - Green, lime, darkgreen, forestgreen, seagreen
- **cyan** - Cyan, aqua, darkcyan, teal, turquoise
- **blue** - Blue, darkblue, navy, royalblue, steelblue
- **purple** - Purple, darkviolet, darkorchid, indigo, violet
- **magenta** - Magenta, fuchsia, deeppink, hotpink, pink
- **brown** - Brown, saddlebrown, sienna, chocolate, tan
- **gray** - White, black, gray, darkgray, lightgray, silver

### Using Palette Colors

In highlights, reference palette colors by name:

```toml
[death_blow]
pattern = "death blow"
fg = "crimson"          # Resolved to #DC143C
bg = "darkred"          # Resolved to #8B0000
```

## Color Abstraction Layer

VellumFE provides a frontend-agnostic color system.

### Color Struct

```rust
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self;
    pub fn from_hex(hex: &str) -> Option<Self>;
    pub fn to_hex(&self) -> String;

    // Constants
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const RED: Self = Self::rgb(255, 0, 0);
    // ... more constants
}
```

### NamedColor Enum

```rust
pub enum NamedColor {
    // Standard ANSI colors
    Black, Red, Green, Yellow, Blue, Magenta, Cyan,
    Gray, DarkGray, LightRed, LightGreen, LightYellow,
    LightBlue, LightMagenta, LightCyan, White,

    // RGB color
    Rgb(u8, u8, u8),

    // ANSI 256-color palette (0-255)
    Indexed(u8),

    // Reset to default
    Reset,
}
```

### ANSI 256-Color Support

| Range | Description |
|-------|-------------|
| 0-15 | Standard 16 ANSI colors |
| 16-231 | 6×6×6 color cube (216 colors) |
| 232-255 | Grayscale ramp (24 grays) |

## Color Resolution

When resolving a color reference:

```rust
pub fn resolve_palette_color(&self, input: &str) -> String {
    // If starts with #, use as-is
    if input.starts_with('#') {
        return input.to_string();
    }

    // Look up in palette (case-insensitive)
    for entry in &self.color_palette {
        if entry.name.eq_ignore_ascii_case(input) {
            return entry.color.clone();
        }
    }

    // Return original if not found
    input.to_string()
}
```

## Hot-Switching

Reload colors without restarting:

```
.reload colors
```

This:
1. Reloads `colors.toml` from disk
2. Updates parser presets
3. Refreshes all widgets
4. Applies immediately to new text

## Creating Custom Themes

### Step 1: Copy Defaults

```bash
cp ~/.vellum-fe/colors.toml ~/.vellum-fe/colors-backup.toml
```

### Step 2: Edit colors.toml

```toml
# Dark theme with blue accents
[presets.speech]
fg = "#6eb5ff"

[presets.monsterbold]
fg = "#ffd700"

[presets.links]
fg = "#87ceeb"

[ui]
border_color = "#2196f3"
focused_border_color = "#64b5f6"
text_color = "#e0e0e0"
background_color = "#121212"
```

### Step 3: Reload

```
.reload colors
```

## Theme Examples

### High Contrast (Accessibility)

```toml
[ui]
text_color = "#ffffff"
background_color = "#000000"
focused_border_color = "#ffff00"

[presets.monsterbold]
fg = "#ff0000"

[presets.speech]
fg = "#00ff00"
```

### Solarized Dark

```toml
[ui]
background_color = "#002b36"
text_color = "#839496"
border_color = "#073642"
focused_border_color = "#2aa198"

[presets.speech]
fg = "#859900"

[presets.monsterbold]
fg = "#cb4b16"
```

### Nord Theme

```toml
[ui]
background_color = "#2e3440"
text_color = "#d8dee9"
border_color = "#3b4252"
focused_border_color = "#88c0d0"

[presets.speech]
fg = "#a3be8c"

[presets.monsterbold]
fg = "#d08770"
```

## Color Format Reference

| Format | Example | Notes |
|--------|---------|-------|
| `#RRGGBB` | `#FF5733` | Standard hex |
| `#rrggbb` | `#ff5733` | Lowercase also valid |
| Named | `crimson` | From color_palette |
| Empty | `""` | Use default/transparent |
| Dash | `"-"` | Transparent (textarea only) |

## Browser Commands

| Command | Description |
|---------|-------------|
| `.colors` | Browse color palette |
| `.spellcolors` | Browse/edit spell colors |
| `.addspellcolor <spell> <color>` | Add spell color |
| `.reload colors` | Hot-reload colors.toml |

## See Also

- [Colors Configuration](../configuration/colors-toml.md) - Full colors.toml reference
- [Highlights Configuration](../configuration/highlights-toml.md) - Using colors in highlights
- [Parser Protocol](./parser-protocol.md) - How presets are applied

