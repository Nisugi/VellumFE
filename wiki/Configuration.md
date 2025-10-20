# Configuration Guide

VellumFE uses TOML configuration files for all settings. This guide covers the configuration system and how to customize your experience.

## Configuration Files

### File Locations

All configuration files are stored in the `~/.vellum-fe/` directory:

**Windows:**
```
C:\Users\YourName\.vellum-fe\
```

**Linux/Mac:**
```
~/.vellum-fe/
```

### Directory Structure

```
~/.vellum-fe/
├── configs/
│   ├── default.toml          # Default configuration
│   └── <character>.toml      # Character-specific configs
├── layouts/
│   ├── default.toml          # Default window layout
│   ├── <character>.toml      # Character layouts
│   └── auto_<character>.toml # Auto-saved layouts
├── debug.log                  # Debug log (default)
└── debug_<character>.log     # Character-specific logs
```

### Loading Priority

**Config Loading:**
1. `~/.vellum-fe/configs/<character>.toml` (if `--character` specified)
2. `~/.vellum-fe/configs/default.toml`
3. Embedded defaults (compiled into VellumFE)

**Layout Loading:**
1. `~/.vellum-fe/layouts/auto_<character>.toml` (auto-saved on exit)
2. `~/.vellum-fe/layouts/<character>.toml`
3. `~/.vellum-fe/layouts/default.toml`
4. Embedded defaults

## Configuration Sections

### Connection Settings

```toml
[connection]
host = "127.0.0.1"  # Lich server address
port = 8000         # Port number (must match Lich's --detachable-client)
```

### UI Settings

```toml
[ui]
command_echo_color = "#ffffff"      # Color for echoed commands
countdown_icon = "\u{f0c8}"         # Icon for countdown blocks (Nerd Font)
poll_timeout_ms = 16                # Event loop timeout (lower = higher FPS, more CPU)
                                    # 16ms = ~60 FPS, 33ms = ~30 FPS

# Tabbed window settings
tab_active_color = "#ffff00"        # Active tab color (yellow)
tab_inactive_color = "#808080"      # Inactive tab color (gray)
tab_unread_color = "#ffffff"        # Unread tab color (white/bold)
tab_unread_prefix = "* "            # Prefix for tabs with unread messages

# Compass colors
compass_active_color = "#00ff00"    # Active exit color (green)
compass_inactive_color = "#333333"  # Inactive exit color (dark gray)
```

### Sound Settings

```toml
[sound]
enabled = true                      # Enable/disable all sounds
volume = 0.5                        # Master volume (0.0 - 1.0)
```

### Preset Colors

Presets define colors for game text styles. Format: `#RRGGBB` for foreground, `#RRGGBB` or `-` for background.

```toml
[[presets]]
id = "speech"                       # Preset identifier
fg = "#53a684"                      # Foreground color (hex)
bg = "-"                            # Background color (- = none)

[[presets]]
id = "thought"
fg = "#9BA2B2"
bg = "#395573"                      # Background color
```

**Common Presets:**
- `speech` - Player speech
- `thought` - Character thoughts
- `whisper` - Whispers
- `room` - Room descriptions
- `watching` - Familiar messages
- `penalty` - Penalties/debuffs
- `bonus` - Bonuses/buffs

### Spell Colors

Override colors for specific spells:

```toml
[[spell_colors]]
spell_number = 906                  # Spell number
color = "#ff0000"                   # Color override
```

### Prompt Colors

Customize prompt indicator colors (R/C/S/L/M):

```toml
[[ui.prompt_colors]]
character = "R"                     # Prompt character (R = roundtime)
color = "#ff0000"                   # Color (red)

[[ui.prompt_colors]]
character = "C"                     # C = casting
color = "#0000ff"                   # Blue

[[ui.prompt_colors]]
character = "S"                     # S = stunned
color = "#ffff00"                   # Yellow
```

## Editing Configuration

### Method 1: Settings Editor (Recommended)

1. Launch VellumFE
2. Type `.settings` and press Enter
3. Navigate with arrow keys
4. Press Enter or Space to edit/toggle values
5. Changes save immediately

**Settings Categories:**
- **Connection** - Host, port
- **UI** - Colors, icons, poll timeout, tab colors
- **Sound** - Enable/disable, volume
- **Presets** - Text style colors
- **Spells** - Spell-specific colors
- **Prompts** - Prompt indicator colors

### Method 2: Manual Editing

1. Exit VellumFE
2. Open `~/.vellum-fe/configs/default.toml` in a text editor
3. Make changes
4. Save and relaunch VellumFE

**Important:** Some settings (like presets) require restart to take effect.

## Character-Specific Configs

To maintain separate settings per character, use the `--character` flag:

```bash
.\vellumfe.exe --port 8001 --character Nisugi
```

This creates and loads `~/.vellum-fe/configs/Nisugi.toml`.

**Benefits:**
- Different color schemes per character
- Character-specific layouts
- Separate debug logs
- Independent highlight sets

**Workflow:**
1. Launch with `--character YourName`
2. Configure settings with `.settings`
3. Arrange windows and save layout
4. Next launch automatically loads your character's config

## Advanced Configuration

### Performance Tuning

The `poll_timeout_ms` setting controls event loop frequency:

```toml
[ui]
poll_timeout_ms = 16  # ~60 FPS (default)
```

**Recommendations:**
- **High FPS (16ms)** - Smooth scrolling, responsive, higher CPU usage
- **Medium FPS (33ms)** - Good balance of smoothness and CPU
- **Low FPS (50ms+)** - Lower CPU usage, less smooth

### Color Format

All colors use hex format: `#RRGGBB`

**Examples:**
- `#ff0000` - Red
- `#00ff00` - Green
- `#0000ff` - Blue
- `#ffffff` - White
- `#000000` - Black
- `#ffff00` - Yellow
- `#ff00ff` - Magenta
- `#00ffff` - Cyan

Use `-` for "no color" (transparent background).

### Nerd Font Icons

The `countdown_icon` supports Nerd Font Unicode characters:

```toml
countdown_icon = "\u{f0c8}"  # Square (default)
countdown_icon = "\u{f111}"  # Circle
countdown_icon = "\u{f068}"  # Minus
countdown_icon = "█"         # Block
```

Requires a Nerd Font-compatible terminal font.

## Configuration Examples

### High Contrast Theme

```toml
[ui]
command_echo_color = "#ffffff"

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
```

### Dark Theme with Subtle Colors

```toml
[ui]
command_echo_color = "#888888"

[[presets]]
id = "speech"
fg = "#53a684"
bg = "#1a1a1a"

[[presets]]
id = "thought"
fg = "#6b7d9c"
bg = "#1a1a1a"
```

### Performance Optimized

```toml
[ui]
poll_timeout_ms = 50  # Lower FPS, less CPU

# Disable expensive features
[sound]
enabled = false
```

## Resetting Configuration

To reset to defaults:

1. **Single character:**
   ```bash
   del ~/.vellum-fe/configs/YourCharacter.toml
   ```

2. **All configs:**
   ```bash
   del ~/.vellum-fe/configs/*
   ```

3. **Everything (nuclear option):**
   ```bash
   rmdir /s ~/.vellum-fe
   ```

VellumFE will recreate default configs on next launch.

## See Also

- [Themes and Colors](Themes-and-Colors.md) - Color scheme examples
- [Windows and Layouts](Windows-and-Layouts.md) - Window configuration
- [Advanced Characters](Advanced-Characters.md) - Multi-character setup
- [Troubleshooting](Troubleshooting.md) - Config issues
