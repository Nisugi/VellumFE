# Color Modes

VellumFE supports multiple color rendering modes for terminal compatibility, allowing users to choose between true color (24-bit RGB) and 256-color palette rendering.

## Overview

| Mode | Terminal Requirement | Color Accuracy | Use Case |
|------|---------------------|----------------|----------|
| `Direct` (default) | True color (24-bit) | Perfect | Modern terminals (iTerm2, Windows Terminal, Kitty) |
| `Slot` | 256-color | Approximated | Legacy terminals, SSH sessions, tmux |

## Architecture

### ColorMode Enum

```rust
// src/config.rs
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    #[default]
    Direct,  // True color RGB (modern terminals)
    Slot,    // 256-color palette indices (legacy terminals)
}
```

### PaletteColor with Slot Assignment

Each color in the palette can have an optional terminal slot assignment:

```rust
// src/config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaletteColor {
    pub name: String,
    pub color: String,      // Hex color code (e.g., "#FF0000")
    pub category: String,   // Color family: "red", "blue", "green", etc.
    #[serde(default)]
    pub favorite: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u8>,   // Terminal palette slot (16-231) for .setpalette
}
```

## Direct Mode (Default)

Sends exact RGB values to the terminal using ANSI true color sequences.

**Escape Sequence:** `ESC[38;2;R;G;Bm` (foreground) / `ESC[48;2;R;G;Bm` (background)

**Example:** Color `#FF6347` (tomato) sends `ESC[38;2;255;99;71m`

**Terminal Support:**
- ✅ iTerm2, Kitty, Alacritty, Windows Terminal, GNOME Terminal 3.x+
- ❌ PuTTY, older xterm, some SSH configurations

## Slot Mode (256-Color)

Converts RGB colors to the nearest match in the xterm 256-color palette.

**Escape Sequence:** `ESC[38;5;Nm` where N is 0-255

### The 256-Color Palette

| Slot Range | Description |
|------------|-------------|
| 0-15 | Standard ANSI colors (terminal-dependent) |
| 16-231 | 6×6×6 color cube (216 colors) |
| 232-255 | Grayscale ramp (24 shades) |

### Color Cube Formula (Slots 16-231)

```
slot = 16 + (36 × r) + (6 × g) + b
where r, g, b ∈ {0, 1, 2, 3, 4, 5}
```

RGB values map to cube indices:
- 0 → 0-47
- 1 → 48-114
- 2 → 115-154
- 3 → 155-194
- 4 → 195-234
- 5 → 235-255

### Implementation

```rust
// src/frontend/tui/colors.rs
pub fn rgb_to_nearest_slot(r: u8, g: u8, b: u8) -> u8 {
    // Check if grayscale (r == g == b)
    if r == g && g == b {
        if r < 8 { return 16; }      // Near black
        if r > 248 { return 231; }   // Near white
        return (232 + (r - 8) / 10).min(255);  // Grayscale ramp
    }

    // Map to 6x6x6 color cube (slots 16-231)
    let to_6 = |v: u8| -> u8 {
        match v {
            0..=47 => 0,
            48..=114 => 1,
            115..=154 => 2,
            155..=194 => 3,
            195..=234 => 4,
            _ => 5,
        }
    };

    16 + 36 * to_6(r) + 6 * to_6(g) + to_6(b)
}
```

## Configuration

### Config File (config.toml)

```toml
[ui]
color_mode = "direct"  # or "slot"
```

### CLI Override

```bash
# True color mode (default)
vellum-fe --port 8000

# 256-color mode
vellum-fe --color-mode slot --port 8000

# 256-color with custom palette loaded
vellum-fe --color-mode slot --setup-palette --port 8000
```

## Palette Commands

### .setpalette

Writes all colors from `color_palette` that have slot assignments to the terminal using OSC 4:

```
.setpalette    # Load palette colors into terminal slots
```

**Implementation:**

