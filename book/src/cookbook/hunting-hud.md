# Hunting HUD

Create an optimized heads-up display for hunting with all critical information visible at a glance.

## Goal

A compact layout showing health, mana, roundtime, target, buffs, and room info - everything a hunter needs without cluttering the main text area.

## Layout

```
┌─────────────────────────────────────────────┬──────────────────┐
│                                             │ ▓▓▓▓▓▓▓▓░░ 326   │ Health
│                                             │ ▓▓▓▓▓▓▓▓▓▓ 481   │ Mana
│                                             │ ▓▓▓▓▓▓▓▓▓░ 223   │ Stamina
│              Main Game Text                 ├──────────────────┤
│                                             │ RT: ██░░░ 3s     │
│                                             │ CT: ░░░░░ 0s     │
│                                             ├──────────────────┤
│                                             │    N             │
│                                             │  W + E           │
│                                             │    S             │
├─────────────────────────────────────────────┼──────────────────┤
│ Room: Angargreft, Pits of the Dead         │ Target: valravn  │
│ Exits: north, southeast                     │ [HIDDEN] [STANCE]│
├─────────────────────────────────────────────┴──────────────────┤
│ > _                                                            │
└────────────────────────────────────────────────────────────────┘
```

## Configuration

### layout.toml

```toml
[layout]
name = "Hunting HUD"
columns = 120
rows = 40

# Main text window - takes most of the screen
[[widgets]]
type = "text"
name = "main"
stream = "main"
x = 0
y = 0
width = 85
height = 32
buffer_size = 3000
show_border = true
title = "Game"

# Vitals panel (right side, top)
[[widgets]]
type = "progress"
name = "health"
data_source = "vitals.health"
x = 85
y = 0
width = 35
height = 1
color = "health"
show_text = true
show_border = false

[[widgets]]
type = "progress"
name = "mana"
data_source = "vitals.mana"
x = 85
y = 1
width = 35
height = 1
color = "mana"
show_text = true
show_border = false

[[widgets]]
type = "progress"
name = "stamina"
data_source = "vitals.stamina"
x = 85
y = 2
width = 35
height = 1
color = "stamina"
show_text = true
show_border = false

[[widgets]]
type = "progress"
name = "spirit"
data_source = "vitals.spirit"
x = 85
y = 3
width = 35
height = 1
color = "spirit"
show_text = true
show_border = false

# Timers
[[widgets]]
type = "countdown"
name = "roundtime"
countdown_id = "roundtime"
x = 85
y = 5
width = 35
height = 1
color = "yellow"
label = "RT"

[[widgets]]
type = "countdown"
name = "casttime"
countdown_id = "casttime"
x = 85
y = 6
width = 35
height = 1
color = "cyan"
label = "CT"

# Compass
[[widgets]]
type = "compass"
name = "compass"
x = 85
y = 8
width = 35
height = 5
style = "graphical"
show_border = true

# Status indicators
[[widgets]]
type = "indicator"
name = "status"
x = 85
y = 14
width = 35
height = 2
indicators = ["hidden", "stunned", "webbed", "prone", "kneeling", "sitting"]
compact = true

# Active buffs (scrollable)
[[widgets]]
type = "effects"
name = "buffs"
category = "Buffs"
x = 85
y = 17
width = 35
height = 15
show_timers = true
show_border = true
title = "Buffs"

# Room info bar
[[widgets]]
type = "room"
name = "room_bar"
x = 0
y = 32
width = 85
height = 3
show_exits = true
compact = true
show_border = true

# Target display
[[widgets]]
type = "text"
name = "target"
stream = "target"
x = 85
y = 32
width = 35
height = 3
show_border = true
title = "Target"

# Command input
[[widgets]]
type = "input"
name = "command"
x = 0
y = 35
width = 120
height = 3
history_size = 200
prompt = "> "
show_border = true
```

### highlights.toml (Hunting)

```toml
# Monster names (bold red)
[[highlights]]
pattern = "\\b(orc|troll|goblin|valravn|panther|drake|golem)\\b"
fg = "bright_red"
bold = true

# Your attacks hitting
[[highlights]]
pattern = "(You|Your).*(strike|hit|slash|stab|fire|cast)"
fg = "bright_green"

# Critical hits
[[highlights]]
pattern = "\\*\\* .+ \\*\\*"
fg = "bright_yellow"
bold = true

# Damage numbers
[[highlights]]
pattern = "\\d+ points? of damage"
fg = "red"

# Enemy death
[[highlights]]
pattern = "(falls dead|crumples|expires|dies)"
fg = "bright_yellow"
bold = true

# Loot
[[highlights]]
pattern = "(silver|coins|silvers|drops? a)"
fg = "bright_yellow"

# Stun warning
[[highlights]]
pattern = "(?i)you are stunned"
fg = "black"
bg = "yellow"
bold = true
flash = true

# Webbed
[[highlights]]
pattern = "(?i)webs? (stick|entangle)"
fg = "black"
bg = "magenta"
bold = true
```

### keybinds.toml (Hunting)

```toml
# Quick stance changes
[[keybinds]]
key = "F1"
action = "send"
command = "stance offensive"

[[keybinds]]
key = "F2"
action = "send"
command = "stance defensive"

# Quick hide
[[keybinds]]
key = "F3"
action = "send"
command = "hide"

# Target next
[[keybinds]]
key = "F4"
action = "send"
command = "target next"

# Attack
[[keybinds]]
key = "F5"
action = "send"
command = "attack"

# Loot all
[[keybinds]]
key = "F6"
action = "send"
command = "loot"

# Quick look
[[keybinds]]
key = "F7"
action = "send"
command = "look"

# Health check
[[keybinds]]
key = "F8"
action = "send"
command = "health"
```

## Triggers for Hunting

```toml
# triggers.toml

# Stun alert
[[triggers]]
name = "stun_alert"
pattern = "(?i)you are stunned"
sound = "stun.wav"
tts = "Stunned"
cooldown = 500

# Low health warning
[[triggers]]
name = "health_low"
pattern = "feel your life fading|death is near"
sound = "alarm.wav"
tts = "Health critical"
cooldown = 3000

# Enemy death confirmation
[[triggers]]
name = "kill_confirm"
pattern = "falls dead|crumples|expires"
sound = "kill.wav"
cooldown = 100
```

## Tips

1. **Adjust widths** based on your terminal size
2. **Add more indicators** for your profession-specific statuses
3. **Customize highlights** for creatures in your hunting area
4. **Use keybinds** for your most common combat actions

## See Also

- [Combat Alerts](./combat-alerts.md)
- [Progress Bars](../widgets/progress-bars.md)
- [Keybind Actions](../customization/keybind-actions.md)
