# Progress Bars

Display character vitals as visual bars.

## Basic Usage

```toml
[[windows]]
name = "health"
widget_type = "progress"
stat = "health"
row = 0
col = 0
rows = 1
cols = 20
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `stat` | string | required | Stat to display |
| `bar_color` | string | auto | Bar fill color |
| `show_percentage` | bool | true | Show % value |
| `show_label` | bool | true | Show stat name |

## Available Stats

| Stat | Description |
|------|-------------|
| `health` | Hit points |
| `mana` | Mana points |
| `stamina` | Stamina points |
| `spirit` | Spirit points |
| `encumbrance` | Carry weight |
| `mind` | Mental state (DR) |
| `concentration` | Concentration (DR) |

## Examples

### Minimal Health Bar
```toml
[[windows]]
name = "health"
widget_type = "progress"
stat = "health"
rows = 1
cols = 15
show_label = false
```

### Colored Mana Bar
```toml
[[windows]]
name = "mana"
widget_type = "progress"
stat = "mana"
bar_color = "#4169E1"
rows = 1
cols = 20
```

### Stacked Vitals

```toml
[[windows]]
name = "health"
widget_type = "progress"
stat = "health"
row = 0
col = 0
rows = 1
cols = 25
bar_color = "#FF4040"

[[windows]]
name = "mana"
widget_type = "progress"
stat = "mana"
row = 1
col = 0
rows = 1
cols = 25
bar_color = "#4169E1"

[[windows]]
name = "stamina"
widget_type = "progress"
stat = "stamina"
row = 2
col = 0
rows = 1
cols = 25
bar_color = "#32CD32"
```

## Color Behavior

Without `bar_color`, bars change color based on value:
- Green (high) → Yellow (medium) → Red (low)
