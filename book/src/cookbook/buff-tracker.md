# Buff Tracker

Display active buffs, debuffs, and spell effects with countdown timers.

## Goal

Track all active effects on your character with visual progress bars showing time remaining.

## The Problem

GemStone IV sends buff data via the `dialogData id='Buffs'` XML elements. These include spell effects, sigils, and other timed buffs with progress bars and countdown timers.

## Layout

```
┌─────────────────────────────────────────────┬────────────────────────┐
│                                             │ Active Effects         │
│                                             ├────────────────────────┤
│                                             │ ▓▓▓▓▓▓▓░░░ 3:44        │
│                                             │ Fasthr's Reward        │
│                                             │                        │
│              Main Game Text                 │ ▓▓▓▓▓▓▓▓▓▓ 0:76        │
│                                             │ Celerity               │
│                                             │                        │
│                                             │ ▓▓▓▓░░░░░░ 1:57        │
│                                             │ Sigil of Defense       │
│                                             │                        │
│                                             │ ▓▓░░░░░░░░ 0:08        │
│                                             │ Sigil of Major Bane    │
└─────────────────────────────────────────────┴────────────────────────┘
```

## Configuration

### layout.toml

```toml
# Active buffs panel
[[widgets]]
type = "effects"
name = "buffs"
category = "Buffs"
x = 85
y = 0
width = 35
height = 20
show_timers = true
show_border = true
title = "Active Effects"
sort_by = "time_remaining"  # or "name", "value"

# Debuffs panel (if you want separate tracking)
[[widgets]]
type = "effects"
name = "debuffs"
category = "Debuffs"
x = 85
y = 20
width = 35
height = 10
show_timers = true
show_border = true
title = "Debuffs"
color_scheme = "danger"

# Cooldowns panel
[[widgets]]
type = "effects"
name = "cooldowns"
category = "Cooldowns"
x = 85
y = 30
width = 35
height = 8
show_timers = true
show_border = true
title = "Cooldowns"
```

### Color Configuration

```toml
# colors.toml

# Buff bar colors by time remaining
buff_full = "#00FF00"      # Green - plenty of time
buff_medium = "#FFFF00"    # Yellow - getting low
buff_low = "#FF6600"       # Orange - almost expired
buff_critical = "#FF0000"  # Red - about to expire

# Category colors
buffs_color = "#00AAFF"    # Blue for buffs
debuffs_color = "#FF0000"  # Red for debuffs
cooldowns_color = "#FFAA00" # Orange for cooldowns
```

## Alerts for Expiring Buffs

```toml
# triggers.toml

# Celerity about to expire
[[triggers]]
name = "celerity_warning"
pattern = "The heightened speed of Celerity fades"
tts = "Celerity expired"
sound = "buff_expire.wav"

# Spell shield warning
[[triggers]]
name = "shield_warning"
pattern = "Your (spell|elemental) shield dissipates"
tts = "Shield down"
sound = "warning.wav"

# Generic buff fade
[[triggers]]
name = "buff_fade"
pattern = "The (glow|aura|effect) of .+ fades"
sound = "buff_expire.wav"
cooldown = 1000
```

## Advanced: Profession-Specific Buffs

### Ranger Buffs

```toml
[[widgets]]
type = "effects"
name = "ranger_buffs"
category = "Buffs"
filter = ["Camouflage", "Nature's Touch", "Resist Nature", "Tangle Weed"]
x = 85
y = 0
width = 35
height = 10
```

### Wizard Buffs

```toml
[[widgets]]
type = "effects"
name = "wizard_buffs"
category = "Buffs"
filter = ["Elemental Defense", "Elemental Targeting", "Haste", "Familiar"]
x = 85
y = 0
width = 35
height = 10
```

### Cleric Buffs

```toml
[[widgets]]
type = "effects"
name = "cleric_buffs"
category = "Buffs"
filter = ["Spirit Shield", "Spirit Warding", "Benediction", "Prayer"]
x = 85
y = 0
width = 35
height = 10
```

## Compact Mode

For smaller displays:

```toml
[[widgets]]
type = "effects"
name = "buffs_compact"
category = "Buffs"
x = 85
y = 0
width = 25
height = 15
show_timers = true
compact = true           # Single line per buff
show_progress = false    # Hide progress bars, just show time
abbreviate_names = true  # "Sigil of Def" instead of full name
```

## Tips

1. **Sort by time** to see what's expiring soon
2. **Use filters** to show only profession-relevant buffs
3. **Add sound alerts** for critical buff expirations
4. **Compact mode** works well for limited screen space

## XML Reference

The game sends buff data like this:

```xml
<dialogData id='Buffs' clear='t'></dialogData>
<dialogData id='Buffs'>
  <progressBar id='115' value='89' text="Fasthr's Reward"
    left='22%' top='0' width='76%' height='15' time='03:44:32'/>
  <label id='l115' value='3:44 ' top='0' left='0' justify='2'/>
  <!-- more buffs... -->
</dialogData>
```

VellumFE parses this into `ActiveEffect` elements with:
- `id` - Spell/effect identifier
- `text` - Display name
- `value` - Progress percentage (0-100)
- `time` - Time remaining string

## See Also

- [Active Effects Widget](../widgets/active-effects.md)
- [Combat Alerts](./combat-alerts.md)
- [Hunting HUD](./hunting-hud.md)
