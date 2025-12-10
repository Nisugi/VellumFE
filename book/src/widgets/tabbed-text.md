# Tabbed Text Windows

Tabbed text windows combine multiple game streams into a single widget with selectable tabs.

## Overview

Tabbed text windows:
- Display multiple streams in one widget
- Switch between tabs to view different content
- Share a single buffer per tab
- Save screen space vs. multiple windows

## Configuration

```toml
[[windows]]
name = "channels"
type = "tabbedtext"

# Position and size
row = 0
col = 80
width = 40
height = 20

# Tab configuration
tabs = ["speech", "thoughts", "combat"]
default_tab = "speech"
show_tab_bar = true

# Visual options
tab_bar_position = "top"    # "top" or "bottom"
buffer_size = 1000          # Per-tab buffer
```

## Properties

### tabs (required)

List of streams to display as tabs:

```toml
tabs = ["speech", "thoughts", "combat"]
```

Each tab shows content from the named stream.

### default_tab

Initially selected tab:

```toml
default_tab = "speech"    # Open to speech tab
```

If not specified, opens to first tab.

### show_tab_bar

Whether to display the tab bar:

```toml
show_tab_bar = true     # Show tabs (default)
show_tab_bar = false    # Hide tab bar
```

### tab_bar_position

Position of the tab bar:

```toml
tab_bar_position = "top"      # Tabs at top (default)
tab_bar_position = "bottom"   # Tabs at bottom
```

### buffer_size

Lines to buffer per tab:

```toml
buffer_size = 1000    # Default per tab
```

## Tab Bar Display

```
┌─ Channels ──────────────────────────┐
│ [Speech] [Thoughts] [Combat]        │
├─────────────────────────────────────┤
│ Alice says, "Hello everyone!"       │
│ Bob asks, "How are you?"            │
│ Charlie exclaims, "Great day!"      │
│                                     │
└─────────────────────────────────────┘
```

Active tab is highlighted, inactive tabs are dimmed.

## Interaction

### Tab Navigation

| Input | Action |
|-------|--------|
| `Tab` | Next tab |
| `Shift+Tab` | Previous tab |
| Click tab | Select tab |
| `1-9` | Select tab by number |

### Content Navigation

| Input | Action |
|-------|--------|
| `Page Up/Down` | Scroll current tab |
| `Home/End` | Top/bottom of current tab |
| Mouse wheel | Scroll current tab |

### Text Operations

| Input | Action |
|-------|--------|
| Click+drag | Select text |
| `Ctrl+C` | Copy selection |

## Examples

### Communication Channels

```toml
[[windows]]
name = "channels"
type = "tabbedtext"
tabs = ["speech", "thoughts", "whisper"]
default_tab = "speech"
row = 0
col = 80
width = 40
height = 20
title = "Channels"
```

### Combat and Notifications

```toml
[[windows]]
name = "alerts"
type = "tabbedtext"
tabs = ["combat", "death", "logons"]
default_tab = "combat"
row = 20
col = 80
width = 40
height = 15
title = "Alerts"
```

### All Streams

```toml
[[windows]]
name = "all_streams"
type = "tabbedtext"
tabs = ["speech", "thoughts", "combat", "death", "logons", "familiar", "group"]
default_tab = "speech"
row = 0
col = 80
width = 40
height = 35
buffer_size = 500    # Smaller buffer for many tabs
```

### Hidden Tab Bar

```toml
[[windows]]
name = "hidden_tabs"
type = "tabbedtext"
tabs = ["speech", "thoughts"]
show_tab_bar = false
row = 0
col = 80
width = 40
height = 20
```

Use keybinds to switch tabs when bar is hidden.

## Tab Indicators

### Unread Content

Tabs with new unread content show an indicator:

```
[Speech*] [Thoughts] [Combat]
```

The `*` indicates unread content in that tab.

### Activity Highlighting

Active tabs can use different colors:

```toml
active_tab_color = "#FFFFFF"
inactive_tab_color = "#808080"
unread_tab_color = "#FFFF00"
```

## Memory Considerations

Each tab maintains its own buffer:

```
Total memory ≈ buffer_size × number_of_tabs × line_size
```

For many tabs, reduce buffer_size:

```toml
# 7 tabs × 500 lines = 3500 lines total
tabs = ["speech", "thoughts", "combat", "death", "logons", "familiar", "group"]
buffer_size = 500
```

## Stream Reference

Common streams for tabs:

| Stream | Content |
|--------|---------|
| `speech` | Player dialogue |
| `thoughts` | ESP/telepathy |
| `combat` | Combat messages |
| `death` | Death notifications |
| `logons` | Arrivals/departures |
| `familiar` | Familiar messages |
| `group` | Group information |
| `whisper` | Private whispers |

## Troubleshooting

### Tab not receiving content

1. Verify stream name is correct
2. Check game is sending that stream
3. Some streams require game features

### Tabs not switching

1. Ensure widget has focus
2. Check keybinds for conflicts
3. Try clicking tabs directly

### Memory issues with many tabs

1. Reduce `buffer_size`
2. Use fewer tabs
3. Remove unused streams

## See Also

- [Text Windows](./text-windows.md) - Single stream windows
- [Stream Reference](../reference/stream-ids.md) - All streams
- [Keybinds](../configuration/keybinds-toml.md) - Tab navigation keys
