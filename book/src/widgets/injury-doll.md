# Injury Doll

The injury doll widget displays a visual representation of injuries to different body parts.

## Overview

Injury doll widgets:
- Show body part injury severity visually
- Display wounds, scars, and blood loss
- Update as injuries change
- Support multiple display styles

## Configuration

```toml
[[windows]]
name = "injuries"
type = "injury_doll"

# Position and size
row = 0
col = 100
width = 20
height = 12

# Injury-specific options
style = "ascii"           # "ascii", "simple", "detailed"
show_labels = true        # Show body part names
show_severity = true      # Show injury level numbers
orientation = "vertical"  # "vertical" or "horizontal"

# Colors by severity
healthy_color = "#00FF00"
minor_color = "#FFFF00"
moderate_color = "#FF8800"
severe_color = "#FF0000"
critical_color = "#FF0000"
```

## Properties

### style

Display style for the injury doll:

```toml
style = "ascii"      # ASCII art body (default)
style = "simple"     # Text list only
style = "detailed"   # Full visual with numbers
```

### show_labels

Display body part names:

```toml
show_labels = true    # Show "Head", "Chest", etc.
show_labels = false   # Visual only
```

### show_severity

Display injury severity numbers:

```toml
show_severity = true    # Show severity level (1-4)
show_severity = false   # Color-coded only
```

### orientation

Layout orientation:

```toml
orientation = "vertical"    # Tall layout (default)
orientation = "horizontal"  # Wide layout
```

## Body Parts

The injury doll tracks these body parts:

| Part | XML ID | Description |
|------|--------|-------------|
| Head | `head` | Head injuries |
| Neck | `neck` | Neck injuries |
| Chest | `chest` | Chest/torso |
| Abdomen | `abdomen` | Stomach area |
| Back | `back` | Back injuries |
| Left Arm | `leftArm` | Left arm |
| Right Arm | `rightArm` | Right arm |
| Left Hand | `leftHand` | Left hand |
| Right Hand | `rightHand` | Right hand |
| Left Leg | `leftLeg` | Left leg |
| Right Leg | `rightLeg` | Right leg |
| Left Eye | `leftEye` | Left eye |
| Right Eye | `rightEye` | Right eye |
| Nerves | `nsys` | Nervous system |

## Severity Levels

| Level | Name | Color (default) | Description |
|-------|------|-----------------|-------------|
| 0 | Healthy | Green | No injury |
| 1 | Minor | Yellow | Minor wound |
| 2 | Moderate | Orange | Moderate injury |
| 3 | Severe | Red | Severe wound |
| 4 | Critical | Bright Red | Critical injury |

## Display Styles

### ASCII Art Style

```
┌─ Injuries ───────────┐
│       ( o o )        │
│         ─┬─          │
│        ──┼──         │
│          │           │
│         / \          │
│        /   \         │
└──────────────────────┘
```

Body parts are colored by severity.

### Simple Text Style

```
┌─ Injuries ───────────┐
│ Head: Minor          │
│ Chest: Moderate      │
│ Right Arm: Severe    │
│ Left Leg: Minor      │
└──────────────────────┘
```

### Detailed Style

```
┌─ Injuries ───────────┐
│ HEAD     [████░░] 2  │
│ CHEST    [██████] 3  │
│ R.ARM    [████████] 4│
│ L.LEG    [██░░░░] 1  │
│ BACK     [░░░░░░] 0  │
└──────────────────────┘
```

## Examples

### Standard Injury Doll

```toml
[[windows]]
name = "injuries"
type = "injury_doll"
row = 0
col = 100
width = 20
height = 12
style = "ascii"
show_labels = true
show_severity = true
border_style = "rounded"
```

### Compact Text List

```toml
[[windows]]
name = "injury_list"
type = "injury_doll"
row = 0
col = 100
width = 25
height = 8
style = "simple"
show_severity = true
title = "Wounds"
```

### Minimal Status

```toml
[[windows]]
name = "injury_mini"
type = "injury_doll"
row = 0
col = 100
width = 15
height = 5
style = "simple"
show_labels = false
show_severity = false
show_border = false
```

### Wide Layout

```toml
[[windows]]
name = "injuries_wide"
type = "injury_doll"
row = 20
col = 0
width = 60
height = 5
style = "detailed"
orientation = "horizontal"
```

## Severity Colors

Customize colors for each severity level:

```toml
[[windows]]
name = "injuries"
type = "injury_doll"

# Custom severity colors
[windows.colors]
healthy = "#00FF00"    # Green
minor = "#FFFF00"      # Yellow
moderate = "#FF8800"   # Orange
severe = "#FF4444"     # Red
critical = "#FF0000"   # Bright red
bleeding = "#880000"   # Dark red for blood
scarred = "#808080"    # Gray for scars
```

## Wound Types

Different wound states may display differently:

| State | Visual | Description |
|-------|--------|-------------|
| Fresh wound | Bright color | Recent injury |
| Bleeding | Pulsing/dark | Active bleeding |
| Scarred | Gray | Old injury |
| Healing | Fading | Being healed |

## Animation

Injuries can animate to draw attention:

```toml
# Flash critical injuries
flash_critical = true
flash_rate = 500        # Milliseconds

# Pulse bleeding wounds
pulse_bleeding = true
```

## Data Source

Injury data comes from XML elements:

```xml
<indicator id="IconBLEEDING" visible="y"/>
<body>
  <part id="head" value="0"/>
  <part id="chest" value="2"/>
  <part id="rightArm" value="3"/>
</body>
```

The `value` attribute indicates severity (0-4).

## Integration with Other Widgets

### Combined with Health Bars

```toml
# Health bar
[[windows]]
name = "health"
type = "progress"
source = "health"
row = 0
col = 80
width = 20
height = 1

# Injury doll below
[[windows]]
name = "injuries"
type = "injury_doll"
row = 1
col = 80
width = 20
height = 10
```

### Part of Status Panel

```toml
# Status indicators
[[windows]]
name = "status"
type = "indicator"
row = 0
col = 100
width = 20
height = 2

# Injuries below status
[[windows]]
name = "injuries"
type = "injury_doll"
row = 2
col = 100
width = 20
height = 10
```

## Troubleshooting

### Injuries not updating

1. Verify receiving body data from game
2. Check window is using correct type
3. Ensure body part IDs match XML

### Wrong colors

1. Check severity color configuration
2. Verify theme colors aren't overriding
3. Test with default colors first

### Display too small

1. Increase width and height
2. Try "simple" style for compact spaces
3. Adjust orientation for available space

## See Also

- [Status Indicators](./indicators.md) - Status condition display
- [Progress Bars](./progress-bars.md) - Health/vitals display
- [Active Effects](./active-effects.md) - Buff/debuff display

