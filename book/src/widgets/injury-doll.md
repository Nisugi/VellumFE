# Injury Display

Shows body part injuries as a visual "paper doll" stick figure.

## Basic Usage

```toml
[[windows]]
name = "injuries"
widget_type = "injury_doll"
row = 0
col = 0
rows = 8
cols = 10
```

## Display

A compact stick figure, with eyes above the head and short text labels on
the right for parts that don't fit the figure:

```
👁   👁
  0      nk
 /|\
o | o    bk
 / \
o   o    ns
```

- Head `0`, arms `/ \`, chest and abdomen `|`, hands and feet `o`
- `nk` = neck, `bk` = back, `ns` = nerves
- Each body part changes color with injury severity

## Colors

Seven severity levels, each with its own configurable color:

| Level | Default color |
|-------|---------------|
| Healthy | `#333333` (dark gray) |
| Injury 1 | `#aa5500` (brown) |
| Injury 2 | `#ff8800` (orange) |
| Injury 3 | `#ff0000` (red) |
| Scar 1 | `#999999` |
| Scar 2 | `#777777` |
| Scar 3 | `#555555` |

Override with `injury_default_color`, `injury1_color` … `injury3_color`,
and `scar1_color` … `scar3_color`.

## Size Requirements

- Minimum: 6 rows × 8 columns
- Default: 8 rows × 10 columns

## Example

```toml
[[windows]]
name = "injuries"
widget_type = "injury_doll"
row = 0
col = 0
rows = 8
cols = 10
show_border = true
border_style = "rounded"
title = "Injuries"
```

## Body Parts Tracked

- Head, Neck
- Chest, Abdomen, Back
- Left/Right Arm
- Left/Right Hand
- Left/Right Leg
- Left/Right Eye
- Nerves
