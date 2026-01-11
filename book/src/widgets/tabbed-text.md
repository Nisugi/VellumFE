# Tabbed Text Windows

Multiple text streams organized into switchable tabs.

## Basic Usage

```toml
[[windows]]
name = "channels"
widget_type = "tabbedtext"
row = 0
col = 80
rows = 20
cols = 40
buffer_size = 2000

[[windows.tabs]]
name = "Speech"
streams = ["speech"]

[[windows.tabs]]
name = "Thoughts"
streams = ["thoughts"]

[[windows.tabs]]
name = "Combat"
streams = ["combat"]
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `buffer_size` | integer | 5000 | Lines per tab |

### Tab Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `name` | string | required | Tab label |
| `streams` | array | required | Stream IDs for this tab |
| `show_timestamps` | bool | false | Show timestamps |
| `timestamp_position` | string | `"end"` | `"start"` or `"end"` |
| `ignore_activity` | bool | false | Don't highlight on new content |

## Tab Switching

- Click tab name to switch
- Activity indicator shows which tabs have new content

## Example: Communication Hub

```toml
[[windows]]
name = "comms"
widget_type = "tabbedtext"
row = 0
col = 100
rows = 25
cols = 60
buffer_size = 3000

[[windows.tabs]]
name = "Speech"
streams = ["speech"]
show_timestamps = true

[[windows.tabs]]
name = "Thoughts"
streams = ["thoughts"]
show_timestamps = true

[[windows.tabs]]
name = "Whispers"
streams = ["whisper"]
show_timestamps = true

[[windows.tabs]]
name = "Group"
streams = ["group"]
ignore_activity = true
```

## Example: Game Activity

```toml
[[windows]]
name = "activity"
widget_type = "tabbedtext"
buffer_size = 1000

[[windows.tabs]]
name = "Combat"
streams = ["combat"]

[[windows.tabs]]
name = "Deaths"
streams = ["death"]

[[windows.tabs]]
name = "Arrivals"
streams = ["logons"]
ignore_activity = true
```
