# Tabbed Channels

Organize different communication streams into tabs for cleaner chat management.

## Goal

Separate game text, combat, and various chat channels into organized tabs you can quickly switch between.

## Basic Tabbed Chat

```toml
[[widgets]]
type = "tabbedtext"
name = "channels"
x = 0
y = 60
width = 100
height = 25

[[widgets.channels.tabs]]
name = "All"
streams = ["speech", "whisper", "thoughts", "group"]
scrollback = 2000

[[widgets.channels.tabs]]
name = "Speech"
streams = ["speech"]
scrollback = 1000

[[widgets.channels.tabs]]
name = "Whispers"
streams = ["whisper"]
scrollback = 500
highlight_new = true

[[widgets.channels.tabs]]
name = "Thoughts"
streams = ["thoughts"]
scrollback = 500
```

## Combat + Chat Tabs

Separate combat from communication:

```toml
[[widgets]]
type = "tabbedtext"
name = "main_tabs"
x = 0
y = 0
width = 75
height = 85

[[widgets.main_tabs.tabs]]
name = "Game"
streams = ["main", "room"]
scrollback = 5000
default = true

[[widgets.main_tabs.tabs]]
name = "Combat"
streams = ["combat"]
scrollback = 2000
notify_on_activity = true

[[widgets.main_tabs.tabs]]
name = "Chat"
streams = ["speech", "whisper", "thoughts"]
scrollback = 1000
notify_on_activity = true

[[widgets.main_tabs.tabs]]
name = "System"
streams = ["logons", "deaths", "experience"]
scrollback = 500
```

## Notification Indicators

Show unread indicators on tabs:

```toml
[[widgets]]
type = "tabbedtext"
name = "chat_tabs"

[widgets.chat_tabs.settings]
show_unread_count = true
unread_indicator = "●"
unread_color = "yellow"
flash_on_highlight = true

[[widgets.chat_tabs.tabs]]
name = "Whisper"
streams = ["whisper"]
# Tab shows: "Whisper ●3" when 3 unread messages
notify_pattern = ".*"  # All messages count
```

## Keybind Navigation

Quick tab switching:

```toml
# keybinds.toml

[keybinds."alt+1"]
action = "switch_tab"
widget = "channels"
tab = 0

[keybinds."alt+2"]
action = "switch_tab"
widget = "channels"
tab = 1

[keybinds."alt+3"]
action = "switch_tab"
widget = "channels"
tab = 2

[keybinds."alt+4"]
action = "switch_tab"
widget = "channels"
tab = 3

# Cycle tabs
[keybinds."ctrl+tab"]
action = "next_tab"
widget = "channels"

[keybinds."ctrl+shift+tab"]
action = "prev_tab"
widget = "channels"
```

## Filtered Tabs

Create tabs with filtered content:

```toml
[[widgets.channels.tabs]]
name = "Trade"
streams = ["speech"]
filter_pattern = "(?i)(sell|buy|trade|silver|coin)"
scrollback = 500

[[widgets.channels.tabs]]
name = "Group"
streams = ["speech", "thoughts"]
filter_pattern = "\\[Group\\]|\\[Party\\]"
scrollback = 500
```

## Tab Styles

### Minimal Tabs

```toml
[widgets.channels.style]
tab_position = "top"
tab_style = "minimal"
# Shows: [Game|Combat|Chat]
separator = "|"
active_indicator = "underline"
```

### Full Tabs

```toml
[widgets.channels.style]
tab_position = "top"
tab_style = "full"
# Shows: ╔════════╗╔═══════╗
#        ║  Game  ║║Combat ║
active_bg = "blue"
inactive_bg = "gray"
```

### Bottom Tabs

```toml
[widgets.channels.style]
tab_position = "bottom"
```

## Per-Tab Settings

Different settings per tab:

```toml
[[widgets.channels.tabs]]
name = "Game"
streams = ["main", "room"]
scrollback = 5000
wrap = true
timestamps = false

[[widgets.channels.tabs]]
name = "Combat"
streams = ["combat"]
scrollback = 1000
wrap = false
timestamps = true
timestamp_format = "[%H:%M:%S]"

[[widgets.channels.tabs]]
name = "Log"
streams = ["experience", "logons", "deaths"]
scrollback = 500
timestamps = true
log_to_file = true
log_file = "~/.vellum-fe/logs/events.log"
```

## Searchable Tabs

Enable search within tabs:

```toml
[widgets.channels.settings]
searchable = true
search_keybind = "ctrl+f"
search_highlight = "yellow"
```

## Auto-Focus Tab

Switch to tab on activity:

```toml
[[widgets.channels.tabs]]
name = "Whispers"
streams = ["whisper"]
auto_focus_on_activity = true
# Automatically switches to this tab when whisper received
```

Or only for specific patterns:

```toml
[[widgets.channels.tabs]]
name = "Important"
streams = ["whisper", "thoughts"]
auto_focus_pattern = "(?i)(urgent|emergency|help)"
```

## Complete Setup

### layout.toml

```toml
# Main game area with tabs
[[widgets]]
type = "tabbedtext"
name = "main_tabs"
x = 0
y = 0
width = 75
height = 70

[widgets.main_tabs.settings]
tab_position = "top"
show_unread_count = true
default_tab = "Game"

[[widgets.main_tabs.tabs]]
name = "Game"
streams = ["main", "room"]
scrollback = 5000

[[widgets.main_tabs.tabs]]
name = "Combat"
streams = ["combat"]
scrollback = 2000

# Chat area with tabs
[[widgets]]
type = "tabbedtext"
name = "chat_tabs"
x = 0
y = 71
width = 75
height = 14

[widgets.chat_tabs.settings]
tab_position = "top"
show_unread_count = true

[[widgets.chat_tabs.tabs]]
name = "All"
streams = ["speech", "whisper", "thoughts"]
scrollback = 1000

[[widgets.chat_tabs.tabs]]
name = "Local"
streams = ["speech"]

[[widgets.chat_tabs.tabs]]
name = "Private"
streams = ["whisper"]
notify_on_activity = true

[[widgets.chat_tabs.tabs]]
name = "ESP"
streams = ["thoughts"]
```

### keybinds.toml

```toml
# Tab navigation
[keybinds."f1"]
action = "focus_widget"
widget = "main_tabs"

[keybinds."f2"]
action = "focus_widget"
widget = "chat_tabs"

[keybinds."ctrl+1"]
action = "switch_tab"
widget = "main_tabs"
tab = 0

[keybinds."ctrl+2"]
action = "switch_tab"
widget = "main_tabs"
tab = 1
```

## Tips

1. **Don't Over-Tab**: Too many tabs defeats organization
2. **Use Notifications**: Visual cues for important tabs
3. **Set Keybinds**: Quick switching is essential
4. **Filter Wisely**: Regex filters can miss messages

## See Also

- [Tabbed Text Widget](../widgets/tabbed-text.md)
- [Stream IDs](../reference/stream-ids.md)
- [Keybind Actions](../reference/keybind-actions.md)

