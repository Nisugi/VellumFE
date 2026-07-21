# Progress Bars

Display character vitals as visual bars.

## Basic Usage

```toml
[[windows]]
name = "health"
widget_type = "progress"
id = "health"
row = 0
col = 0
rows = 1
cols = 20
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `id` | string | required | Progress feed to display (the game's `progressBar` id, case-sensitive) |
| `label` | string | auto | Label text override |
| `color` | string | auto | Bar fill color |
| `numbers_only` | bool | false | Show only `current/max` numbers (no label) |
| `current_only` | bool | false | Show only the current value (no label, no max) |

## Common Feed Ids

Built-in templates exist for these ids (via `.addwindow` or the Add Window
menu):

| Id | Description |
|------|-------------|
| `health` | Hit points |
| `mana` | Mana points |
| `stamina` | Stamina points |
| `spirit` | Spirit points |
| `concentration` | Concentration (DR) |
| `pbarStance` | Stance (the `stance` template) |

`id` is not a closed list — it matches whatever `progressBar` ids the game
sends. (Encumbrance and mind state are separate widgets, not progress ids.)

## Examples

### Minimal Health Bar
```toml
[[windows]]
name = "health"
widget_type = "progress"
id = "health"
rows = 1
cols = 15
numbers_only = true
```

### Colored Mana Bar
```toml
[[windows]]
name = "mana"
widget_type = "progress"
id = "mana"
color = "#4169E1"
rows = 1
cols = 20
```

### Stacked Vitals

```toml
[[windows]]
name = "health"
widget_type = "progress"
id = "health"
row = 0
col = 0
rows = 1
cols = 25
color = "#FF4040"

[[windows]]
name = "mana"
widget_type = "progress"
id = "mana"
row = 1
col = 0
rows = 1
cols = 25
color = "#4169E1"

[[windows]]
name = "stamina"
widget_type = "progress"
id = "stamina"
row = 2
col = 0
rows = 1
cols = 25
color = "#32CD32"
```

## Color Behavior

Each bar renders in a single solid color: the configured `color` if set,
otherwise a per-id default (dark red for health, dark blue for mana, orange
for stamina, gray for spirit, green as the generic fallback). Bars do not
change color based on the current value.
