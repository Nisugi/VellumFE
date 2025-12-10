# Dashboard Widget

The dashboard widget combines multiple data displays into a single composite panel, providing an at-a-glance overview of character status.

## Overview

Dashboard widgets:
- Combine multiple metrics in one widget
- Provide compact status overview
- Reduce window count
- Customizable component selection

## Configuration

```toml
[[windows]]
name = "dashboard"
type = "dashboard"

# Position and size
row = 0
col = 0
width = 40
height = 15

# Components to include
components = [
    "vitals",
    "experience",
    "stance",
    "encumbrance",
    "wounds"
]

# Layout
layout = "vertical"       # "vertical", "horizontal", "grid"
compact = false           # Condensed display

# Visual options
show_labels = true
show_dividers = true
```

## Properties

### components

Which status elements to display:

```toml
# Full dashboard
components = [
    "vitals",       # Health, mana, stamina, spirit
    "experience",   # XP info
    "stance",       # Combat stance
    "encumbrance",  # Weight carried
    "wounds",       # Injury summary
    "status",       # Status indicators
    "wealth",       # Silver/gold
    "position"      # Standing/sitting/etc.
]

# Minimal dashboard
components = ["vitals", "stance"]
```

### layout

Component arrangement:

```toml
layout = "vertical"     # Stacked top to bottom
layout = "horizontal"   # Side by side
layout = "grid"         # 2-column grid
```

### compact

Condensed display mode:

```toml
compact = false   # Full labels and spacing
compact = true    # Minimal spacing, abbreviations
```

### show_labels

Display component labels:

```toml
show_labels = true    # "Health: 100/100"
show_labels = false   # "100/100"
```

### show_dividers

Display lines between components:

```toml
show_dividers = true    # Lines between sections
show_dividers = false   # No dividers
```

## Available Components

### vitals

Health, mana, stamina, spirit bars:

```
Health:  [████████████████░░░░] 80%
Mana:    [████████████░░░░░░░░] 60%
Stamina: [██████████████████░░] 90%
Spirit:  [████████████████████] 100%
```

### experience

Experience and level info:

```
Level: 42
Exp: 12,345,678 / 15,000,000
Mind: Clear
```

### stance

Combat stance display:

```
Stance: Offensive (100/80)
```

### encumbrance

Weight and capacity:

```
Encumbrance: 45.2 / 100 lbs (45%)
```

### wounds

Injury summary:

```
Wounds: Head(1) Chest(2) RArm(3)
```

### status

Active status conditions:

```
Status: Stunned, Poisoned
```

### wealth

Currency display:

```
Silver: 12,345
```

### position

Character position:

```
Position: Standing
```

### roundtime

Current roundtime:

```
RT: 3 seconds
```

### target

Current target:

```
Target: a massive troll
```

## Display Formats

### Standard Vertical

```
┌─ Dashboard ──────────────────────┐
│ Health:  [████████░░] 80/100     │
│ Mana:    [██████░░░░] 60/100     │
│ Stamina: [█████████░] 90/100     │
│ ──────────────────────────────── │
│ Level 42 | Mind: Clear           │
│ Exp: 12.3M / 15M                 │
│ ──────────────────────────────── │
│ Stance: Offensive (100/80)       │
│ Position: Standing               │
│ ──────────────────────────────── │
│ Wounds: None                     │
│ Status: Clear                    │
└──────────────────────────────────┘
```

### Compact Horizontal

```
┌─ Status ─────────────────────────────────────────────┐
│ HP:80% MP:60% ST:90% | L42 | Off(100/80) | Standing  │
└──────────────────────────────────────────────────────┘
```

### Grid Layout

```
┌─ Dashboard ──────────────────────┐
│ Health:  80% │ Level: 42         │
│ Mana:    60% │ Mind: Clear       │
│ Stamina: 90% │ Exp: 12.3M        │
│ ─────────────┼────────────────── │
│ Stance: Off  │ Position: Stand   │
│ RT: 0        │ Silver: 12,345    │
└──────────────────────────────────┘
```

## Examples

### Full Status Dashboard

