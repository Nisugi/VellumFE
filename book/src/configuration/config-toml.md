# config.toml

General client settings: connection, UI behavior, sound, TTS, and the web
server. Most values can also be changed in-app via `.settings`. Apply file
edits with `.reload settings`.

## Connection

```toml
[connection]
host = "127.0.0.1"
port = 8000
character = "YourName"

# For direct connection (optional - can use CLI instead)
account = "your_account"
password = "your_password"  # Stored in plain text!
game = "prime"              # GS4: prime, platinum, shattered, test
                            # DR: dr, drplatinum, drfallen, drtest
```

> **Tip**: For security, omit `password` — VellumFE prompts for it securely
> at startup in direct mode. CLI arguments override these values.

## User Interface

```toml
[ui]
buffer_size = 10000             # Default lines kept per window
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
# (default blocks combat, injuries, stance, minivitals, and other
# dialogs that have dedicated widgets)
open_dialog_blocklist = ["combat", "injuries", "stance"]
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
boss_color = "#ff5555"          # AscensionBoss / MiniBoss creatures
challenging_color = "#ffaa55"   # "challenging" creatures

[target_list.status_abbrev]
stunned = "stu"
frozen = "frz"
dead = "ded"
```

The boss and challenging colors apply when the game's structured creature
status feed classifies a creature; see [Targets](../widgets/targets.md).

## Highlights

Global toggles for the highlight system:

```toml
[highlights]
sounds_enabled = true           # Play sounds on match
replace_enabled = true          # Apply text replacements
redirect_enabled = true         # Route lines to other windows
coloring_enabled = true         # Apply color highlighting
```

## Map

Where the [map widget](../widgets/map.md) finds its data (GUI):

```toml
[map]
lich_dir = "C:/Lich5"              # Lich install (folder containing data/)
# mapdb_path = "C:/maps/map.json"    # explicit file; overrides everything
mapdb_repo = "Nisugi/mapdb"        # GitHub repo for Settings > Map downloads
mapping_mode = false               # Cartography mode: sketch unmapped rooms
```

All of these are editable in **Settings → Map** in the GUI. Downloaded
map data outranks the Lich folder; an empty `mapdb_repo` disables
downloads. `mapping_mode` gates ghost-room sketches — see
[ghost rooms](../widgets/map.md#ghost-rooms-unmapped-interiors).

## Travel (.go2)

Managed by the client — you normally never edit this section:

```toml
[go2]
native_map_clicks = true   # Map clicks travel natively; false sends ;go2 to Lich

[go2.saved]                # .go2 save <name> writes these
bank = 3517

[go2.pathcodes]            # Maze routes, captured automatically in-game
```

`native_map_clicks` is also in **Settings → Travel** in the GUI. See the
[Travel chapter](../widgets/travel.md) for the `.go2` command family.

## Sound

```toml
[sound]
enabled = true
volume = 0.7                    # 0.0 to 1.0
cooldown_ms = 500               # Min time between repeated sounds
startup_music = true
startup_music_delay_ms = 0      # Delay before the login theme starts
```

## Text-to-Speech

Prefer the editors: **Settings > Speech** in the GUI, or the
[`.tts` commands](../reference/commands.md#text-to-speech) on any frontend.

```toml
[tts]
enabled = false
rate = 1.0                      # 0.5 (slow) to 3.0 (engine max)
volume = 1.0
speak_thoughts = true
speak_speech = true
speak_main = false              # Usually too noisy
# voice = "Microsoft Zira"      # Voice by name (.tts voices lists them)

# Lines matching these regexes are shown but never spoken
# gags = ["^You feel fully rested"]

# Pronunciation fixes applied before speaking
# substitutions = [{ pattern = "Wehnimer's", replacement = "Wenimers" }]
```

The three `speak_*` toggles are classic shortcuts for main/thoughts/speech.
Any other window can opt in per-window: the "speak new lines" checkbox in
its window editor (`tts_speak` in [layout.toml](./layout-toml.md)).

TTS queue navigation keys are bound in [keybinds.toml](./keybinds-toml.md)
(defaults: `Ctrl+Alt+arrows`, `F7`–`F11`).

## Web Server (Mobile Frontend)

Embedded HTTP + WebSocket server that lets a phone browser join the
session. Off by default; see [Mobile Web](../frontends/web.md).

```toml
[web]
enabled = false
port = 8040           # base port; instances walk upward unless pinned
bind = "127.0.0.1"    # set "0.0.0.0" to allow phones on your LAN
pinned = false        # true = bind exactly this port or fail loudly
```

Phones must pair once via a token — run `.webinfo` in-game for the URL
and QR code.

> **Security**: pairing keeps strangers out, but keep the port on a
> trusted LAN; for off-LAN play use Tailscale/WireGuard. Never expose it
> to the open internet.

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

Controls where a stream's text goes when no window subscribes to it.
Prefer the editor: `.streams` (or the GUI's Streams panel) lists every
known stream and lets you set its route.

```toml
[streams]
fallback = "main"               # Streams with no route entry go here
room_in_main = true             # Show room text in main (DR only)

[streams.routes]
# Per-stream policy: "discard", "main", or "window:<name>"
speech = "discard"              # Already echoed in main; drop the duplicate
targetlist = "discard"          # Lich script noise without a widget
logons = "window:arrivals"      # Send to a window (received even while hidden)
```

Windows that subscribe to a stream always win; routes only decide what
happens to *orphaned* streams. A `window:<name>` route never creates or
opens the window — if it doesn't exist, the stream uses `fallback`.

> **Legacy**: older configs used `drop_unsubscribed = [...]`. It still
> loads — each entry is migrated to `routes.<stream> = "discard"` — but
> `routes` is the current mechanism.

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
