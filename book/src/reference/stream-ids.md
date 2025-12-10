# Stream IDs

Reference of all game stream identifiers.

## Overview

Streams categorize game output. Each text window can subscribe to specific streams to filter content.

## Core Streams

| Stream ID | Description | Content Type |
|-----------|-------------|--------------|
| `main` | Primary game output | General text, actions, results |
| `room` | Room descriptions | Room name, description, exits, objects |
| `combat` | Combat messages | Attacks, damage, death |
| `speech` | Spoken communication | Say, ask, exclaim |
| `whisper` | Private messages | Whispers |
| `thoughts` | ESP/mental communication | Think, group chat |

## Communication Streams

| Stream ID | Description |
|-----------|-------------|
| `speech` | Player speech (say, ask) |
| `whisper` | Private whispers |
| `thoughts` | ESP channel (think) |
| `shout` | Shouted messages |
| `sing` | Sung messages |

## Game State Streams

| Stream ID | Description |
|-----------|-------------|
| `room` | Room information |
| `inv` | Inventory updates |
| `percWindow` | Perception window |
| `familiar` | Familiar view |
| `bounty` | Bounty task info |

## Combat Streams

| Stream ID | Description |
|-----------|-------------|
| `combat` | Combat actions and results |
| `assess` | Creature assessments |
| `death` | Death notifications |

## Specialty Streams

| Stream ID | Description |
|-----------|-------------|
| `logons` | Player login/logout |
| `atmospherics` | Weather and atmosphere |
| `loot` | Loot messages |
| `group` | Group/party messages |

## System Streams

| Stream ID | Description |
|-----------|-------------|
| `raw` | Unprocessed output |
| `debug` | Debug information |
| `script` | Script output (Lich) |

## Using Streams

### Widget Configuration

```toml
# Main window - everything
[[widgets]]
type = "text"
name = "main"
streams = ["main", "room", "combat"]

# Chat window - communication only
[[widgets]]
type = "text"
name = "chat"
streams = ["speech", "whisper", "thoughts"]

# Combat window - combat only
[[widgets]]
type = "text"
name = "combat"
streams = ["combat"]
```

### Empty Streams (All)

```toml
# Empty array = receive all streams
[[widgets]]
type = "text"
streams = []
```

### Tabbed Text

```toml
[[widgets]]
type = "tabbedtext"
tabs = [
    { name = "All", streams = [] },
    { name = "Game", streams = ["main", "room"] },
    { name = "Combat", streams = ["combat"] },
    { name = "Chat", streams = ["speech", "thoughts", "whisper"] },
]
```

### Triggers

```toml
# Trigger on specific stream
[[triggers]]
pattern = "whispers,"
command = ".notify Whisper!"
stream = "whisper"

# Trigger on combat stream
[[triggers]]
pattern = "falls dead"
command = "search"
stream = "combat"
```

## Stream Colors

Configure stream-specific colors in `colors.toml`:

```toml
[theme]
# Stream colors
main = "#ffffff"
room = "#ffff00"
combat = "#ff4444"
speech = "#00ffff"
whisper = "#ff00ff"
thoughts = "#00ff00"
```

## Stream Priority

When streams overlap, content appears in all matching windows. Configure priority in layout:

```toml
[[widgets]]
type = "text"
name = "combat"
streams = ["combat"]
priority = 100        # Higher = checked first
```

## Common Configurations

### Single Window (All Content)

```toml
[[widgets]]
type = "text"
name = "main"
streams = []          # All streams
```

### Two Windows (Game + Chat)

```toml
[[widgets]]
type = "text"
name = "game"
streams = ["main", "room", "combat"]

[[widgets]]
type = "text"
name = "chat"
streams = ["speech", "thoughts", "whisper"]
```

### Three Windows (Specialized)

```toml
[[widgets]]
type = "text"
name = "story"
streams = ["main", "room"]

[[widgets]]
type = "text"
name = "combat"
streams = ["combat"]

[[widgets]]
type = "text"
name = "social"
streams = ["speech", "thoughts", "whisper"]
```

### RP Layout

```toml
[[widgets]]
type = "text"
name = "story"
streams = ["main", "room"]        # Immersive content

[[widgets]]
type = "text"
name = "ic"
streams = ["speech", "whisper"]   # In-character

[[widgets]]
type = "text"
name = "ooc"
streams = ["thoughts"]            # Out-of-character
```

## Stream Detection

The parser assigns streams based on XML tags:

| XML Source | Stream |
|------------|--------|
| `<pushStream id="X"/>` | Stream X |
| `<roomName>` | room |
| `<roomDesc>` | room |
| Combat XML | combat |
| Default text | main |

## See Also

- [Text Windows](../widgets/text-windows.md)
- [Tabbed Text](../widgets/tabbed-text.md)
- [Parser Protocol](../architecture/parser-protocol.md)

