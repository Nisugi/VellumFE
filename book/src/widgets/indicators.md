# Status Indicators

Status indicator widgets display active character conditions like stunned, hidden, poisoned, etc.

## Overview

Indicator widgets:
- Show active status conditions
- Update automatically when conditions change
- Support icon or text display modes
- Can show multiple conditions

## Configuration

```toml
[[windows]]
name = "status"
type = "indicator"

# Position and size
row = 8
col = 100
width = 20
height = 3

# Indicator-specific options
indicators = ["stunned", "hidden", "webbed", "poisoned"]
style = "icons"           # "icons" or "text"
layout = "horizontal"     # "horizontal" or "vertical"
show_inactive = false     # Show inactive indicators dimmed

# Colors
active_color = "#FF0000"
inactive_color = "#333333"
```

## Properties

### indicators

Which conditions to display:

```toml
# Show specific indicators
indicators = ["stunned", "hidden", "poisoned"]

# Show all indicators
indicators = "all"
```

Available indicators:

| ID | Condition |
|----|-----------|
| `stunned` | Character stunned |
| `hidden` | Character hidden |
| `webbed` | Caught in web |
| `poisoned` | Poisoned |
| `diseased` | Diseased |
| `bleeding` | Bleeding wound |
| `prone` | Lying down |
| `kneeling` | Kneeling |
| `sitting` | Sitting |
| `dead` | Dead |

### style

Display style:

```toml
style = "icons"     # Show icons (default)
style = "text"      # Show text labels
style = "both"      # Icon and text
```

### layout

Indicator arrangement:

```toml
layout = "horizontal"   # Side by side (default)
layout = "vertical"     # Stacked
layout = "grid"         # Grid layout
```

### show_inactive

Whether to show inactive indicators:

```toml
show_inactive = false   # Hide inactive (default)
show_inactive = true    # Show dimmed
```

## Display Modes

### Icons Only

```
⚡ 🕸️ ☠️
```

Compact, visual indicators.

### Text Only

```
STUNNED  WEBBED  POISONED
```

Clear text labels.

### Icons with Text

```
⚡ Stunned  🕸️ Webbed
```

Combined display.

### Vertical

```
⚡ Stunned
🕸️ Webbed
☠️ Poisoned
```

Stacked layout.

## Indicator Icons

Default icons for each condition:

| Condition | Icon | Description |
|-----------|------|-------------|
| stunned | ⚡ | Lightning bolt |
| hidden | 👁️ | Eye |
| webbed | 🕸️ | Spider web |
| poisoned | ☠️ | Skull |
| diseased | 🦠 | Microbe |
| bleeding | 🩸 | Blood drop |
| prone | ⬇️ | Down arrow |
| kneeling | 🧎 | Kneeling |
| sitting | 🪑 | Chair |
| dead | 💀 | Skull |

### Custom Icons

Override default icons:

```toml
[indicator_icons]
stunned = "STUN"
hidden = "HIDE"
poisoned = "POIS"
```

## Examples

### Combat Status Bar

```toml
[[windows]]
name = "combat_status"
type = "indicator"
indicators = ["stunned", "prone", "webbed"]
row = 0
col = 80
width = 30
height = 1
style = "icons"
layout = "horizontal"
active_color = "#FF4444"
```

### Full Status Panel

```toml
[[windows]]
name = "status"
type = "indicator"
indicators = "all"
row = 10
col = 100
width = 15
height = 10
style = "text"
layout = "vertical"
show_inactive = true
active_color = "#FF0000"
inactive_color = "#333333"
```

### Compact Icons

```toml
[[windows]]
name = "status_icons"
type = "indicator"
indicators = ["stunned", "hidden", "poisoned", "diseased"]
row = 5
col = 110
width = 10
height = 1
style = "icons"
show_border = false
```

### Health Conditions

```toml
[[windows]]
name = "health_status"
type = "indicator"
indicators = ["poisoned", "diseased", "bleeding"]
row = 4
col = 80
width = 20
height = 1
style = "both"
active_color = "#FF8800"
title = "Conditions"
```

## Condition Colors

Different colors for different severity:

```toml
[indicator_colors]
# Immediate threats (red)
stunned = "#FF0000"
prone = "#FF0000"

# Combat conditions (orange)
webbed = "#FF8800"
bleeding = "#FF8800"

# Health conditions (yellow)
poisoned = "#FFFF00"
diseased = "#FFFF00"

# Status (blue)
hidden = "#0088FF"
kneeling = "#0088FF"
sitting = "#0088FF"
```

## Animation

Indicators can flash or pulse:

```toml
# Flash active indicators
flash_active = true
flash_rate = 500        # Milliseconds

# Pulse critical conditions
pulse_critical = true
critical_indicators = ["stunned", "dead"]
```

## Data Source

Indicators receive data from XML elements:

```xml
<indicator id="IconSTUNNED" visible="y"/>
<indicator id="IconHIDDEN" visible="n"/>
<indicator id="IconPOISONED" visible="y"/>
```

The `visible` attribute determines active state.

## Troubleshooting

### Indicators not showing

1. Verify condition is actually active
2. Check `indicators` list includes the condition
3. Ensure `show_inactive = false` isn't hiding it

### Wrong icons

1. Check `style` setting
2. Verify icon font support in terminal
3. Use text fallbacks if needed

### Layout issues

1. Adjust `width` and `height`
2. Change `layout` mode
3. Reduce number of indicators

## See Also

- [Injury Doll](./injury-doll.md) - Body injury display
- [Active Effects](./active-effects.md) - Buff/debuff display
- [Progress Bars](./progress-bars.md) - Health status
