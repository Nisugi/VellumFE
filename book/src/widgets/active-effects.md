# Active Effects

The active effects widget displays current buffs, debuffs, spells, and other timed effects on your character.

## Overview

Active effects widgets:
- Show currently active spells and abilities
- Display remaining duration
- Track buff and debuff status
- Update in real-time as effects expire

## Configuration

```toml
[[windows]]
name = "effects"
type = "active_effects"

# Position and size
row = 0
col = 100
width = 30
height = 15

# Effect display options
show_duration = true       # Show remaining time
show_icons = true          # Show effect icons
sort_by = "duration"       # "duration", "name", "type"
group_by_type = false      # Group buffs/debuffs

# Filter options
show_buffs = true
show_debuffs = true
show_spells = true
show_abilities = true

# Colors
buff_color = "#00FF00"
debuff_color = "#FF0000"
spell_color = "#00FFFF"
expiring_color = "#FFFF00"
```

## Properties

### show_duration

Display remaining time for effects:

```toml
show_duration = true    # Show "Spirit Shield (2:45)"
show_duration = false   # Show "Spirit Shield" only
```

### show_icons

Display icons for effects:

```toml
show_icons = true     # Show icons/symbols
show_icons = false    # Text only
```

### sort_by

How to sort the effect list:

```toml
sort_by = "duration"   # Shortest remaining first
sort_by = "name"       # Alphabetical
sort_by = "type"       # Buffs, then debuffs, etc.
```

### group_by_type

Group effects by category:

```toml
group_by_type = true    # Separate sections
group_by_type = false   # Single list (default)
```

### Filter Options

Control which effect types to display:

```toml
show_buffs = true       # Beneficial effects
show_debuffs = true     # Harmful effects
show_spells = true      # Active spells
show_abilities = true   # Active abilities
```

## Display Format

### Standard List

```
┌─ Active Effects ─────────────┐
│ ✦ Spirit Shield      (5:23)  │
│ ✦ Elemental Defense  (4:15)  │
│ ✦ Haste              (2:30)  │
│ ✧ Poison             (1:45)  │
│ ✦ Minor Sanctuary    (0:58)  │
└──────────────────────────────┘
```

- ✦ = Buff (beneficial)
- ✧ = Debuff (harmful)

### Grouped Display

```
┌─ Active Effects ─────────────┐
│ BUFFS:                       │
│   Spirit Shield      (5:23)  │
│   Elemental Defense  (4:15)  │
│   Haste              (2:30)  │
│                              │
│ DEBUFFS:                     │
│   Poison             (1:45)  │
└──────────────────────────────┘
```

### Compact Display

```
┌─ Effects ────────────────────┐
│ Shield:5:23 Defense:4:15     │
│ Haste:2:30 Poison:1:45       │
└──────────────────────────────┘
```

## Effect Types

| Type | Description | Default Color |
|------|-------------|---------------|
| Buff | Beneficial effect | Green |
| Debuff | Harmful effect | Red |
| Spell | Active spell | Cyan |
| Ability | Character ability | Blue |
| Item | Item effect | Purple |

## Duration Display

### Time Formats

| Remaining | Display |
|-----------|---------|
| > 1 hour | 1:23:45 |
| > 1 minute | 5:23 |
| < 1 minute | 0:45 |
| Expiring soon | Flashing |
| Permanent | ∞ or -- |

### Expiring Warning

Effects close to expiring can be highlighted:

```toml
expiring_threshold = 30    # Seconds before expiring
expiring_color = "#FFFF00" # Yellow warning
expiring_flash = true      # Flash when expiring
```

## Examples

### Full Effects Panel

```toml
[[windows]]
name = "effects"
type = "active_effects"
row = 0
col = 100
width = 30
height = 20
show_duration = true
show_icons = true
sort_by = "duration"
group_by_type = true
title = "Active Effects"
```

### Buffs Only

