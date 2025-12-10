# Combat Alerts

Set up visual and audio notifications for combat events.

## Goal

Never miss critical combat information with customized alerts for stuns, low health, deaths, and other important events.

## Visual Alerts

### Critical Highlight Pattern

```toml
# highlights.toml

# Stunned - high visibility
[[highlights]]
pattern = "(?i)you are stunned"
fg = "black"
bg = "yellow"
bold = true
flash = true

# Webbed
[[highlights]]
pattern = "(?i)webs? (stick|entangle|ensnare)"
fg = "black"
bg = "magenta"
bold = true

# Low health warning
[[highlights]]
pattern = "feel your life fading"
fg = "white"
bg = "red"
bold = true
flash = true

# Enemy death
[[highlights]]
pattern = "falls dead"
fg = "bright_yellow"
bold = true

# Critical hit received
[[highlights]]
pattern = "\\*\\* .+ \\*\\*"
fg = "bright_red"
bold = true
```

## Audio Alerts

### Sound Triggers

```toml
# triggers.toml

# Stun alert
[[triggers]]
name = "stun_sound"
pattern = "(?i)you are stunned"
sound = "alert_high.wav"
cooldown = 500

# Low health alert
[[triggers]]
name = "health_critical"
pattern = "feel your life fading|growing weak|death is near"
sound = "alarm.wav"
cooldown = 2000

# Enemy dies
[[triggers]]
name = "kill_sound"
pattern = "falls dead|crumples to the ground"
sound = "success.wav"
cooldown = 100

# Webbed
[[triggers]]
name = "web_sound"
pattern = "(?i)webs? (stick|entangle)"
sound = "warning.wav"
```

### TTS Alerts

```toml
# triggers.toml

[[triggers]]
name = "stun_tts"
pattern = "(?i)you are stunned"
tts = "Stunned!"
tts_priority = "high"

[[triggers]]
name = "health_tts"
pattern = "feel your life fading"
tts = "Health critical!"
tts_priority = "urgent"
```

## Status Widget Alerts

### Flashing Indicators

```toml
# layout.toml

[[widgets]]
type = "indicator"
name = "combat_status"
x = 0
y = 85
width = 30
height = 3
indicators = ["stunned", "webbed", "prone", "bleeding"]

# Indicator styles
[widgets.combat_status.styles]
stunned = { fg = "black", bg = "yellow", flash = true }
webbed = { fg = "black", bg = "magenta", flash = true }
prone = { fg = "white", bg = "red" }
bleeding = { fg = "red", bg = "black", flash = true }
```

### Health Bar Alerts

```toml
[[widgets]]
type = "progress"
name = "health"
data_source = "vitals.health"
# Color changes at thresholds
color_thresholds = [
    { value = 100, color = "health" },
    { value = 50, color = "health_low" },
    { value = 25, color = "health_critical", flash = true }
]
```

## Combat Dashboard

Combined alert display:

```toml
[[widgets]]
type = "dashboard"
name = "combat_hud"
x = 75
y = 0
width = 25
height = 20
title = "Combat"

[widgets.combat_hud.components]
health = { type = "progress", data = "vitals.health" }
mana = { type = "progress", data = "vitals.mana" }
rt = { type = "countdown", data = "roundtime" }
status = { type = "indicator", items = ["stunned", "webbed", "prone"] }
target = { type = "text", data = "combat.target" }
```

## Notification System

### Popup Notifications

```toml
[[triggers]]
name = "death_notify"
pattern = "seems to have died|falls dead"
command = ".notify Enemy Killed!"

[[triggers]]
name = "stun_notify"
pattern = "(?i)you are stunned"
command = ".notify STUNNED!"
notification_style = "urgent"
```

### Screen Flash

```toml
[[triggers]]
name = "critical_flash"
pattern = "feel your life fading"
action = "flash_screen"
flash_color = "red"
flash_duration = 200
```

## Complete Combat Setup

### highlights.toml

```toml
# Combat highlights

# Status effects
[[highlights]]
pattern = "(?i)you are stunned"
fg = "black"
bg = "yellow"
bold = true

[[highlights]]
pattern = "(?i)webs? (stick|entangle|ensnare)"
fg = "black"
bg = "magenta"
bold = true

[[highlights]]
pattern = "(?i)(knocked|fall) (down|prone|to the ground)"
fg = "black"
bg = "cyan"
bold = true

# Damage
[[highlights]]
pattern = "\\*\\* .+ \\*\\*"
fg = "bright_red"
bold = true

[[highlights]]
pattern = "\\d+ points? of damage"
fg = "red"

# Success
[[highlights]]
pattern = "falls dead|crumples"
fg = "bright_yellow"
bold = true

[[highlights]]
pattern = "(strike|hit|slash|stab).*(head|neck|chest)"
fg = "bright_green"
```

### triggers.toml

```toml
# Combat triggers

[[triggers]]
name = "stun_alert"
pattern = "(?i)you are stunned"
sound = "stun.wav"
tts = "Stunned"
cooldown = 500
enabled = true

[[triggers]]
name = "web_alert"
pattern = "(?i)webs? (stick|entangle)"
sound = "web.wav"
cooldown = 500

[[triggers]]
name = "health_warning"
pattern = "feel your life fading|death is near"
sound = "alarm.wav"
tts = "Health critical"
tts_priority = "urgent"
cooldown = 3000

[[triggers]]
name = "kill_confirm"
pattern = "falls dead|crumples"
sound = "kill.wav"
cooldown = 100
```

## Customizing Sounds

### Sound File Locations

```
~/.vellum-fe/sounds/
├── alert_high.wav
├── alarm.wav
├── success.wav
├── warning.wav
├── stun.wav
├── web.wav
└── kill.wav
```

### Sound Settings

```toml
# config.toml
[sound]
enabled = true
volume = 0.7
alert_volume = 1.0  # Louder for alerts
```

## Tips

1. **Prioritize Alerts**: Not everything needs sound
2. **Use Cooldowns**: Prevent alert spam
3. **Test Patterns**: Verify regex matches correctly
4. **Balance Volume**: Alerts should be noticeable but not jarring

## See Also

- [Highlights](../configuration/highlights-toml.md)
- [Triggers](../automation/triggers.md)
- [Text-to-Speech](../customization/tts-setup.md)

