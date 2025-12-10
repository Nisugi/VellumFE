# Split Main Window

Divide the main text area into multiple panels for better organization.

## Goal

Create separate text windows for different content types while maintaining a clean, organized layout.

## Basic Split: Combat + Main

Two-column layout with combat on the left, main text on the right.

```toml
# Main game text (right side)
[[widgets]]
type = "text"
name = "main"
title = "Game"
x = 30
y = 0
width = 70
height = 85
streams = ["main", "room"]
scrollback = 5000

# Combat text (left side)
[[widgets]]
type = "text"
name = "combat"
title = "Combat"
x = 0
y = 0
width = 29
height = 85
streams = ["combat"]
scrollback = 2000
```

## Horizontal Split: Main + Chat

Main text on top, communications below.

```toml
# Main game text (top)
[[widgets]]
type = "text"
name = "main"
title = "Game"
x = 0
y = 0
width = 100
height = 60
streams = ["main", "room", "combat"]
scrollback = 5000

# Chat window (bottom)
[[widgets]]
type = "text"
name = "chat"
title = "Chat"
x = 0
y = 61
width = 100
height = 24
streams = ["speech", "whisper", "thoughts"]
scrollback = 3000
```

## Three-Panel Layout

Combat left, main center, chat right.

```toml
# Combat (left)
[[widgets]]
type = "text"
name = "combat"
title = "Combat"
x = 0
y = 0
width = 25
height = 85
streams = ["combat"]

# Main (center)
[[widgets]]
type = "text"
name = "main"
title = "Game"
x = 26
y = 0
width = 48
height = 85
streams = ["main", "room"]

# Chat (right)
[[widgets]]
type = "text"
name = "chat"
title = "Chat"
x = 75
y = 0
width = 25
height = 85
streams = ["speech", "whisper", "thoughts"]
```

## Percentage-Based Split

Adapts to terminal size.

```toml
# Left panel (30% width)
[[widgets]]
type = "text"
name = "side"
x = "0%"
y = "0%"
width = "30%"
height = "80%"
streams = ["combat"]

# Right panel (70% width)
[[widgets]]
type = "text"
name = "main"
x = "30%"
y = "0%"
width = "70%"
height = "80%"
streams = ["main", "room"]
```

## How It Works

### Stream Assignment

Each widget shows only its assigned streams:
- `main` - Primary game output
- `room` - Room descriptions
- `combat` - Combat messages
- `speech` - Character speech
- `whisper` - Private messages
- `thoughts` - Mental communications

### Position Calculation

Widgets are positioned using x, y coordinates:
- Fixed numbers = absolute cells
- Percentages = relative to terminal size

### Gap Prevention

Leave 1 cell between widgets for borders:
```toml
# Widget 1 ends at x=29
width = 29

# Widget 2 starts at x=30
x = 30
```

## Variations

### Minimal Split

Just main and combat:
```toml
[[widgets]]
type = "text"
name = "main"
x = 0
width = "65%"
streams = ["main", "room", "speech", "whisper"]

[[widgets]]
type = "text"
name = "combat"
x = "66%"
width = "34%"
streams = ["combat"]
```

### Stacked Chat

Multiple chat streams in separate panels:
```toml
[[widgets]]
type = "text"
name = "speech"
title = "Speech"
y = 0
height = "33%"
streams = ["speech"]

[[widgets]]
type = "text"
name = "whisper"
title = "Whispers"
y = "34%"
height = "33%"
streams = ["whisper"]

[[widgets]]
type = "text"
name = "thoughts"
title = "Thoughts"
y = "67%"
height = "33%"
streams = ["thoughts"]
```

## Tips

1. **Consider Focus**: Which panel needs most attention?
2. **Size by Importance**: Larger panels for important content
3. **Test Scrollback**: Reduce scrollback for secondary panels
4. **Add Borders**: Visual separation helps readability

## See Also

- [layout.toml Reference](../configuration/layout-toml.md)
- [Text Windows](../widgets/text-windows.md)
- [Stream IDs](../reference/stream-ids.md)

