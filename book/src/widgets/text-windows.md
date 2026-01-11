# Text Windows

Scrollable text display for game output.

## Basic Usage

```toml
[[windows]]
name = "main"
widget_type = "text"
streams = ["main"]
row = 0
col = 0
rows = 30
cols = 80
buffer_size = 10000
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `streams` | array | `[]` | Stream IDs to display |
| `buffer_size` | integer | 1000 | Lines to keep in memory |
| `compact` | bool | false | Remove blank lines |
| `show_timestamps` | bool | false | Prefix lines with time |
| `timestamp_position` | string | `"end"` | `"start"` or `"end"` |

## Common Streams

| Stream | Content |
|--------|---------|
| `main` | Primary game output |
| `speech` | Player dialogue |
| `thoughts` | ESP/telepathy |
| `combat` | Combat messages |
| `death` | Death messages |
| `familiar` | Familiar messages |
| `group` | Group information |
| `logons` | Login/logout |
| `society` | Society messages |
| `bounty` | Bounty information |

## Examples

### Main Window
```toml
[[windows]]
name = "main"
widget_type = "text"
streams = ["main"]
buffer_size = 10000
```

### Speech Window
```toml
[[windows]]
name = "speech"
widget_type = "text"
streams = ["speech"]
buffer_size = 2000
show_timestamps = true
timestamp_position = "start"
```

### Combat Log (Compact)
```toml
[[windows]]
name = "combat"
widget_type = "text"
streams = ["combat"]
buffer_size = 500
compact = true
```

## Scrolling

- `Page Up` / `Page Down` - Scroll when focused
- Mouse wheel - Scroll under cursor
- `Home` / `End` - Jump to top/bottom
- Auto-scrolls when new text arrives (unless scrolled back)
