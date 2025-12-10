# Config Schema

Complete configuration file reference for VellumFE.

## File Overview

| File | Purpose |
|------|---------|
| `config.toml` | Main configuration |
| `layout.toml` | Widget layout |
| `colors.toml` | Color theme |
| `keybinds.toml` | Key bindings |
| `highlights.toml` | Text patterns |
| `triggers.toml` | Automation triggers |

## config.toml

### Connection Section

```toml
[connection]
mode = "lich"           # "lich" or "direct"
host = "127.0.0.1"      # Lich host (lich mode)
port = 8000             # Lich port (lich mode)
auto_reconnect = true   # Auto-reconnect on disconnect
reconnect_delay = 5     # Seconds between reconnect attempts
game = "prime"          # Game instance (direct mode)
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `mode` | string | `"lich"` | Connection mode |
| `host` | string | `"127.0.0.1"` | Proxy host |
| `port` | integer | `8000` | Proxy port |
| `auto_reconnect` | boolean | `true` | Enable auto-reconnect |
| `reconnect_delay` | integer | `5` | Reconnect delay (seconds) |
| `game` | string | `"prime"` | Game instance (direct) |

### TTS Section

```toml
[tts]
enabled = false
engine = "default"      # "sapi", "say", "espeak"
voice = "default"
rate = 1.0              # 0.5 to 2.0
volume = 1.0            # 0.0 to 1.0

speak_room_descriptions = true
speak_combat = false
speak_speech = true
speak_whispers = true
speak_thoughts = false
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | boolean | `false` | Enable TTS |
| `engine` | string | `"default"` | TTS engine |
| `voice` | string | `"default"` | Voice name |
| `rate` | float | `1.0` | Speech rate |
| `volume` | float | `1.0` | Volume level |
| `speak_*` | boolean | varies | Speak stream types |

### Logging Section

```toml
[logging]
level = "info"          # "error", "warn", "info", "debug", "trace"
file = "~/.vellum-fe/vellum-fe.log"
max_size = 10485760     # 10MB
rotate = true
```

### Performance Section

```toml
[performance]
render_rate = 60        # Target FPS
batch_updates = true
lazy_render = true
max_scrollback = 10000
```

## layout.toml

### Widget Definition

```toml
[[widgets]]
type = "text"           # Widget type (required)
name = "main"           # Unique name (required)
x = 0                   # X position (0-100)
y = 0                   # Y position (0-100)
width = 100             # Width (0-100)
height = 100            # Height (0-100)

# Optional common properties
title = "Main"          # Widget title
border = true           # Show border
border_color = "white"  # Border color
focus_order = 1         # Tab order
visible = true          # Initial visibility
```

### Widget Types

#### text

```toml
[[widgets]]
type = "text"
streams = ["main", "room"]
scrollback = 5000
auto_scroll = true
show_timestamps = false
wrap = true
```

#### tabbed_text

```toml
[[widgets]]
type = "tabbed_text"
tabs = [
    { name = "All", streams = ["main", "room", "combat"] },
    { name = "Combat", streams = ["combat"] },
    { name = "Chat", streams = ["speech", "thoughts"] },
]
```

#### progress

```toml
[[widgets]]
type = "progress"
data_source = "vitals.health"
color = "health"
show_text = true
show_percentage = true
```

#### countdown

```toml
[[widgets]]
type = "countdown"
data_source = "roundtime"
warning_threshold = 2
critical_threshold = 0
```

#### compass

```toml
[[widgets]]
type = "compass"
style = "unicode"       # "ascii", "unicode", "minimal"
clickable = true
```

#### indicator

```toml
[[widgets]]
type = "indicator"
indicators = ["hidden", "stunned", "prone"]
columns = 2
```

#### room

```toml
[[widgets]]
type = "room"
show_exits = true
show_creatures = true
show_players = false
```

#### command_input

```toml
[[widgets]]
type = "command_input"
prompt = "> "
history_size = 500
show_roundtime = true
```

## colors.toml

### Theme Definition

```toml
[theme]
name = "Custom Theme"

# Base colors
background = "#000000"
text = "#ffffff"
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
combat = "#ff4444"
speech = "#00ffff"
thoughts = "#00ff00"
whisper = "#ff00ff"
room = "#ffff00"

# Indicators
hidden = "#00ff00"
stunned = "#ffff00"
prone = "#00ffff"
```

### Color Values

Colors accept:
- Preset names: `"red"`, `"bright_blue"`
- Hex codes: `"#ff0000"`, `"#f00"`

## keybinds.toml

### Keybind Definition

```toml
[keybinds."key_name"]
action = "action_name"  # Widget action

# OR

[keybinds."key_name"]
macro = "game command"  # Send command
```

### Key Names

| Category | Examples |
|----------|----------|
| Letters | `"a"`, `"z"`, `"A"` (shift+a) |
| Numbers | `"1"`, `"0"` |
| Function | `"f1"`, `"f12"` |
| Navigation | `"up"`, `"down"`, `"left"`, `"right"` |
| Editing | `"enter"`, `"tab"`, `"backspace"`, `"delete"` |
| Special | `"escape"`, `"space"`, `"home"`, `"end"` |
| Numpad | `"numpad0"`, `"numpad_plus"` |

### Modifiers

Combine with `+`:
- `"ctrl+a"`
- `"shift+f1"`
- `"alt+enter"`
- `"ctrl+shift+s"`

## highlights.toml

### Highlight Definition

```toml
[[highlights]]
pattern = "regex pattern"
fg = "color"            # Foreground color
bg = "color"            # Background color (optional)
bold = false
italic = false
underline = false

# Optional
enabled = true
priority = 100
fast_parse = false      # Use literal matching
```

## triggers.toml

### Trigger Definition

```toml
[[triggers]]
name = "trigger_name"
pattern = "regex pattern"
command = "action"

# Optional
enabled = true
priority = 100
cooldown = 1000         # Milliseconds
category = "combat"
stream = "main"         # Limit to stream
```

## Type Reference

| Type | Format | Example |
|------|--------|---------|
| string | Quoted text | `"value"` |
| integer | Whole number | `100` |
| float | Decimal number | `1.5` |
| boolean | true/false | `true` |
| array | Brackets | `["a", "b"]` |
| table | Section | `[section]` |

## See Also

- [Configuration Guide](../configuration/README.md)
- [Keybind Actions](./keybind-actions.md)
- [Preset Colors](./preset-colors.md)

