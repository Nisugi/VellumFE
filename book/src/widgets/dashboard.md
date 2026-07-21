# Dashboard

Configurable group of status indicators in a single widget.

## Basic Usage

```toml
[[windows]]
name = "dashboard"
widget_type = "dashboard"
row = 0
col = 0
rows = 2
cols = 12
dashboard_layout = "horizontal"

[[windows.dashboard_indicators]]
id = "POISONED"
icon = "☠"
colors = ["#555555", "#00ff00"]   # [inactive, active]

[[windows.dashboard_indicators]]
id = "BLEEDING"
icon = "♥"
colors = ["#555555", "#ff0000"]
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `dashboard_layout` | string | `"horizontal"` | `"horizontal"`, `"vertical"`, or `"grid:RxC"` (e.g. `"grid:2x3"`) |
| `dashboard_spacing` | integer | — | Spacing between indicators (characters) |
| `dashboard_hide_inactive` | bool | — | Hide indicators that are inactive |
| `dashboard_indicators` | array of tables | empty | The indicators to show (see below) |

Each `[[windows.dashboard_indicators]]` entry has:

| Field | Description |
|-------|-------------|
| `id` | Indicator id (e.g. `POISONED`, `BLEEDING`, `STUNNED`, `WEBBED`, `HIDDEN` — see [Status Indicators](./indicators.md)) |
| `icon` | Glyph to display |
| `colors` | Colors by state: `[inactive, active]` |

There is no default indicator set — an empty dashboard shows nothing until
you add indicator entries (the shipped `sidebar` layout includes a
configured example, and the GUI window editor can edit them).

## Display

Shows the configured indicator icons in the chosen layout:

```
┌────────────┐
│ ☠  ♥  ⚠   │
└────────────┘
```

- Active indicators render in their active color
- Inactive indicators render in their inactive color (or are hidden
  entirely with `dashboard_hide_inactive = true`)

## Example: Combat Dashboard

```toml
[[windows]]
name = "combat_status"
widget_type = "dashboard"
row = 0
col = 0
rows = 1
cols = 15
dashboard_layout = "horizontal"
dashboard_spacing = 1
show_border = true
title = "Status"

[[windows.dashboard_indicators]]
id = "STUNNED"
icon = "⚠"
colors = ["#555555", "#ffff00"]

[[windows.dashboard_indicators]]
id = "WEBBED"
icon = "🕸"
colors = ["#555555", "#ffffff"]

[[windows.dashboard_indicators]]
id = "BLEEDING"
icon = "♥"
colors = ["#555555", "#ff0000"]
```
