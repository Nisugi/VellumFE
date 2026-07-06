# config.toml

General client settings: connection, UI behavior, sound, TTS, and the web
server. Most values can also be changed in-app via `.settings`. Apply file
edits with `.reload settings`.

## Connection

```toml
[connection]
host = "127.0.0.1"
port = 8001
character = "YourName"

# For direct connection (optional - can use CLI instead)
account = "your_account"
password = "your_password"  # Stored in plain text!
game = "prime"              # prime, platinum, shattered, test, dr, ...
```

> **Tip**: For security, omit `password` — VellumFE prompts for it securely
> at startup in direct mode. CLI arguments override these values.

## User Interface

```toml
[ui]
buffer_size = 1000              # Default lines kept per window
border_style = "single"         # single, double, rounded, thick, none
countdown_icon = "█"            # Glyph for RT/CT timer blocks
color_mode = "direct"           # direct, slot, indexed (see below)

# Text selection
selection_enabled = true
selection_respect_window_boundaries = true
selection_auto_copy = true      # Copy on mouse-up

# Commands
command_echo = true             # Show sent commands in main window
min_command_length = 3          # Min length to save in history

# Drag modifier for moving windows
drag_modifier_key = "ctrl"      # ctrl, alt, or shift

# Prevent specific server dialogs from auto-opening windows
open_dialog_blocklist = ["bank", "combat", "injuries"]
```

### Color Modes

| Mode | Description |
|------|-------------|
| `direct` | 24-bit true color. Use with modern terminals (kitty, alacritty, Windows Terminal) |
| `slot` | 256-color with custom palette via `.setpalette`. For terminals supporting OSC 4 |
| `indexed` | 256-color with standard palette (closest match). Safe fallback |

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
cooldown_ms = 500               # Min time between repeated sounds
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

TTS navigation keys are bound in [keybinds.toml](./keybinds-toml.md)
(defaults: `Ctrl+Alt+arrows`, `F7`–`F11`).

## Web Server (Mobile Frontend)

Embedded HTTP + WebSocket server that lets a phone browser join the
session. Off by default; see [Mobile Web](../frontends/web.md).

```toml
[web]
enabled = false
port = 8040
bind = "127.0.0.1"    # set "0.0.0.0" to allow phones on your LAN
```

> **Security**: there is no authentication yet. Only bind to `0.0.0.0` on a
> trusted LAN; for off-LAN play use Tailscale/WireGuard. Never expose the
> port to the open internet.

## Quickbars

Define custom quickbar windows that send commands:

```toml
[quickbars]
default = "quick-custom"

[[quickbars.custom]]
id = "quick-custom"     # must be "quick" or start with "quick-"
title = "Custom"
entries = [
  { type = "link", label = "look", command = "look" },
  { type = "sep" },
  { type = "link", label = "inventory", command = "inventory" }
]
```

## Stream Routing

Control how text streams without a subscribed window are handled:

```toml
[streams]
# Streams to silently discard (prevents duplicates/noise)
drop_unsubscribed = [
  "speech", "whisper", "talk", "conversation",
  "targetcount", "playercount", "targetlist", "playerlist"
]

fallback = "main"               # Route unknown streams here
room_in_main = true             # Show room text in main (DR only)
```

## Logging

Capture raw XML for debugging (written to `profiles/<character>/logs/`):

```toml
[logging]
enabled = false
# dir = "logs"
# timestamps = true
# max_lines_per_file = 30000
```

## Layout Mappings

Automatically switch layouts based on terminal size:

```toml
[[layout_mappings]]
min_width = 80
min_height = 24
max_width = 120
max_height = 40
layout = "compact"
```

## Event Patterns

Regex patterns that drive countdown timers (stun/RT/CT). The defaults
cover standard stun messages; add your own:

```toml
[event_patterns.stun_rounds]
pattern = '^\s*You are stunned for ([0-9]+) rounds?'
event_type = "stun"             # stun, rt, ct
action = "set"                  # set or clear
duration_capture = 1            # capture group holding the duration
duration_multiplier = 5.0       # rounds -> seconds
enabled = true
```
