# Config Cheatsheet

Quick reference for all configuration files.

## config.toml

```toml
[connection]
host = "127.0.0.1"      # Lich proxy host
port = 8000             # Lich proxy port
auto_reconnect = true   # Reconnect on disconnect
reconnect_delay = 5     # Seconds between attempts

[logging]
enabled = true
level = "info"          # debug, info, warn, error
file = "vellum-fe.log"

[sound]
enabled = true
volume = 0.7            # 0.0 - 1.0

[tts]
enabled = false
voice = "default"
rate = 1.0

[display]
timestamps = false
compact_mode = false
show_borders = true
```

## layout.toml

```toml
[layout]
name = "Default"
columns = 100
rows = 40

# Main text window
[[widgets]]
type = "text"
name = "main"
stream = "main"
x = 0
y = 0
width = 70
height = 35
buffer_size = 2000

# Command input
[[widgets]]
type = "input"
name = "command"
x = 0
y = 35
width = 100
height = 3

# Vitals
[[widgets]]
type = "progress"
name = "health"
data_source = "vitals.health"
x = 70
y = 0
width = 30
height = 1

# Compass
[[widgets]]
type = "compass"
name = "compass"
x = 70
y = 5
width = 15
height = 5
```

## Widget Types Quick Reference

| Type | Purpose | Key Properties |
|------|---------|----------------|
| `text` | Text display | `stream`, `buffer_size` |
| `tabbed_text` | Multi-stream tabs | `tabs`, `streams` |
| `input` | Command entry | `history_size` |
| `progress` | Health/mana bars | `data_source`, `color` |
| `countdown` | RT/CT timers | `countdown_id` |
| `compass` | Direction display | `style` |
| `hands` | Equipment display | - |
| `indicator` | Status icons | `indicators` |
| `injury` | Body diagram | - |
| `effects` | Buff/debuff list | `category` |
| `room` | Room display | - |
| `dashboard` | Combined view | `components` |

## highlights.toml

```toml
# Monster highlighting
[[highlights]]
pattern = "\\b(orc|troll|goblin)\\b"
fg = "red"
bold = true

# Speech highlighting
[[highlights]]
pattern = "^\\w+ (says|asks|exclaims)"
fg = "cyan"

# Loot highlighting
[[highlights]]
pattern = "(silver|coins|silvers)"
fg = "bright_yellow"

# Stun warning
[[highlights]]
pattern = "(?i)you are stunned"
fg = "black"
bg = "yellow"
bold = true
flash = true
```

## keybinds.toml

```toml
# Send command on keypress
[[keybinds]]
key = "F5"
action = "send"
command = "look"

# Multi-command macro
[[keybinds]]
key = "Ctrl+H"
action = "send"
command = "health;mana;stamina"

# Window focus
[[keybinds]]
key = "Ctrl+1"
action = "focus_window"
window = "main"
```

## colors.toml

```toml
[colors]
# Standard colors
black = "#000000"
red = "#CC0000"
green = "#00CC00"
yellow = "#CCCC00"
blue = "#0000CC"
magenta = "#CC00CC"
cyan = "#00CCCC"
white = "#CCCCCC"

# Bright colors
bright_black = "#666666"
bright_red = "#FF0000"
bright_green = "#00FF00"
bright_yellow = "#FFFF00"
bright_blue = "#0000FF"
bright_magenta = "#FF00FF"
bright_cyan = "#00FFFF"
bright_white = "#FFFFFF"

# Custom colors
health = "#00FF00"
health_low = "#FFFF00"
health_critical = "#FF0000"
mana = "#0066FF"
stamina = "#FF6600"
spirit = "#9900FF"

# UI colors
background = "#000000"
foreground = "#CCCCCC"
border = "#444444"
border_focused = "#00AAFF"
```

## Data Sources

For `progress` and `countdown` widgets:

| Source | Description |
|--------|-------------|
| `vitals.health` | Health bar |
| `vitals.mana` | Mana bar |
| `vitals.stamina` | Stamina bar |
| `vitals.spirit` | Spirit bar |
| `roundtime` | Roundtime countdown |
| `casttime` | Casttime countdown |
| `encumbrance` | Load level |

## Stream IDs

For `text` widgets:

| Stream | Content |
|--------|---------|
| `main` | Primary game output |
| `room` | Room descriptions |
| `speech` | Player dialogue |
| `thoughts` | ESP/telepathy |
| `combat` | Combat messages |
| `death` | Death messages |
| `logons` | Arrivals/departures |
| `familiar` | Familiar messages |
| `group` | Group info |
| `loot` | Loot messages |
| `inv` | Inventory |