```toml
[[windows]]
name = "buffs"
type = "active_effects"
row = 0
col = 100
width = 25
height = 10
show_buffs = true
show_debuffs = false
show_spells = true
buff_color = "#00FF00"
title = "Buffs"
```

### Debuff Monitor

```toml
[[windows]]
name = "debuffs"
type = "active_effects"
row = 10
col = 100
width = 25
height = 8
show_buffs = false
show_debuffs = true
debuff_color = "#FF4444"
expiring_color = "#FFFF00"
title = "Debuffs"
```

### Compact Status Bar

```toml
[[windows]]
name = "effect_bar"
type = "active_effects"
row = 0
col = 50
width = 50
height = 2
show_icons = true
show_duration = false
sort_by = "type"
show_border = false
```

### Spell Timer

```toml
[[windows]]
name = "spells"
type = "active_effects"
row = 15
col = 100
width = 25
height = 12
show_buffs = false
show_debuffs = false
show_spells = true
sort_by = "duration"
title = "Active Spells"
```

## Effect Colors

Customize colors by effect type:

```toml
[[windows]]
name = "effects"
type = "active_effects"

[windows.colors]
buff = "#00FF00"          # Green
debuff = "#FF0000"        # Red
spell = "#00FFFF"         # Cyan
ability = "#0088FF"       # Blue
item = "#FF00FF"          # Magenta
expiring = "#FFFF00"      # Yellow
permanent = "#FFFFFF"     # White
```

### Severity-Based Colors

For debuffs, color by severity:

```toml
[windows.debuff_colors]
minor = "#FFFF00"      # Yellow
moderate = "#FF8800"   # Orange
severe = "#FF0000"     # Red
critical = "#FF0000"   # Bright red + flash
```

## Animation

### Expiring Effects

```toml
# Flash effects about to expire
flash_expiring = true
flash_threshold = 30      # Seconds
flash_rate = 500          # Milliseconds

# Fade out expired effects
fade_on_expire = true
fade_duration = 1000      # Milliseconds
```

### New Effects

```toml
# Highlight newly added effects
highlight_new = true
highlight_duration = 2000  # Milliseconds
highlight_color = "#FFFFFF"
```

## Data Source

Effects are tracked from multiple XML sources:

```xml
<spell>Spirit Shield</spell>
<duration value="323"/>

<effect name="Poison" type="debuff" duration="105"/>

<component id="activeSpells">
  Spirit Shield, Elemental Defense
</component>
```

## Integration Examples

### Combined with Spell List

```toml
# Active effects
[[windows]]
name = "active"
type = "active_effects"
row = 0
col = 100
width = 25
height = 10

# Known spells below
[[windows]]
name = "spells"
type = "spells"
row = 10
col = 100
width = 25
height = 15
```

### Part of Combat Dashboard

```toml
# Health/vitals
[[windows]]
name = "vitals"
type = "progress"
row = 0
col = 80
width = 40
height = 4

# Active effects
[[windows]]
name = "effects"
type = "active_effects"
row = 4
col = 80
width = 20
height = 8

# Debuff alerts
[[windows]]
name = "debuffs"
type = "active_effects"
row = 4
col = 100
width = 20
height = 8
show_buffs = false
show_debuffs = true
```

## Troubleshooting

### Effects not showing

1. Verify game is sending effect data
2. Check filter settings (show_buffs, etc.)
3. Ensure correct widget type

### Duration not updating

1. Effects may not send duration updates
2. Check timer refresh rate
3. Some effects show as permanent

### Missing effect types

1. Enable all show_* options to test
2. Check if game sends that effect type
3. Verify effect parsing in logs

## See Also

- [Spells](./spells.md) - Known spell list
- [Status Indicators](./indicators.md) - Status conditions
- [Progress Bars](./progress-bars.md) - Vitals display
- [Countdowns](./countdowns.md) - Timer display

