# Spells Widget

The spells widget displays your character's known spells, spell preparation status, and casting availability.

## Overview

Spells widgets:
- List known spells and abilities
- Show spell preparation status
- Display mana costs and cooldowns
- Enable quick spell casting

## Configuration

```toml
[[windows]]
name = "spells"
type = "spells"

# Position and size
row = 0
col = 100
width = 30
height = 20

# Spell display options
show_prepared = true       # Show preparation status
show_cost = true           # Show mana cost
show_circle = true         # Show spell circle
group_by_circle = true     # Group by spell circle
sort_by = "circle"         # "circle", "name", "cost"

# Interaction
clickable = true           # Click to cast
show_tooltip = true        # Show spell info on hover

# Colors
prepared_color = "#00FF00"
unprepared_color = "#808080"
unavailable_color = "#FF0000"
```

## Properties

### show_prepared

Display spell preparation status:

```toml
show_prepared = true    # Show ✓/✗ for prepared
show_prepared = false   # List only
```

### show_cost

Display mana cost:

```toml
show_cost = true     # Show "(5 mana)"
show_cost = false    # Name only
```

### show_circle

Display spell circle number:

```toml
show_circle = true    # Show circle/level
show_circle = false   # Hide circle
```

### group_by_circle

Group spells by circle:

```toml
group_by_circle = true    # Sections by circle
group_by_circle = false   # Single list
```

### sort_by

Sort order for spells:

```toml
sort_by = "circle"   # By spell circle
sort_by = "name"     # Alphabetical
sort_by = "cost"     # By mana cost
```

## Display Format

### Grouped by Circle

```
┌─ Spells ─────────────────────────┐
│ ─── Minor Spirit (100s) ───      │
│ ✓ Spirit Warding I        (1)    │
│ ✓ Spirit Defense II       (2)    │
│ ✗ Spirit Fog              (6)    │
│                                  │
│ ─── Major Spirit (200s) ───      │
│ ✓ Spirit Shield           (3)    │
│ ✗ Elemental Defense       (5)    │
└──────────────────────────────────┘
```

### Simple List

```
┌─ Spells ─────────────────────────┐
│ ✓ Spirit Warding I        (1)    │
│ ✓ Spirit Defense II       (2)    │
│ ✓ Spirit Shield           (3)    │
│ ✗ Elemental Defense       (5)    │
│ ✗ Spirit Fog              (6)    │
└──────────────────────────────────┘
```

### Compact Format

```
┌─ Spells ──────────────┐
│ 101✓ 102✓ 103✗ 104✓   │
│ 201✓ 202✗ 203✓ 204✗   │
└───────────────────────┘
```

## Spell Status

| Symbol | Meaning | Color (default) |
|--------|---------|-----------------|
| ✓ | Prepared | Green |
| ✗ | Not prepared | Gray |
| ◉ | Currently casting | Cyan |
| ⊘ | Unavailable | Red |
| ⏱ | On cooldown | Yellow |

## Examples

### Full Spell Panel

```toml
[[windows]]
name = "spells"
type = "spells"
row = 0
col = 100
width = 35
height = 25
show_prepared = true
show_cost = true
show_circle = true
group_by_circle = true
clickable = true
title = "Known Spells"
```

### Compact Spell Bar

```toml
[[windows]]
name = "spell_bar"
type = "spells"
row = 0
col = 50
width = 40
height = 3
group_by_circle = false
show_cost = false
layout = "horizontal"
show_border = false
```

### Prepared Spells Only

```toml
[[windows]]
name = "prepared"
type = "spells"
row = 10
col = 100
width = 25
height = 10
filter = "prepared"    # Only show prepared
show_prepared = false  # No status icon needed
title = "Ready Spells"
```

### Quick Cast Panel

```toml
[[windows]]
name = "quick_cast"
type = "spells"
row = 0
col = 100
width = 20
height = 15
favorite_spells = [101, 103, 107, 201, 206]
clickable = true
double_click_action = "cast"
title = "Quick Cast"
```

## Spell Interaction

### Click Actions

| Action | Result |
|--------|--------|
| Single click | Select spell |
| Double click | Cast spell |
| Right click | Spell info menu |
| Shift+click | Prepare spell |

### Context Menu

