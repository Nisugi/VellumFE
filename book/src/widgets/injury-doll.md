# Injury Display

Shows body part injuries as a visual "paper doll".

## Basic Usage

```toml
[[windows]]
name = "injuries"
widget_type = "injury_doll"
row = 0
col = 0
rows = 10
cols = 15
```

## Display

```
    [Head]
[L.Arm][Chest][R.Arm]
   [L.Leg][R.Leg]
```

- Each body part changes color based on injury severity
- Green (healthy) → Yellow (minor) → Orange (moderate) → Red (severe)

## Size Requirements

- Minimum: 8 rows × 12 columns
- Recommended: 10×15

## Example

```toml
[[windows]]
name = "injuries"
widget_type = "injury_doll"
row = 0
col = 0
rows = 10
cols = 15
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