```rust
// src/frontend/tui/mod.rs
pub fn execute_setpalette(&mut self, app_core: &AppCore) -> Result<()> {
    let palette = &app_core.config.colors.color_palette;
    let backend = self.terminal.backend_mut();

    for palette_color in palette {
        if let Some(slot) = palette_color.slot {
            if let Ok(color) = parse_hex_color(&palette_color.color) {
                if let Color::Rgb(r, g, b) = color {
                    // OSC 4 format: ESC]4;<slot>;rgb:<rr>/<gg>/<bb>BEL
                    let seq = format!(
                        "\x1b]4;{};rgb:{:02x}/{:02x}/{:02x}\x07",
                        slot, r, g, b
                    );
                    backend.write_all(seq.as_bytes())?;
                }
            }
        }
    }
    backend.flush()?;
    Ok(())
}
```

### .resetpalette

Resets terminal palette to defaults using OSC 104:

```
.resetpalette  # Restore terminal's default palette
```

**Implementation:**

```rust
pub fn execute_resetpalette(&mut self) -> Result<()> {
    let backend = self.terminal.backend_mut();
    backend.write_all(b"\x1b]104\x07")?;  // OSC 104 resets all slots
    backend.flush()?;
    Ok(())
}
```

## Default Palette Slot Assignments

The default `colors.toml` includes **actual application colors** with slot assignments, organized by category:

### Preset Colors (Slots 16-23)
Game text stream colors used for speech, whisper, links, etc.

| Slot | Name | Color | Usage |
|------|------|-------|-------|
| 16 | Link Blue | `#477ab3` | Clickable links and commands |
| 17 | Speech Green | `#53a684` | Character speech |
| 18 | Room Name | `#9BA2B2` | Room title foreground |
| 19 | Room Name BG | `#395573` | Room title background |
| 20 | Monster Bold | `#a29900` | Creature names |
| 21 | Familiar | `#767339` | Familiar channel |
| 22 | Thought | `#FF8080` | ESP/thought text |
| 23 | Whisper | `#60b4bf` | Whispered text |

### UI Colors (Slots 24-29)
Interface element colors for borders, text, and backgrounds.

| Slot | Name | Color | Usage |
|------|------|-------|-------|
| 24 | UI Border | `#00ffff` | Default window borders |
| 25 | UI Focused Border | `#ffff00` | Active window border |
| 26 | UI Text | `#ffffff` | Default text color |
| 27 | UI Background | `#000000` | Default background |
| 28 | UI Selection | `#4a4a4a` | Text selection highlight |
| 29 | Spell Text | `#909090` | Spell indicator text |

### Prompt Colors (Slots 30-33)
Prompt indicator character colors.

| Slot | Name | Color | Usage |
|------|------|-------|-------|
| 30 | Prompt Red | `#ff0000` | 'R' (roundtime) |
| 31 | Prompt Yellow | `#ffff00` | 'S' (stance) |
| 32 | Prompt Purple | `#9370db` | 'H' (hidden/mana) |
| 33 | Prompt Gray | `#a9a9a9` | '>' (standard prompt) |

### Spell Circle Colors (Slots 40-50)
Colors for active spell indicators by circle.

| Slot | Name | Color | Spell Circle |
|------|------|-------|--------------|
| 40 | Sorcerer | `#5c0000` | 500s |
| 41 | Empath (900) | `#9370db` | 900s |
| 42 | Ranger | `#1c731c` | 600s |
| 43 | Cleric (700) | `#4b0082` | 700s |
| 44 | Empath (1100) | `#76284b` | 1100s |
| 45 | Wizard | `#ff8c00` | 1000s |
| 46 | Bard | `#ff69b4` | 1600s |
| 47 | Minor Spirit | `#0086b3` | 100s |
| 48 | Cleric (300) | `#ffffff` | 300s |
| 49 | Minor Elemental | `#003d52` | 200s |
| 50 | Arcane | `#808000` | 5300s |

### Utility Colors (Slots 60-89)
Common named colors for highlights, custom widgets, and user convenience.

