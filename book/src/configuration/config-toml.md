# config.toml

General client settings including connection, UI behavior, and sound.

## Connection

```toml
[connection]
host = "127.0.0.1"
port = 8001
character = "YourName"

# For direct connection (optional - can use CLI instead)
account = "your_account"
password = "your_password"  # Stored in plain text!
game = "prime"              # prime, platinum, shattered, test
```

> **Tip**: For security, pass credentials via CLI: `--account X --password Y`

## User Interface

```toml
[ui]
buffer_size = 1000              # Lines kept per window
border_style = "single"         # single, double, rounded, thick, none
color_mode = "direct"           # direct (24-bit), indexed (256-color)

# Text selection
selection_enabled = true
selection_auto_copy = true      # Copy on mouse-up

# Commands
command_echo = true             # Show sent commands in main window
min_command_length = 3          # Min length to save in history

# Drag modifier for moving windows
drag_modifier_key = "ctrl"      # ctrl, alt, or shift
```

### Color Modes

| Mode | Description |
|------|-------------|
| `direct` | 24-bit true color. Use with modern terminals (kitty, alacritty, Windows Terminal) |
| `indexed` | 256-color with standard palette. Fallback for legacy terminals |
| `slot` | 256-color with custom palette. For terminals supporting OSC4 |

## Focus Navigation

Control which windows are focusable with Tab:

```toml
[ui.focus]
types = ["text", "tabbedtext"]  # Widget types that can receive focus
exclude = ["bounty", "society"] # Specific windows to skip
order = []                      # Custom focus order (empty = layout order)
```

## Target List

Configure the targets widget display:

```toml
[target_list]
status_position = "end"         # "end" or "start"
truncation_mode = "noun"        # "full" or "noun"
excluded_nouns = ["arm", "coal"]

[target_list.status_abbrev]
stunned = "stu"
frozen = "frz"
dead = "ded"
```

## Highlights

Global toggles for the highlight system:

```toml
[highlights]
sounds_enabled = true           # Play sounds on match
replace_enabled = true          # Apply text replacements
redirect_enabled = true         # Route lines to other windows
coloring_enabled = true         # Apply color highlighting
```

## Sound

```toml
[sound]
enabled = true
volume = 0.7                    # 0.0 to 1.0
cooldown_ms = 500               # Min time between sounds
startup_music = true
```

## Text-to-Speech

```toml
[tts]
enabled = false
rate = 1.0                      # 0.5 (slow) to 2.0 (fast)
volume = 1.0
speak_thoughts = true
speak_speech = true
speak_main = false              # Usually too noisy
```

## Stream Routing

Control how unsubscribed text streams are handled:

```toml
[streams]
# Streams to silently discard
drop_unsubscribed = [
  "speech", "whisper", "talk",
  "targetcount", "playercount"
]

fallback = "main"               # Route unknown streams here
room_in_main = true             # Show room text in main (DR only)
```

## Logging

Capture raw XML for debugging:

```toml
[logging]
enabled = false
# dir = "logs"
# timestamps = true
```

## Event Patterns

Regex patterns for countdown timers:

```toml
[event_patterns.stun_rounds]
pattern = '^\s*You are stunned for ([0-9]+) rounds?'
event_type = "stun"
action = "set"
duration_capture = 1
duration_multiplier = 5.0
enabled = true
```