```
┌────────────────────┐
│ Cast               │
│ Prepare            │
│ ────────────────── │
│ Spell Info         │
│ Show Incantation   │
│ ────────────────── │
│ Add to Quick Cast  │
│ Set Hotkey         │
└────────────────────┘
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑/↓` | Navigate spells |
| `Enter` | Cast selected |
| `p` | Prepare selected |
| `i` | Show info |
| `1-9` | Quick cast by position |

## Spell Colors

Color spells by type or circle:

```toml
[[windows]]
name = "spells"
type = "spells"

[windows.spell_colors]
minor_spirit = "#00FFFF"    # Cyan (100s)
major_spirit = "#0088FF"    # Blue (200s)
cleric = "#FFFFFF"          # White (300s)
minor_elemental = "#FF8800" # Orange (400s)
major_elemental = "#FF0000" # Red (500s)
ranger = "#00FF00"          # Green (600s)
sorcerer = "#FF00FF"        # Purple (700s)
wizard = "#FFFF00"          # Yellow (900s)
```

### Status Colors

```toml
[windows.status_colors]
prepared = "#00FF00"        # Green
unprepared = "#808080"      # Gray
casting = "#00FFFF"         # Cyan
cooldown = "#FFFF00"        # Yellow
unavailable = "#FF0000"     # Red
insufficient_mana = "#FF8800" # Orange
```

## Mana Cost Display

Show mana costs with current mana:

```toml
show_cost = true
show_current_mana = true   # Show current/max
highlight_affordable = true # Dim unaffordable
```

```
┌─ Spells ─────────────────────────┐
│ Mana: 45/100                     │
│ ──────────────────────────────── │
│ ✓ Spirit Shield           (3)   │
│ ✓ Elemental Defense       (5)   │
│ ✗ Major Sanctuary        (50)   │  ← Dimmed (can't afford)
└──────────────────────────────────┘
```

## Cooldown Display

Show cooldown timers:

```toml
show_cooldowns = true
cooldown_format = "remaining"   # "remaining", "ready_at", "bar"
```

```
┌─ Spells ─────────────────────────┐
│ ✓ Spirit Shield           Ready  │
│ ⏱ Mass Sanctuary         (0:45)  │
│ ⏱ Divine Intervention    (2:30)  │
└──────────────────────────────────┘
```

## Data Source

Spell data comes from XML elements:

```xml
<spellList>
  <spell id="101" name="Spirit Warding I" prepared="y"/>
  <spell id="102" name="Spirit Barrier" prepared="n"/>
</spellList>

<spell>Spirit Shield</spell>
<duration value="300"/>
```

## Integration Examples

### With Active Effects

```toml
# Active spells/effects
[[windows]]
name = "active"
type = "active_effects"
row = 0
col = 100
width = 25
height = 10
title = "Active"

# Known spells below
[[windows]]
name = "spells"
type = "spells"
row = 10
col = 100
width = 25
height = 20
title = "Spells"
```

### Combat Spell Panel

```toml
# Mana bar
[[windows]]
name = "mana"
type = "progress"
source = "mana"
row = 0
col = 80
width = 30
height = 1

# Offensive spells
[[windows]]
name = "attack_spells"
type = "spells"
row = 1
col = 80
width = 15
height = 10
filter_circles = [400, 500, 700]
title = "Attack"

# Defensive spells
[[windows]]
name = "defense_spells"
type = "spells"
row = 1
col = 95
width = 15
height = 10
filter_circles = [100, 200, 300]
title = "Defense"
```

## Troubleshooting

### Spells not showing

1. Verify spell list data is being received
2. Check filter settings
3. Ensure widget type is correct

### Preparation status wrong

1. Check game is sending status updates
2. Verify spell ID matching
3. Manual refresh may help

### Can't click to cast

1. Verify clickable = true
2. Check double_click_action setting
3. Ensure widget has focus

### Missing spell circles

1. Check filter_circles setting
2. Verify all circles are enabled
3. Check data parsing

## See Also

- [Active Effects](./active-effects.md) - Active spell display
- [Progress Bars](./progress-bars.md) - Mana display
- [Hands](./hands.md) - Spell hand display
- [Countdowns](./countdowns.md) - Cast time display