```toml
[[windows]]
name = "dashboard"
type = "dashboard"
row = 0
col = 80
width = 40
height = 20
components = [
    "vitals",
    "experience",
    "stance",
    "encumbrance",
    "wounds",
    "status",
    "wealth",
    "position"
]
layout = "vertical"
show_dividers = true
title = "Character Status"
```

### Combat Dashboard

```toml
[[windows]]
name = "combat_dash"
type = "dashboard"
row = 0
col = 80
width = 35
height = 12
components = [
    "vitals",
    "stance",
    "roundtime",
    "target",
    "wounds"
]
layout = "vertical"
compact = true
title = "Combat"
```

### Minimal Status Bar

```toml
[[windows]]
name = "status_bar"
type = "dashboard"
row = 0
col = 0
width = 80
height = 2
components = ["vitals", "stance", "roundtime"]
layout = "horizontal"
compact = true
show_border = false
show_dividers = false
```

### Experience Focus

```toml
[[windows]]
name = "exp_dash"
type = "dashboard"
row = 10
col = 100
width = 25
height = 8
components = ["experience", "wealth"]
layout = "vertical"
title = "Progress"
```

### Hunting Dashboard

```toml
[[windows]]
name = "hunting"
type = "dashboard"
row = 0
col = 80
width = 40
height = 15
components = [
    "vitals",
    "stance",
    "wounds",
    "target",
    "roundtime",
    "encumbrance"
]
layout = "grid"
compact = false
title = "Hunting Status"
```

## Component Colors

Customize colors for each component:

```toml
[[windows]]
name = "dashboard"
type = "dashboard"

[windows.colors]
# Vital bar colors
health = "#FF0000"
mana = "#0088FF"
stamina = "#00FF00"
spirit = "#FFFF00"

# Status colors
wounded = "#FF8800"
stunned = "#FF0000"
hidden = "#00FFFF"

# Text colors
labels = "#808080"
values = "#FFFFFF"
dividers = "#404040"
```

### Threshold Colors

Change colors based on values:

```toml
[windows.health_thresholds]
high = "#00FF00"      # > 75%
medium = "#FFFF00"    # 25-75%
low = "#FF8800"       # 10-25%
critical = "#FF0000"  # < 10%
```

## Data Sources

Dashboard pulls from multiple XML sources:

```xml
<dialogData id="health" text="80/100"/>
<dialogData id="mana" text="60/100"/>
<dialogData id="stamina" text="90/100"/>
<dialogData id="spirit" text="100/100"/>

<dialogData id="stance" text="offensive"/>
<indicator id="IconSTANDING" visible="y"/>

<exp>12345678</exp>
<nextExp>15000000</nextExp>
```

## Building Custom Dashboards

### Selective Components

Only include what you need:

```toml
# Minimalist - just vitals
components = ["vitals"]

# Combat-focused
components = ["vitals", "stance", "wounds", "roundtime"]

# Exploration-focused
components = ["vitals", "encumbrance", "position"]

# Progression-focused
components = ["experience", "wealth"]
```

### Multiple Dashboards

Use different dashboards for different activities:

```toml
# Main status (always visible)
[[windows]]
name = "main_dash"
type = "dashboard"
row = 0
col = 80
width = 40
height = 8
components = ["vitals", "stance"]

# Combat details (show during hunting)
[[windows]]
name = "combat_dash"
type = "dashboard"
row = 8
col = 80
width = 40
height = 10
components = ["wounds", "target", "roundtime"]
visible = false  # Toggle via keybind
```

## Troubleshooting

### Components not showing

1. Verify component name is correct
2. Check game is sending that data
3. Ensure dashboard has sufficient size

### Layout looks wrong

1. Adjust width/height for layout type
2. Try different layout option
3. Enable/disable compact mode

### Data not updating

1. Verify XML data is being received
2. Check component is correctly configured
3. Test individual widgets for same data

### Performance issues

1. Reduce number of components
2. Use compact mode
3. Increase refresh interval

## See Also

- [Progress Bars](./progress-bars.md) - Individual vital displays
- [Status Indicators](./indicators.md) - Status condition display
- [Injury Doll](./injury-doll.md) - Wound display
- [Performance](./performance.md) - Performance metrics

