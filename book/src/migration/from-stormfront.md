# Migrating from StormFront

Guide for StormFront users transitioning to VellumFE.

## Overview

StormFront is Simutronics' official graphical client. VellumFE is an alternative terminal-based client with different strengths.

## Key Differences

| Aspect | StormFront | VellumFE |
|--------|------------|----------|
| Type | Official GUI client | Third-party terminal |
| Platform | Windows/macOS | Cross-platform terminal |
| Scripting | Limited | Via Lich |
| Customization | Limited | Extensive |
| Price | Free | Free |

## Feature Comparison

### Windows

| StormFront | VellumFE |
|------------|----------|
| Game Window | `text` widget |
| Thoughts Window | `text` widget (thoughts stream) |
| Inventory Panel | `inventory` widget |
| Status Bar | `indicator` widget |

### Visual Elements

| StormFront | VellumFE |
|------------|----------|
| Health/Mana bars | `progress` widgets |
| Compass rose | `compass` widget |
| Character portrait | Not supported |
| Spell icons | `active_effects` widget |

## Why Switch?

### Advantages of VellumFE

- **Lich scripting**: Full automation capability
- **Deep customization**: Complete control over appearance
- **Terminal access**: Play over SSH
- **Cross-platform**: Works everywhere
- **Lightweight**: Low resource usage

### What You Keep

- Game experience
- Character data
- Account access
- In-game settings

### What Changes

- Visual appearance
- Configuration method
- Scripting approach

## Step-by-Step Migration

### 1. Choose Connection Method

**Option A: Lich Proxy (Recommended)**

Install Lich for scripting and automation:
1. Install Lich and Ruby
2. Configure Lich with your account
3. Connect VellumFE through Lich

**Option B: Direct Connection**

Connect without Lich (no scripts):
```bash
vellum-fe --direct \
  --account YOUR_ACCOUNT \
  --password YOUR_PASSWORD \
  --game prime \
  --character CHARACTER
```

### 2. Install VellumFE

Follow [installation guide](../getting-started/installation.md).

### 3. Create Configuration

Generate default configuration:

```bash
mkdir -p ~/.vellum-fe
vellum-fe --dump-config > ~/.vellum-fe/config.toml
```

### 4. Create Layout

Approximate StormFront layout:

```toml
# ~/.vellum-fe/layout.toml

# Room name at top
[[widgets]]
type = "room"
name = "room"
x = 0
y = 0
width = 100
height = 4
show_exits = true

# Main game text
[[widgets]]
type = "text"
name = "main"
x = 0
y = 4
width = 70
height = 55
streams = ["main", "room"]

# Thoughts/ESP window
[[widgets]]
type = "text"
name = "thoughts"
title = "Thoughts"
x = 0
y = 60
width = 70
height = 30
streams = ["thoughts"]

# Right sidebar - vitals
[[widgets]]
type = "progress"
name = "health"
title = "Health"
x = 71
y = 4
width = 29
height = 4
data_source = "vitals.health"
color = "health"
show_text = true

[[widgets]]
type = "progress"
name = "mana"
title = "Mana"
x = 71
y = 9
width = 29
height = 4
data_source = "vitals.mana"
color = "mana"
show_text = true

[[widgets]]
type = "progress"
name = "stamina"
title = "Stamina"
x = 71
y = 14
width = 29
height = 4
data_source = "vitals.stamina"
color = "stamina"
show_text = true

# Compass
[[widgets]]
type = "compass"
name = "compass"
x = 71
y = 19
width = 29
height = 12
style = "unicode"
clickable = true

# Status indicators
[[widgets]]
type = "indicator"
name = "status"
x = 71
y = 32
width = 29
height = 10
indicators = ["hidden", "stunned", "prone"]

# Active spells
[[widgets]]
type = "active_effects"
name = "spells"
title = "Spells"
x = 71
y = 43
width = 29
height = 20
show_duration = true

# Command input
[[widgets]]
type = "command_input"
name = "input"
x = 0
y = 91
width = 100
height = 9
prompt = "> "
```

### 5. Set Up Keybinds

StormFront uses basic keybinds. Expand with VellumFE:

```toml
# ~/.vellum-fe/keybinds.toml

# Movement (numpad)
[keybinds."numpad8"]
macro = "north"

[keybinds."numpad2"]
macro = "south"

[keybinds."numpad4"]
macro = "west"

[keybinds."numpad6"]
macro = "east"

[keybinds."numpad7"]
macro = "northwest"

[keybinds."numpad9"]
macro = "northeast"

[keybinds."numpad1"]
macro = "southwest"

[keybinds."numpad3"]
macro = "southeast"

[keybinds."numpad5"]
macro = "out"

# Quick commands
[keybinds."f1"]
macro = "look"

[keybinds."f2"]
macro = "inventory"

[keybinds."f3"]
macro = "experience"

# Combat
[keybinds."f5"]
macro = "attack target"

[keybinds."f6"]
macro = "hide"

# Navigation
[keybinds."tab"]
action = "next_widget"

[keybinds."page_up"]
action = "scroll_up"

[keybinds."page_down"]
action = "scroll_down"
```

### 6. Configure Highlights

```toml
# ~/.vellum-fe/highlights.toml

# Critical status
[[highlights]]
pattern = "(?i)you are stunned"
fg = "black"
bg = "yellow"
bold = true

[[highlights]]
pattern = "(?i)webs? (stick|entangle)"
fg = "black"
bg = "magenta"
bold = true

# Combat
[[highlights]]
pattern = "\\*\\* .+ \\*\\*"
fg = "bright_red"
bold = true

# Social
[[highlights]]
pattern = "(\\w+) whispers,"
fg = "magenta"
```

## Visual Adaptation

### StormFront Visual Elements

StormFront provides:
- Graphical status bars
- Clickable compass
- Character portrait
- Spell icons

### VellumFE Equivalents

VellumFE provides text-based versions:
- ASCII/Unicode progress bars
- Clickable text compass
- Status text indicators
- Spell name lists

### Missing Features

Some StormFront features don't have equivalents:
- Character portrait
- Graphical inventory icons
- Animated status effects

## Gaining Lich Scripts

StormFront users often don't use Lich. VellumFE + Lich provides:

### Popular Scripts

- `go2` - Automatic navigation
- `bigshot` - Hunting automation
- `lnet` - Player network
- `repository` - Script management

### Installing Scripts

```
# In-game with Lich
;repository download go2
;go2 bank
```

## What You Gain

- **Customizable interface**: Complete layout control
- **Powerful scripting**: Full Lich ecosystem
- **Terminal access**: Play anywhere
- **Lower resources**: Faster, lighter
- **Community support**: Active development

## Tips for StormFront Users

### Terminal Navigation

- Arrow keys and numpad work intuitively
- Tab cycles between widgets
- Type commands directly in input

### Learn Keyboard Shortcuts

Memorize key bindings for efficiency:
- F-keys for common commands
- Numpad for movement
- Ctrl combinations for macros

### Explore Lich

Lich transforms gameplay:
1. Start with `go2` for navigation
2. Try utility scripts
3. Gradually automate routine tasks

## See Also

- [Getting Started](../getting-started/README.md)
- [Lich Proxy](../network/lich-proxy.md)
- [Your First Layout](../tutorials/your-first-layout.md)

