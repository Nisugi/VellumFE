# Countdowns

Display roundtime, cast time, and stun timers.

## Basic Usage

```toml
[[windows]]
name = "roundtime"
widget_type = "countdown"
id = "roundtime"
row = 0
col = 0
rows = 1
cols = 15
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `id` | string | required | Timer type |
| `label` | string | auto | Custom label |
| `bar_color` | string | auto | Bar color |

## Timer Types

| ID | Description |
|----|-------------|
| `roundtime` | Action roundtime (RT) |
| `casttime` | Spell cast time (CT) |
| `stuntime` | Stun duration |

## Examples

### Roundtime Bar
```toml
[[windows]]
name = "rt"
widget_type = "countdown"
id = "roundtime"
rows = 1
cols = 20
bar_color = "#FFD700"
```

### Cast Time
```toml
[[windows]]
name = "ct"
widget_type = "countdown"
id = "casttime"
rows = 1
cols = 15
bar_color = "#9370DB"
```

### Stun Timer
```toml
[[windows]]
name = "stun"
widget_type = "countdown"
id = "stuntime"
rows = 1
cols = 15
bar_color = "#FF4500"
```

## Display

- Shows remaining seconds with visual bar
- Bar depletes as time passes
- Empty/hidden when not active
