# config.toml Reference

The main configuration file controlling VellumFE's behavior, connection settings, and general preferences.

## Location

`~/.vellum-fe/config.toml`

---

## Complete Reference

```toml
#
# VellumFE Configuration
#

[connection]
# Default host for Lich proxy mode
host = "127.0.0.1"

# Default port for Lich proxy mode
port = 8000

# Default character name (used for profile loading)
character = ""

# Timeout for connection attempts (seconds)
timeout = 30

[interface]
# Enable clickable links in game text
links = true

# Show window borders by default
show_borders = true

# Default border style: "plain", "rounded", "double", "thick"
border_style = "rounded"

# Mouse support
mouse = true

# Scroll speed (lines per scroll event)
scroll_speed = 3

# Command history size
history_size = 1000

[sound]
# Enable sound effects
enabled = true

# Master volume (0.0 - 1.0)
volume = 0.8

# Play startup music
startup_music = true

# Sound file paths (relative to data dir or absolute)
# alert_sound = "sounds/alert.wav"

[tts]
# Enable text-to-speech
enabled = false

# TTS rate (words per minute, platform dependent)
rate = 150

# Which streams to speak
streams = ["main"]

# Speak room descriptions
speak_rooms = true

# Speak player speech
speak_speech = true

[logging]
# Log level: "error", "warn", "info", "debug", "trace"
level = "warn"

# Log file path (relative to data dir)
file = "vellum-fe.log"

# Log to console (for debugging)
console = false

[behavior]
# Auto-scroll to bottom on new text
auto_scroll = true

# Flash window on important events
flash_on_alert = false

# Notification sound on important events
sound_on_alert = false

# Pause input during roundtime (experimental)
pause_on_rt = false

[performance]
# Maximum lines to buffer per text window
max_buffer_lines = 2000

# Frame rate target
target_fps = 60

# Enable performance metrics collection
collect_metrics = true
```

---

## Section Details

### [connection]

Controls how VellumFE connects to the game.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `host` | string | `"127.0.0.1"` | Host for Lich proxy mode |
| `port` | integer | `8000` | Port for Lich proxy mode |
| `character` | string | `""` | Default character name for profiles |
| `timeout` | integer | `30` | Connection timeout in seconds |

**Note**: Direct mode (`--direct`) ignores these settings and uses command-line credentials.

### [interface]

Visual and interaction settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `links` | boolean | `true` | Enable clickable game links |
| `show_borders` | boolean | `true` | Show window borders by default |
| `border_style` | string | `"rounded"` | Default border style |
| `mouse` | boolean | `true` | Enable mouse support |
| `scroll_speed` | integer | `3` | Lines scrolled per mouse wheel tick |
| `history_size` | integer | `1000` | Command history entries to remember |

**Border styles:**
- `"plain"` - Single line: `─│─│┌┐└┘`
- `"rounded"` - Rounded corners: `─│─│╭╮╰╯`
- `"double"` - Double line: `═║═║╔╗╚╝`
- `"thick"` - Thick line: `━┃━┃┏┓┗┛`

### [sound]

Audio settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | boolean | `true` | Master sound toggle |
| `volume` | float | `0.8` | Master volume (0.0 to 1.0) |
| `startup_music` | boolean | `true` | Play music on launch |

**Note**: Requires the `sound` feature to be compiled in.

### [tts]

Text-to-speech accessibility features.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | boolean | `false` | Enable TTS |
| `rate` | integer | `150` | Speech rate (WPM) |
| `streams` | array | `["main"]` | Streams to speak |
| `speak_rooms` | boolean | `true` | Speak room descriptions |
| `speak_speech` | boolean | `true` | Speak player dialogue |

**Supported streams:** `"main"`, `"speech"`, `"thoughts"`, `"combat"`

### [logging]

Diagnostic logging configuration.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `level` | string | `"warn"` | Minimum log level |
| `file` | string | `"vellum-fe.log"` | Log file path |
| `console` | boolean | `false` | Also log to console |

**Log levels** (from least to most verbose):
- `"error"` - Only errors
- `"warn"` - Errors and warnings
- `"info"` - Normal operation info
- `"debug"` - Debugging details
- `"trace"` - Very verbose tracing

### [behavior]

Client behavior tweaks.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `auto_scroll` | boolean | `true` | Auto-scroll on new text |
| `flash_on_alert` | boolean | `false` | Flash window on alerts |
| `sound_on_alert` | boolean | `false` | Play sound on alerts |
| `pause_on_rt` | boolean | `false` | Pause input during RT |

### [performance]

Performance tuning.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `max_buffer_lines` | integer | `2000` | Max lines per text window |
| `target_fps` | integer | `60` | Target frame rate |
| `collect_metrics` | boolean | `true` | Enable performance stats |

---

## Examples

### Minimal Config

```toml
[connection]
port = 8000
character = "MyCharacter"
```

### High-Performance Config

```toml
[performance]
max_buffer_lines = 1000
target_fps = 120
collect_metrics = false

[interface]
links = false  # Disable link parsing for speed
```

### Accessibility Config

```toml
[tts]
enabled = true
rate = 175
streams = ["main", "speech"]
speak_rooms = true
speak_speech = true

[interface]
scroll_speed = 5
```

---

## Environment Variable Overrides

Some settings can be overridden via environment variables:

| Variable | Overrides |
|----------|-----------|
| `VELLUM_FE_DIR` | Data directory location |
| `RUST_LOG` | Logging level |
| `COLORTERM` | Terminal color support |

---

## See Also

- [layout.toml](./layout-toml.md) - Window layout configuration
- [Character Profiles](./profiles.md) - Per-character settings
- [Environment Variables](../reference/environment-vars.md) - All environment variables
