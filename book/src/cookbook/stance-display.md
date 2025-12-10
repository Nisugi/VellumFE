# Stance Display

Show your current combat stance with quick-switch controls.

## Goal

Display current stance (offensive/defensive) with visual indicator and easy keybind switching.

## The Data

GemStone IV sends stance data via multiple XML elements:

```xml
<dialogData id='stance'>
  <progressBar id='pbarStance' value='100' text='defensive (100%)' .../>
</dialogData>

<dialogData id='combat'>
  <dropDownBox id='dDBStance' value="defensive"
    cmd='_stance %dDBStance%'
    content_text='offensive,advance,forward,neutral,guarded,defensive' .../>
</dialogData>
```

## Layout Options

### Option 1: Progress Bar

```toml
[[widgets]]
type = "progress"
name = "stance"
data_source = "stance"
x = 85
y = 5
width = 25
height = 1
show_text = true
color_dynamic = true  # Color based on stance type
```

### Option 2: Text Indicator

```toml
[[widgets]]
type = "indicator"
name = "stance_indicator"
x = 85
y = 5
width = 15
height = 1
indicators = ["offensive", "defensive", "neutral"]
style = "text"
```

### Option 3: Combined with Vitals

```toml
[[widgets]]
type = "dashboard"
name = "combat_status"
x = 85
y = 0
width = 35
height = 8
title = "Combat"

[widgets.combat_status.components]
health = { type = "progress", data = "vitals.health", row = 0 }
mana = { type = "progress", data = "vitals.mana", row = 1 }
stance = { type = "progress", data = "stance", row = 2 }
rt = { type = "countdown", data = "roundtime", row = 3 }
```

## Stance Colors

```toml
# colors.toml

# Stance-specific colors
stance_offensive = "#FF0000"    # Red - aggressive
stance_advance = "#FF6600"      # Orange
stance_forward = "#FFAA00"      # Yellow-orange
stance_neutral = "#FFFF00"      # Yellow
stance_guarded = "#00AAFF"      # Light blue
stance_defensive = "#00FF00"    # Green - safe
```

## Quick Stance Keybinds

```toml
# keybinds.toml

# F1-F6 for stance ladder
[[keybinds]]
key = "F1"
action = "send"
command = "stance offensive"

[[keybinds]]
key = "F2"
action = "send"
command = "stance advance"

[[keybinds]]
key = "F3"
action = "send"
command = "stance forward"

[[keybinds]]
key = "F4"
action = "send"
command = "stance neutral"

[[keybinds]]
key = "F5"
action = "send"
command = "stance guarded"

[[keybinds]]
key = "F6"
action = "send"
command = "stance defensive"

# Quick toggles
[[keybinds]]
key = "Ctrl+O"
action = "send"
command = "stance offensive"

[[keybinds]]
key = "Ctrl+D"
action = "send"
command = "stance defensive"
```

## Stance Highlights

```toml
# highlights.toml

# Stance change confirmations
[[highlights]]
pattern = "You are now in an? (offensive|advance) stance"
fg = "red"
bold = true

[[highlights]]
pattern = "You are now in a (forward|neutral) stance"
fg = "yellow"

[[highlights]]
pattern = "You are now in a (guarded|defensive) stance"
fg = "green"
bold = true
```

## Stance Ladder Reference

| Stance | DS% | AS Penalty | Best For |
|--------|-----|------------|----------|
| Offensive | 0% | None | Max damage |
| Advance | 20% | -5 | Aggressive |
| Forward | 40% | -10 | Balanced attack |
| Neutral | 60% | -15 | Balanced |
| Guarded | 80% | -20 | Cautious |
| Defensive | 100% | -25 | Max defense |

## Combat Triggers

```toml
# triggers.toml

# Auto-defensive on stun
[[triggers]]
name = "stun_defensive"
pattern = "(?i)you are stunned"
command = "stance defensive"
enabled = false  # Enable if you want auto-stance

# Stance reminder when attacking
[[triggers]]
name = "offensive_reminder"
pattern = "Roundtime: \\d+ sec"
condition = "stance != offensive"
tts = "Not in offensive stance"
enabled = false
```

## Tips

1. **Color code** your stance for instant recognition
2. **Keybind frequently used stances** - F1/F6 for offensive/defensive is common
3. **Consider auto-stance** triggers for safety (controversial - some consider it automation)
4. **Position near RT** - Stance and roundtime are often checked together

## See Also

- [Hunting HUD](./hunting-hud.md)
- [Combat Alerts](./combat-alerts.md)
- [Keybind Actions](../customization/keybind-actions.md)
