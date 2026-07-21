# Status Indicators

Display character status conditions (poisoned, stunned, webbed, etc).

## Basic Usage

```toml
[[windows]]
name = "poisoned"
widget_type = "indicator"
indicator_id = "POISONED"
row = 0
col = 0
rows = 1
cols = 3
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `indicator_id` | string | required | Status to track (see below) |
| `icon` | string | none | Glyph/text shown when active |
| `active_color` | string | `#00ff00` | Color when active |
| `inactive_color` | string | `#555555` | Color when inactive |

## Available Statuses

Built-in templates exist for `poisoned`, `bleeding`, `diseased`, `stunned`,
and `webbed` — add them via `.addwindow` or the Add Window menu.

The full set of recognized indicator ids:

| ID | Description |
|----|-------------|
| `STANDING` | Standing position |
| `KNEELING` | Kneeling position |
| `SITTING` | Sitting position |
| `PRONE` | Lying down |
| `DEAD` | Dead |
| `STUNNED` | Stunned |
| `BLEEDING` | Bleeding |
| `HIDDEN` | Hidden |
| `INVISIBLE` | Invisible |
| `WEBBED` | Webbed |
| `POISONED` | Poisoned |
| `DISEASED` | Diseased |
| `JOINED` | Grouped/joined |

Ids without a built-in template can be added as custom indicator windows.

## Examples

### Single Indicator
```toml
[[windows]]
name = "hidden_indicator"
widget_type = "indicator"
indicator_id = "HIDDEN"
rows = 1
cols = 3
active_color = "#00FF00"
```

### Status Row

```toml
[[windows]]
name = "stun"
widget_type = "indicator"
indicator_id = "STUNNED"
row = 0
col = 0
rows = 1
cols = 3

[[windows]]
name = "web"
widget_type = "indicator"
indicator_id = "WEBBED"
row = 0
col = 3
rows = 1
cols = 3

[[windows]]
name = "bleed"
widget_type = "indicator"
indicator_id = "BLEEDING"
row = 0
col = 6
rows = 1
cols = 3
```

## Display

- **Active**: shows the widget's icon/title (the built-in templates use
  glyph icons), centered, in `active_color`
- **Inactive**: renders nothing — the indicator is invisible until the
  status is active