| Slot | Name | Slot | Name |
|------|------|------|------|
| 60 | Red | 75 | Gold |
| 61 | Green | 76 | Silver |
| 62 | Blue | 77 | Lime |
| 63 | Yellow | 78 | Teal |
| 64 | Cyan | 79 | Navy |
| 65 | Magenta | 80 | Coral |
| 66 | White | 81 | Salmon |
| 67 | Black | 82 | Violet |
| 68 | Orange | 83 | Indigo |
| 69 | Pink | 84 | Forest Green |
| 70 | Purple | 85 | Sky Blue |
| 71 | Brown | 86 | Deep Pink |
| 72 | Gray | 87 | Olive |
| 73 | Light Gray | 88 | Crimson |
| 74 | Dark Gray | 89 | Turquoise |

### Dark Theme Colors (Slots 100-103)
Unique colors from the dark theme not covered by utility colors.

| Slot | Name | Color | Usage |
|------|------|-------|-------|
| 100 | Background Secondary | `#141414` | Alternate backgrounds |
| 101 | Background Hover | `#282828` | Mouse hover states |
| 102 | Form Field BG | `#1E1E1E` | Form input backgrounds |
| 103 | Cornflower Blue | `#6495ED` | Form labels |

### Theme Family Colors (Slots 104-231)
All unique colors from all 36 built-in themes are pre-loaded, enabling **instant theme switching in Slot mode** without re-running `.setpalette`.

| Slot Range | Theme | Key Colors |
|------------|-------|------------|
| 104-115 | Nord | Frost ice, aurora colors, polar night |
| 116-126 | Dracula | Purple, pink, cyan, green |
| 127-141 | Solarized | Base tones, accent colors |
| 142-150 | Monokai | Cyan, pink, green, orange |
| 151-162 | Gruvbox | Aqua, earthy tones, warm colors |
| 163-174 | Night Owl | Ocean blues, neon highlights |
| 175-185 | Catppuccin | Mocha pastels, soft tones |
| 186-196 | Cyberpunk | Neon pink, cyan, electric colors |
| 197-203 | Retro Terminal | Amber, phosphor green |
| 204-211 | Apex | Cyan, orange, deep blue |
| 212-218 | Synthwave | Magenta, violet, neon |
| 219-223 | Ocean Depths | Ocean blues, cyan |
| 224-227 | Forest | Green tones, natural colors |
| 228-231 | Sunset | Purple, orange, warm tones |

**Total theme colors: 128** (slots 104-231)

This comprehensive pre-loading ensures that when you switch themes (e.g., from Dark to Nord to Dracula), all the new theme's colors are already in the terminal palette. No need to run `.setpalette` again!

## User Workflows

### Modern Terminal (Default)

```bash
vellum-fe --port 8000
# Uses Direct mode with true color RGB - no setup needed
```

### Legacy 256-Color Terminal

```bash
vellum-fe --color-mode slot --port 8000
# Uses nearest-match approximation to 256-color palette
```

### Profanity-Like Exact Colors

For users who want exact color reproduction on 256-color terminals (similar to Profanity client):

```bash
vellum-fe --color-mode slot --setup-palette --port 8000
# Loads custom colors into terminal palette slots, then uses Slot mode
```

### Runtime Palette Control

```
.setpalette      # Load theme colors into terminal palette
.resetpalette    # Reset terminal to default palette
```

## Terminal Compatibility

| Terminal | OSC 4 Support | Direct Mode | Notes |
|----------|---------------|-------------|-------|
| iTerm2 | ✅ Full | ✅ | Best experience |
| Kitty | ✅ Full | ✅ | Best experience |
| Windows Terminal | ✅ Full | ✅ | Best experience |
| GNOME Terminal | ✅ Full | ✅ | |
| xterm | ✅ Full | ✅ | |
| Alacritty | ⚠️ Partial | ✅ | OSC 4 may not persist |
| tmux | ⚠️ Passthrough | ⚠️ | Requires `set -g allow-passthrough on` |
| PuTTY | ❌ None | ❌ | Use Slot mode |

## Files Modified

