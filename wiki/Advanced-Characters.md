# Multi-Character Configuration

VellumFE supports character-specific configurations, allowing you to maintain separate settings, layouts, and highlights for each character. This guide covers multi-character setup and management.

## Character-Specific Files

### Directory Structure

```
~/.vellum-fe/
├── configs/
│   ├── default.toml              # Default config (fallback)
│   ├── Character1.toml           # Character 1 config
│   ├── Character2.toml           # Character 2 config
│   └── Character3.toml           # Character 3 config
├── layouts/
│   ├── default.toml              # Default layout
│   ├── Character1.toml           # Character 1 layout
│   ├── auto_Character1.toml      # Auto-saved layout
│   ├── Character2.toml           # Character 2 layout
│   └── auto_Character2.toml      # Auto-saved layout
├── debug.log                      # Default debug log
├── debug_Character1.log          # Character 1 debug log
└── debug_Character2.log          # Character 2 debug log
```

## Using Character-Specific Configs

### Command-Line Flag

Launch VellumFE with the `--character` flag:

```bash
.\vellumfe.exe --port 8001 --character Nisugi
```

This:
1. Loads `~/.vellum-fe/configs/Nisugi.toml` (creates if doesn't exist)
2. Loads `~/.vellum-fe/layouts/auto_Nisugi.toml` or `Nisugi.toml`
3. Writes logs to `~/.vellum-fe/debug_Nisugi.log`

### Without Character Flag

If you don't specify `--character`:
1. Loads `~/.vellum-fe/configs/default.toml`
2. Loads `~/.vellum-fe/layouts/default.toml`
3. Writes logs to `~/.vellum-fe/debug.log`

## Loading Priority

### Config Loading

When you specify `--character CharacterName`:

1. `~/.vellum-fe/configs/CharacterName.toml` (if exists)
2. `~/.vellum-fe/configs/default.toml` (if exists)
3. Embedded defaults (compiled into VellumFE)

**Example:** Launching with `--character Nisugi`:
- VellumFE loads `Nisugi.toml`
- Settings in `Nisugi.toml` override `default.toml`
- Settings in `default.toml` override embedded defaults
- Missing settings use embedded defaults

### Layout Loading

When you specify `--character CharacterName`:

1. `~/.vellum-fe/layouts/auto_CharacterName.toml` (auto-saved on exit)
2. `~/.vellum-fe/layouts/CharacterName.toml`
3. `~/.vellum-fe/layouts/default.toml`
4. Embedded default layout

**Auto-save has highest priority!** To use a saved layout:
```bash
del ~/.vellum-fe/layouts/auto_CharacterName.toml
```

## Character-Specific Use Cases

### Different Color Schemes

Each character can have different preset colors:

**Warrior (Character1.toml):**
```toml
[[presets]]
id = "speech"
fg = "#ff0000"  # Red theme
```

**Wizard (Character2.toml):**
```toml
[[presets]]
id = "speech"
fg = "#0000ff"  # Blue theme
```

### Different Layouts

**Combat character:**
- Large main window
- Compact vitals
- Combat-focused windows

**Social character:**
- Large chat windows
- Smaller main window
- Social-focused layout

### Different Highlights

**Hunting character:**
```toml
[[highlights]]
name = "loot"
pattern = "\\b(?:box|chest|trunk)\\b"
fg_color = "#ffaa00"
sound_file = "C:\\Sounds\\loot.wav"
```

**Crafting character:**
```toml
[[highlights]]
name = "crafting"
pattern = "^You (?:forge|craft|smith)"
fg_color = "#00ff00"
```

### Different Keybinds

**Warrior:**
```toml
[[keybinds]]
key = "F1"
action_type = "macro"
action = "stance offensive"

[[keybinds]]
key = "F2"
action_type = "macro"
action = "berserk"
```

**Wizard:**
```toml
[[keybinds]]
key = "F1"
action_type = "macro"
action = "prepare 901"

[[keybinds]]
key = "F2"
action_type = "macro"
action = "cast"
```

## Setting Up Multiple Characters

### Step 1: Launch with Character Name

```bash
.\vellumfe.exe --port 8001 --character Character1
```

### Step 2: Configure Settings

```bash
.settings
# Customize colors, sounds, etc.
```

Changes save to `~/.vellum-fe/configs/Character1.toml`.

### Step 3: Arrange Layout

- Move/resize windows
- Create character-specific windows
- Set up vitals, timers, etc.

### Step 4: Save Layout

```bash
.savelayout
```

Or just quit—auto-save creates `auto_Character1.toml`.

### Step 5: Repeat for Other Characters

```bash
.\vellumfe.exe --port 8002 --character Character2
# Configure Character2...
```

## Managing Multiple Characters

### Checking Current Character

Your character name appears in:
- Window titles (if configured)
- Debug log filename
- Config file being used

### Switching Between Characters

Simply launch with different `--character`:

```bash
# Play Character1
.\vellumfe.exe --port 8001 --character Character1

# Later, play Character2
.\vellumfe.exe --port 8002 --character Character2
```

### Copying Configs

To copy one character's config to another:

**Windows:**
```bash
copy C:\Users\YourName\.vellum-fe\configs\Character1.toml C:\Users\YourName\.vellum-fe\configs\Character2.toml
```

**Linux/Mac:**
```bash
cp ~/.vellum-fe/configs/Character1.toml ~/.vellum-fe/configs/Character2.toml
```

Then customize Character2's config as needed.

### Sharing Settings

To share some settings but keep others separate:

1. Put common settings in `default.toml`
2. Put character-specific settings in `CharacterName.toml`

**Example:**

**default.toml:**
```toml
[connection]
host = "127.0.0.1"

[ui]
poll_timeout_ms = 16
```

**Character1.toml:**
```toml
# Inherits connection and ui from default.toml

[[presets]]
id = "speech"
fg = "#ff0000"  # Character1-specific color
```

**Character2.toml:**
```toml
# Inherits connection and ui from default.toml

[[presets]]
id = "speech"
fg = "#0000ff"  # Character2-specific color
```

## Running Multiple Characters Simultaneously

VellumFE supports multiple instances:

### Different Ports

Each character needs its own port:

**Character 1:**
```bash
# Lich:
--detachable-client=8001

# VellumFE:
.\vellumfe.exe --port 8001 --character Character1
```

**Character 2:**
```bash
# Lich:
--detachable-client=8002

# VellumFE:
.\vellumfe.exe --port 8002 --character Character2
```

### Separate Terminals

Run each VellumFE instance in its own terminal window.

**Windows Terminal:** Create multiple tabs/panes

**Tmux/Screen (Linux/Mac):** Create multiple sessions

### Window Management

With multiple characters running:
- Each has its own VellumFE window
- Each has its own config/layout
- Each has its own debug log

## Troubleshooting Multi-Character Setup

### Wrong Config Loading

**Problem:** Character1 loads Character2's config

**Solution:** Check `--character` flag matches exactly (case-sensitive):
```bash
# Wrong
--character character1

# Correct
--character Character1
```

### Configs Not Separate

**Problem:** Changes to one character affect another

**Solution:** Ensure separate config files exist:
```bash
ls ~/.vellum-fe/configs/
# Should see Character1.toml, Character2.toml, etc.
```

### Layout Confusion

**Problem:** Wrong layout loads for character

**Solution:**
1. Check auto-save isn't interfering:
   ```bash
   del ~/.vellum-fe/layouts/auto_CharacterName.toml
   ```

2. Save layout explicitly:
   ```bash
   .savelayout
   ```

### Port Conflicts

**Problem:** Can't connect, port in use

**Solution:** Each character needs unique port:
- Character1: port 8001
- Character2: port 8002
- Character3: port 8003

### Debug Log Confusion

**Problem:** Can't find character's debug log

**Solution:** Logs are named by character:
```
debug_Character1.log
debug_Character2.log
```

Without `--character`, log is `debug.log`.

## Best Practices

### Naming Convention

Use consistent character names:
```bash
--character Nisugi       # Good
--character nisugi       # Different from above!
```

Case matters in filenames.

### Backup Configs

Backup character configs before major changes:

```bash
copy ~/.vellum-fe/configs/Character1.toml ~/.vellum-fe/configs/Character1.toml.backup
```

### Default as Template

Use `default.toml` as a base template:
1. Configure `default.toml` with common settings
2. Create character configs with only overrides
3. Smaller character configs, easier to manage

### Version Control

Consider using Git for configs:

```bash
cd ~/.vellum-fe/configs
git init
git add *.toml
git commit -m "Initial configs"
```

Track changes, revert mistakes, share with others.

### Documentation

Add comments to character configs:

```toml
# Character1 - Warrior
# Primary role: Combat
# Color scheme: Red theme
# Last updated: 2025-01-20

[[presets]]
id = "speech"
fg = "#ff0000"
```

## Example Multi-Character Workflows

### Two-Character Hunting

**Main character (Character1):**
- Full layout with all windows
- Highlights for loot, combat
- Keybinds for combat actions

**Support character (Character2):**
- Minimal layout
- Highlights for group messages
- Keybinds for healing/buffing

### Profession-Specific Configs

**Warrior:**
```toml
# Combat focus
[[highlights]]
name = "roundtime"
pattern = "^Roundtime:"
fg_color = "#ff0000"
sound_file = "C:\\Sounds\\rt.wav"
```

**Wizard:**
```toml
# Magic focus
[[highlights]]
name = "mana"
pattern = "not enough mana"
fg_color = "#ff0000"
sound_file = "C:\\Sounds\\oom.wav"
```

### Theme Per Character

Give each character a distinct visual theme:

**Character1 - Fire theme:**
```toml
[[presets]]
id = "speech"
fg = "#ff4400"

[[presets]]
id = "bonus"
fg = "#ff8800"
```

**Character2 - Ice theme:**
```toml
[[presets]]
id = "speech"
fg = "#4488ff"

[[presets]]
id = "bonus"
fg = "#88ccff"
```

**Character3 - Nature theme:**
```toml
[[presets]]
id = "speech"
fg = "#44ff44"

[[presets]]
id = "bonus"
fg = "#88ff88"
```

## See Also

- [Configuration](Configuration.md) - Config file format
- [Windows and Layouts](Windows-and-Layouts.md) - Layout management
- [Getting Started](Getting-Started.md) - Basic setup
- [FAQ](FAQ.md) - Common questions
