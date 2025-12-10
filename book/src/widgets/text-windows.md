# Text Windows

Text windows display scrollable game text. They're the primary widgets for viewing game output.

## Overview

Text windows:
- Display one game stream (main, speech, thoughts, etc.)
- Support scrolling through history
- Apply highlights and colors
- Handle clickable links
- Buffer a configurable number of lines

## Configuration

```toml
[[windows]]
name = "main"
type = "text"

# Position and size
row = 0
col = 0
width = 80
height = 30

# Text-specific options
stream = "main"           # Game stream to display
buffer_size = 2000        # Maximum lines to keep
word_wrap = true          # Enable word wrapping

# Visual options
show_border = true
border_style = "rounded"
show_title = true
title = "Game"            # Custom title
background_color = "#000000"
text_color = "#CCCCCC"
```

## Properties

### stream

The game stream to display:

| Stream | Content |
|--------|---------|
| `main` | Primary game output |
| `speech` | Player dialogue |
| `thoughts` | ESP/telepathy |
| `combat` | Combat messages |
| `death` | Death messages |
| `logons` | Arrivals/departures |
| `familiar` | Familiar messages |
| `group` | Group info |

If not specified, defaults to the widget name.

### buffer_size

Maximum lines to keep in memory:

```toml
buffer_size = 2000    # Default
buffer_size = 500     # Smaller for secondary windows
buffer_size = 5000    # Large for main window
```

Older lines are removed when the buffer fills. Larger buffers use more memory.

### word_wrap

Whether to wrap long lines:

```toml
word_wrap = true      # Wrap at window width (default)
word_wrap = false     # Allow horizontal scrolling
```

## Interaction

### Scrolling

| Input | Action |
|-------|--------|
| `Page Up` | Scroll up one page |
| `Page Down` | Scroll down one page |
| `Home` | Jump to oldest line |
| `End` | Jump to newest line |
| Mouse wheel | Scroll up/down |
| `Ctrl+Up` | Scroll up one line |
| `Ctrl+Down` | Scroll down one line |

### Selection

| Input | Action |
|-------|--------|
| Click + drag | Select text |
| Double-click | Select word |
| Triple-click | Select line |
| `Ctrl+C` | Copy selection |
| `Ctrl+A` | Select all |

### Links

When `links = true` in config.toml:

| Input | Action |
|-------|--------|
| Click link | Primary action (look/get) |
| Right-click | Context menu |
| Ctrl+click | Alternative action |

## Text Styling

### Game Colors

The server sends color information through the XML protocol:
- `<color fg='...' bg='...'>` - Explicit colors
- `<preset id='...'>` - Named presets (speech, monsterbold)
- `<pushBold/><popBold/>` - Bold/monsterbold

### Highlights

Your `highlights.toml` patterns are applied after game colors:

```toml
[[highlights]]
name = "creatures"
pattern = "goblin|orc"
fg = "#FF6600"
bold = true
```

### Priority

Color priority (highest to lowest):
1. Explicit `<color>` tags
2. Monsterbold (`<pushBold/>`)
3. User highlights
4. Preset colors
5. Default text color

## Auto-Scroll Behavior

Text windows auto-scroll to show new content when:
- Window is scrolled to the bottom
- New text arrives

Auto-scroll pauses when:
- User scrolls up
- Window is not at bottom

To resume auto-scroll:
- Press `End` to jump to bottom
- Scroll to the bottom manually

Configure in config.toml:
```toml
[behavior]
auto_scroll = true    # Default
```

## Memory Management

Text windows use generation-based change detection for efficient updates:

1. Each `add_line()` increments a generation counter
2. Sync only copies lines newer than last sync
3. Buffer trimming removes oldest lines when full

This enables smooth performance even with high text throughput.

## Examples

### Main Game Window

```toml
[[windows]]
name = "main"
type = "text"
stream = "main"
row = 0
col = 0
width = "70%"
height = "90%"
buffer_size = 3000
show_title = false
border_style = "rounded"
```

### Speech Window

```toml
[[windows]]
name = "speech"
type = "text"
stream = "speech"
row = 0
col = 85
width = 35
height = 15
buffer_size = 500
title = "Speech"
```

### Thoughts Window

```toml
[[windows]]
name = "thoughts"
type = "text"
stream = "thoughts"
row = 15
col = 85
width = 35
height = 15
buffer_size = 500
title = "Thoughts"
```

### Minimal (No Border)

```toml
[[windows]]
name = "main"
type = "text"
row = 0
col = 0
width = "100%"
height = "100%"
show_border = false
show_title = false
transparent_background = true
```

## Troubleshooting

### Text not appearing

1. Check `stream` matches intended source
2. Verify window is within layout bounds
3. Check `enabled = true` (default)

### Colors wrong

1. Verify `COLORTERM=truecolor` is set
2. Check highlights aren't overriding game colors
3. Review preset colors in `colors.toml`

### Performance issues

1. Reduce `buffer_size` on secondary windows
2. Reduce number of highlight patterns
3. Use `fast_parse = true` for literal patterns

## See Also

- [Tabbed Text Windows](./tabbed-text.md) - Multiple streams in tabs
- [Highlight Patterns](../customization/highlight-patterns.md) - Text styling
- [Stream Reference](../reference/stream-ids.md) - All streams
