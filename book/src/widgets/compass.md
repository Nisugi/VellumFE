# Compass

Displays available room exits with directional arrows.

## Basic Usage

```toml
[[windows]]
name = "compass"
widget_type = "compass"
row = 0
col = 0
rows = 5
cols = 9
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `active_color` | string | green | Color for available exits |
| `inactive_color` | string | dark gray | Color for unavailable directions |

## Size Requirements

- The direction grid itself is 3 rows × 7 columns
- The built-in template defaults to 5 rows × 9 columns (room for a border)
  with a 3×7 minimum

## Display

```
↑ ↖ ▲ ↗
  ◀ o ▶
↓ ↙ ▼ ↘
```

- Available exits shown as colored arrows; unavailable directions dimmed
- The center `o` is the **out** direction
- Up (`↑`) and down (`↓`) render in the left column
- Supports all 11 directions: N, S, E, W, NE, NW, SE, SW, Up, Down, Out

## Example

```toml
[[windows]]
name = "compass"
widget_type = "compass"
row = 0
col = 0
rows = 5
cols = 9
show_border = true
border_style = "rounded"
```
