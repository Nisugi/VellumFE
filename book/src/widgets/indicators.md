# Status Indicators

Display character status conditions (kneeling, hidden, webbed, etc).

## Basic Usage

```toml
[[windows]]
name = "status"
widget_type = "indicator"
id = "kneeling"
row = 0
col = 0
rows = 1
cols = 3
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `id` | string | required | Status to track |
| `active_color` | string | auto | Color when active |
| `inactive_color` | string | `"gray"` | Color when inactive |

## Available Statuses

| ID | Description |
|----|-------------|
| `kneeling` | Kneeling position |
| `sitting` | Sitting position |
| `prone` | Lying down |
| `stunned` | Stunned |
| `webbed` | Webbed |
| `hidden` | Hidden |
| `invisible` | Invisible |
| `dead` | Dead |
| `bleeding` | Bleeding |
| `poisoned` | Poisoned |
| `diseased` | Diseased |

## Examples

### Single Indicator
```toml
[[windows]]
name = "hidden_indicator"
widget_type = "indicator"
id = "hidden"
rows = 1
cols = 3
active_color = "#00FF00"
```

### Status Row

```toml
[[windows]]
name = "kneel"
widget_type = "indicator"
id = "kneeling"
row = 0
col = 0
rows = 1
cols = 3

[[windows]]
name = "hide"
widget_type = "indicator"
id = "hidden"
row = 0
col = 3
rows = 1
cols = 3

[[windows]]
name = "stun"
widget_type = "indicator"
id = "stunned"
row = 0
col = 6
rows = 1
cols = 3
```

## Display

- Shows abbreviated status code (3 chars)
- Active: colored, Inactive: dimmed
- Example: `KNE` (kneeling), `HID` (hidden)
