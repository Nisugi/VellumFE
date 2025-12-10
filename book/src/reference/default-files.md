# Default Files

Default configuration file contents for reference.

## config.toml

```toml
# VellumFE Configuration
# Default settings

[connection]
mode = "lich"
host = "127.0.0.1"
port = 8000
auto_reconnect = true
reconnect_delay = 5

[tts]
enabled = false
engine = "default"
voice = "default"
rate = 1.0
volume = 1.0
speak_room_descriptions = true
speak_combat = false
speak_speech = true
speak_whispers = true
speak_thoughts = false

[logging]
level = "info"
file = "~/.vellum-fe/vellum-fe.log"

[performance]
render_rate = 60
batch_updates = true
lazy_render = true
```

## layout.toml

```toml
# VellumFE Layout
# Default widget arrangement

# Main game text window
[[widgets]]
type = "text"
name = "main"
title = "Game"
x = 0
y = 0
width = 75
height = 85
streams = ["main", "room", "combat", "speech", "thoughts"]
scrollback = 5000
auto_scroll = true

# Health bar
[[widgets]]
type = "progress"
name = "health"
title = "HP"
x = 76
y = 0
width = 24
height = 3
data_source = "vitals.health"
color = "health"
show_text = true
show_percentage = true

# Mana bar
[[widgets]]
type = "progress"
name = "mana"
title = "MP"
x = 76
y = 4
width = 24
height = 3
data_source = "vitals.mana"
color = "mana"
show_text = true
show_percentage = true

# Stamina bar
[[widgets]]
type = "progress"
name = "stamina"
title = "ST"
x = 76
y = 8
width = 24
height = 3
data_source = "vitals.stamina"
color = "stamina"
show_text = true
show_percentage = true

# Spirit bar
[[widgets]]
type = "progress"
name = "spirit"
title = "SP"
x = 76
y = 12
width = 24
height = 3
data_source = "vitals.spirit"
color = "spirit"
show_text = true
show_percentage = true

# Compass
[[widgets]]
type = "compass"
name = "compass"
x = 76
y = 16
width = 24
height = 10
style = "unicode"
clickable = true

# Roundtime
[[widgets]]
type = "countdown"
name = "roundtime"
title = "RT"
x = 76
y = 27
width = 24
height = 3
data_source = "roundtime"

# Status indicators
[[widgets]]
type = "indicator"
name = "status"
x = 76
y = 31
width = 24
height = 10
indicators = ["hidden", "stunned", "webbed", "prone", "kneeling"]
columns = 2

# Command input
[[widgets]]
type = "command_input"
name = "input"
x = 0
y = 86
width = 100
height = 14
history_size = 500
prompt = "> "
```

## colors.toml

```toml
# VellumFE Colors
# Default theme

[theme]
name = "Default"

# Base colors
background = "#000000"
text = "#c0c0c0"
text_dim = "#808080"

# Borders
border = "#404040"
border_focused = "#ffffff"

# Vitals
health = "#00ff00"
health_low = "#ffff00"
health_critical = "#ff0000"
mana = "#0080ff"
stamina = "#ff8000"
spirit = "#ff00ff"

# Streams
main = "#ffffff"
room = "#ffff00"
combat = "#ff4444"
speech = "#00ffff"
whisper = "#ff00ff"
thoughts = "#00ff00"

# Status indicators
hidden = "#00ff00"
invisible = "#00ffff"
stunned = "#ffff00"
webbed = "#ff00ff"
prone = "#00ffff"
kneeling = "#ff8000"
sitting = "#808080"
dead = "#ff0000"
```

## keybinds.toml

```toml
# VellumFE Keybinds
# Default key mappings

# Navigation - Numpad
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

# Widget navigation
[keybinds."tab"]
action = "next_widget"

[keybinds."shift+tab"]
action = "prev_widget"

[keybinds."escape"]
action = "focus_input"

# Scrolling
[keybinds."page_up"]
action = "scroll_up"

[keybinds."page_down"]
action = "scroll_down"

[keybinds."home"]
action = "scroll_top"

[keybinds."end"]
action = "scroll_bottom"

# Quick commands
[keybinds."f1"]
macro = "look"

[keybinds."f2"]
macro = "inventory"

[keybinds."f3"]
macro = "experience"

# Search
[keybinds."ctrl+f"]
action = "open_search"

# Quit
[keybinds."ctrl+q"]
action = "quit"
```

## highlights.toml

```toml
# VellumFE Highlights
# Default text patterns

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

[[highlights]]
pattern = "falls dead"
fg = "bright_yellow"
bold = true

# Communication
[[highlights]]
pattern = "(\\w+) whispers,"
fg = "magenta"

[[highlights]]
pattern = "(\\w+) says?,"
fg = "cyan"

# Room elements
[[highlights]]
pattern = "^\\[.+\\]$"
fg = "bright_yellow"
bold = true

[[highlights]]
pattern = "Obvious (exits|paths):"
fg = "gray"
```

## triggers.toml

```toml
# VellumFE Triggers
# Default automation (minimal)

# Whisper notification
[[triggers]]
name = "whisper_alert"
pattern = "(\\w+) whispers,"
command = ".notify Whisper from $1"
enabled = false

# Stun notification
[[triggers]]
name = "stun_alert"
pattern = "(?i)you are stunned"
command = ".notify Stunned!"
cooldown = 1000
enabled = false
```

## cmdlist.toml

```toml
# VellumFE Command Lists
# Default context menus

# General items
[[cmdlist]]
noun = ".*"
match_mode = "regex"
commands = ["look", "get", "drop"]
priority = 1

# Creatures
[[cmdlist]]
category = "creature"
noun = ".*"
match_mode = "regex"
commands = ["attack", "look", "assess"]
priority = 10

# Containers
[[cmdlist]]
category = "container"
noun = "(?i)(backpack|bag|pouch|sack|cloak)"
match_mode = "regex"
commands = ["look in", "open", "close"]
priority = 20

# Players
[[cmdlist]]
category = "player"
noun = "^[A-Z][a-z]+$"
match_mode = "regex"
commands = ["look", "smile", "bow", "wave"]
priority = 30
```

## Generating Defaults

To generate default configuration files:

```bash
# Dump defaults to stdout
vellum-fe --dump-config

# Create default files
vellum-fe --dump-config > ~/.vellum-fe/config.toml
```

## Restoring Defaults

To restore a file to defaults:

```bash
# Backup current config
cp ~/.vellum-fe/config.toml ~/.vellum-fe/config.toml.bak

# Generate fresh default
vellum-fe --dump-config > ~/.vellum-fe/config.toml
```

Or delete the file - VellumFE will use built-in defaults.

## See Also

- [Configuration](../configuration/README.md)
- [Config Schema](./config-schema.md)
- [Your First Layout](../tutorials/your-first-layout.md)

