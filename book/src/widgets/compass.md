# Compass

Displays available room exits with directional arrows.

## Basic Usage

```toml
[[windows]]
name = "compass"
widget_type = "compass"
row = 0
col = 0
rows = 3
cols = 7
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `style` | string | `"arrows"` | Display style |

## Size Requirements

- Minimum: 3 rows × 7 columns
- Recommended: 3×7 or 5×9

## Display

```
  N
W ◆ E
  S
```

- Available exits shown with arrows
- Unavailable directions dimmed
- Supports all 10 directions: N, S, E, W, NE, NW, SE, SW, Up, Down, Out

## Example

```toml
[[windows]]
name = "compass"
widget_type = "compass"
row = 0
col = 0
rows = 3
cols = 7
show_border = true
border_style = "rounded"
```