| File | Changes |
|------|---------|
| `src/config.rs` | Added `ColorMode` enum, `slot` field to `PaletteColor`, `color_mode` to `UiConfig` |
| `src/main.rs` | Added `--color-mode` and `--setup-palette` CLI flags |
| `src/frontend/tui/colors.rs` | Added `rgb_to_nearest_slot()`, `parse_hex_color_with_mode()` |
| `src/core/app_core/commands.rs` | Added `.setpalette` and `.resetpalette` commands |
| `src/frontend/tui/menu_actions.rs` | Added action handlers for palette commands |
| `src/frontend/tui/mod.rs` | Added `execute_setpalette()`, `execute_resetpalette()` methods |
| `src/frontend/tui/runtime.rs` | Added startup hook for `--setup-palette` |
| `src/core/app_core/state.rs` | Updated help text |
| `defaults/colors.toml` | Centralized 191 colors (63 base + 128 theme) with slot assignments |

## Design Decisions

### Centralized Color Management

The `color_palette` in `colors.toml` serves as the **single source of truth** for all application colors. Rather than arbitrary CSS color names, it contains the actual colors used by VellumFE:

- **Preset colors** - Speech, whisper, links, room names, etc.
- **UI colors** - Borders, text, backgrounds, selections
- **Prompt colors** - RT, stance, mana indicators
- **Spell colors** - Each spell circle has a distinct color
- **Utility colors** - Common named colors for user convenience
- **Theme colors** - All unique colors from all 36 built-in themes

**Benefits:**
- `.setpalette` loads colors that **actually matter** for the UI
- **Instant theme switching** - All theme colors pre-loaded
- Single place to customize all application colors
- Works across all frontends (TUI, GUI, WUI)
- Users can see and edit all color assignments via Color Palette Browser
- Easy to extend with new colors

### Slot Range Organization

Slots are organized by category for predictability:

| Range | Category | Count | Purpose |
|-------|----------|-------|---------|
| 16-23 | Presets | 8 | Game text streams |
| 24-29 | UI | 6 | Interface elements |
| 30-33 | Prompt | 4 | Prompt indicators |
| 40-50 | Spell | 11 | Spell circle colors |
| 60-89 | Utility | 30 | Common named colors |
| 100-103 | Dark Theme | 4 | Dark theme specifics |
| 104-231 | All Themes | 128 | Nord, Dracula, Solarized, etc. |

**Total used: 191 slots** out of 216 available (16-231).

This avoids ANSI colors 0-15 (terminal-dependent) and grayscale 232-255.

### All Themes Pre-Loaded

Unlike systems that only load the current theme's colors, VellumFE pre-loads **all unique colors from all 36 built-in themes** into the palette:

**Why this matters:**
- **Instant theme switching** - Change from Dark to Nord to Dracula without re-running `.setpalette`
- **No palette reloading** - The 191 pre-loaded colors cover every theme
- **Consistent experience** - Slot mode works identically to Direct mode for all themes

**How it works:**
1. Each theme has ~56 color fields, but only ~10-15 unique hex values
2. Many themes share base colors (black, white, gray)
3. Unique colors are deduplicated across all themes
4. The resulting 128 theme-specific colors + 63 base colors = 191 total

**Example:** When you run `.setpalette` once, you load Nord's frost blues, Dracula's purples, Solarized's precision colors, Gruvbox's earthy tones, AND all other theme colors simultaneously. Switching themes just changes which palette slots are referenced.

### Auto-Slot Assignment

When users add new colors via the Color Palette Browser, they automatically receive the next available slot:

```rust
// src/frontend/tui/input.rs
fn find_next_available_slot(palette: &[PaletteColor]) -> Option<u8> {
    let used_slots: HashSet<u8> = palette
        .iter()
        .filter_map(|c| c.slot)
        .collect();

    // Search in color cube range (16-231)
    (16u8..=231).find(|slot| !used_slots.contains(slot))
}
```

This ensures:
- New colors are immediately usable with `.setpalette`
- No manual slot assignment required
- Slots are allocated sequentially starting from the first unused slot
- Existing slots are never overwritten

### Mode-Aware Parsing

The `parse_hex_color_with_mode()` function allows future integration where all color parsing respects the configured mode, enabling automatic nearest-match in Slot mode without `.setpalette`.

## See Also

- [Color Palette Browser](../configuration/color-palette.md) - Managing palette colors
- [Theme System](./themes.md) - Theme color configuration
- [Terminal Setup](../getting-started/terminal-setup.md) - Terminal compatibility guide
