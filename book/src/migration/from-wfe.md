# Migrating from Wizard Front End (WFE)

Guide for WFE users transitioning to VellumFE.

## Overview

WFE (Wizard Front End) is a graphical Windows client. VellumFE is a terminal-based client, so the visual experience differs, but functionality translates.

## Key Differences

| Aspect | WFE | VellumFE |
|--------|-----|----------|
| Platform | Windows GUI | Terminal (cross-platform) |
| Interface | Graphical windows | Text-based widgets |
| Scripting | Built-in | Via Lich |
| Configuration | GUI settings | TOML files |

## Feature Mapping

### Windows to Widgets

| WFE Window | VellumFE Widget |
|------------|-----------------|
| Main Game Window | `text` widget |
| Inventory Window | `inventory` widget |
| Status Window | `indicator` widget |
| Compass | `compass` widget |
| Health/Mana Bars | `progress` widgets |

### Configuration

| WFE | VellumFE |
|-----|----------|
| Settings dialogs | `config.toml` |
| Window layout | `layout.toml` |
| Color settings | `colors.toml` |
| Macros | `keybinds.toml` |

## Macros Translation

### WFE Macro

```
F1 = attack target
Ctrl+1 = prep 101;cast
```

### VellumFE Keybind

```toml
[keybinds."f1"]
macro = "attack target"

[keybinds."ctrl+1"]
macro = "prep 101;cast"
```

### Macro Variables

| WFE | VellumFE |
|-----|----------|
| `%0` (input) | `$input` |
| `%target` | `$target` |
| Pause 500ms | `{500}` |

## Highlights Translation

### WFE Highlight

```
Text: "stunned" - Color: Yellow, Bold
```

### VellumFE Highlight

```toml
[[highlights]]
pattern = "stunned"
fg = "bright_yellow"
bold = true
```

## Triggers Translation

### WFE Trigger

```
Pattern: "You are stunned"
Action: Play sound stun.wav
```

### VellumFE Trigger

```toml
[[triggers]]
pattern = "You are stunned"
command = ".sound stun.wav"
```

## Layout Translation

WFE uses absolute pixel positions. VellumFE uses percentages.

### WFE Layout Concept

```
Main Window: 0,0 - 600x400
Side Panel: 605,0 - 200x400
```

### VellumFE Equivalent

```toml
# Estimate percentages

[[widgets]]
type = "text"
name = "main"
x = 0
y = 0
width = 75
height = 90

[[widgets]]
type = "indicator"
name = "status"
x = 76
y = 0
width = 24
height = 20
```

## Step-by-Step Migration

### 1. Document WFE Settings

Note:
- Window sizes and positions
- Macro assignments
- Highlight patterns
- Trigger patterns
- Color preferences

### 2. Install VellumFE

Follow [installation guide](../getting-started/installation.md).

### 3. Set Up Lich Connection

If you weren't using Lich with WFE, you'll need to:
1. Install Lich
2. Configure Lich with your credentials
3. Connect VellumFE through Lich

```bash
vellum-fe --host 127.0.0.1 --port 8000
```

Or use direct mode:
```bash
vellum-fe --direct --account USER --password PASS --game prime --character CHAR
```

### 4. Create Basic Layout

```toml
# ~/.vellum-fe/layout.toml

# Main window
[[widgets]]
type = "text"
name = "main"
x = 0
y = 0
width = 70
height = 85
streams = ["main", "room", "combat"]

# Health bar
[[widgets]]
type = "progress"
name = "health"
title = "HP"
x = 71
y = 0
width = 29
height = 4
data_source = "vitals.health"
color = "health"

# Mana bar
[[widgets]]
type = "progress"
name = "mana"
title = "MP"
x = 71
y = 5
width = 29
height = 4
data_source = "vitals.mana"
color = "mana"

# Compass
[[widgets]]
type = "compass"
name = "compass"
x = 71
y = 10
width = 29
height = 12
style = "unicode"

# Command input
[[widgets]]
type = "command_input"
name = "input"
x = 0
y = 86
width = 100
height = 14
```

### 5. Migrate Macros

```toml
# ~/.vellum-fe/keybinds.toml

# Movement
[keybinds."numpad8"]
macro = "north"

[keybinds."numpad2"]
macro = "south"

# Combat
[keybinds."f1"]
macro = "attack target"

# Spells
[keybinds."ctrl+1"]
macro = "prep 101;cast"
```

### 6. Migrate Colors

```toml
# ~/.vellum-fe/colors.toml

[theme]
background = "#000000"
text = "#ffffff"

health = "#00ff00"
mana = "#0000ff"
stamina = "#ffff00"

combat = "#ff0000"
speech = "#00ffff"
```

### 7. Migrate Triggers

```toml
# ~/.vellum-fe/triggers.toml

[[triggers]]
name = "stun_alert"
pattern = "You are stunned"
command = ".notify STUNNED!"
cooldown = 1000

[[triggers]]
name = "death_search"
pattern = "falls dead"
command = "search"
enabled = false  # Enable when ready
```

## Visual Differences

### GUI vs Terminal

WFE's graphical interface provides:
- Clickable buttons
- Drag-and-drop
- Visual menus

VellumFE's terminal interface provides:
- Keyboard-centric operation
- Works over SSH
- Lower resource usage

### Adapting Workflow

| WFE Action | VellumFE Action |
|------------|-----------------|
| Click compass direction | Numpad navigation |
| Right-click context menu | `.cmdlist` system |
| Toolbar buttons | Function key macros |
| Settings dialog | Edit TOML files |

## Scripting Differences

### WFE Built-in Scripting

WFE has built-in scripting capabilities.

### VellumFE + Lich

VellumFE relies on Lich for scripting:

```
# In game with Lich
;script_name      # Run script
;kill script      # Stop script
;list             # List scripts
```

## What You Gain

Moving to VellumFE:

- **Cross-platform**: Works on Windows, macOS, Linux
- **SSH access**: Play remotely via terminal
- **Customizability**: Deep configuration options
- **Modern rendering**: Unicode support
- **Open source**: Community improvements

## What You Lose

- **Visual GUI**: No graphical interface
- **Built-in scripting**: Must use Lich
- **Click-based operation**: Keyboard-focused

## Tips for GUI Users

### Learn Terminal Basics

- Arrow keys navigate
- Tab cycles widgets
- Page Up/Down scrolls
- Type commands directly

### Embrace Keyboard

Most efficient VellumFE users:
- Memorize key macros
- Use numpad for movement
- Minimize mouse usage

### Gradual Transition

Consider running both clients initially while learning VellumFE.

## See Also

- [Your First Layout](../tutorials/your-first-layout.md)
- [Keybinds Configuration](../configuration/keybinds-toml.md)
- [Lich Proxy](../network/lich-proxy.md)

