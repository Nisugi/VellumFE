# Dashboard

Configurable grid of status indicators in a single widget.

## Basic Usage

```toml
[[windows]]
name = "dashboard"
widget_type = "dashboard"
row = 0
col = 0
rows = 2
cols = 12
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `indicators` | array | all | Which indicators to show |
| `columns` | integer | auto | Grid columns |

## Display

Shows multiple status indicators in a compact grid:

```
┌──────────┐
│KNE SIT HID│
│STU WEB BLE│
└──────────┘
```

- Active indicators are highlighted
- Inactive indicators are dimmed

## Custom Indicators

```toml
[[windows]]
name = "dashboard"
widget_type = "dashboard"
indicators = ["kneeling", "hidden", "stunned", "webbed"]
columns = 2
```

## Available Indicators

- `kneeling`, `sitting`, `prone`
- `stunned`, `webbed`, `hidden`
- `invisible`, `dead`
- `bleeding`, `poisoned`, `diseased`

## Example: Combat Dashboard

```toml
[[windows]]
name = "combat_status"
widget_type = "dashboard"
row = 0
col = 0
rows = 2
cols = 15
indicators = ["stunned", "webbed", "prone", "bleeding"]
columns = 2
show_border = true
title = "Status"
```
