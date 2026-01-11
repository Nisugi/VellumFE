# Hands

Display items held in left and right hands.

## Basic Usage

```toml
[[windows]]
name = "right_hand"
widget_type = "hand"
hand = "right"
row = 0
col = 0
rows = 1
cols = 25
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `hand` | string | required | `"left"` or `"right"` |

## Examples

### Right Hand
```toml
[[windows]]
name = "right"
widget_type = "hand"
hand = "right"
rows = 1
cols = 30
title = "R:"
```

### Left Hand
```toml
[[windows]]
name = "left"
widget_type = "hand"
hand = "left"
rows = 1
cols = 30
title = "L:"
```

### Side by Side

```toml
[[windows]]
name = "right_hand"
widget_type = "hand"
hand = "right"
row = 0
col = 0
rows = 1
cols = 25
show_border = false
title = "R:"

[[windows]]
name = "left_hand"
widget_type = "hand"
hand = "left"
row = 0
col = 25
rows = 1
cols = 25
show_border = false
title = "L:"
```

## Interaction

- Click item name to interact
- Right-click for context menu
- Shows "Empty" when nothing held
