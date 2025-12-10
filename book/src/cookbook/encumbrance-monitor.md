# Encumbrance Monitor

Track your carrying capacity with visual feedback and alerts.

## Goal

Display your current encumbrance level with color-coded warnings as you pick up loot.

## The Data

GemStone IV sends encumbrance via `dialogData id='encum'`:

```xml
<dialogData id='encum'>
  <progressBar id='encumlevel' value='10' text='Light' .../>
  <label id='encumblurb' value='You feel confident that your load
    is not affecting your actions very much.' .../>
</dialogData>
```

## Layout Options

### Option 1: Progress Bar

```toml
[[widgets]]
type = "progress"
name = "encumbrance"
data_source = "encumbrance"
x = 0
y = 35
width = 30
height = 1
show_text = true
color_thresholds = [
    { value = 100, color = "#00FF00" },  # Light - green
    { value = 75, color = "#66FF00" },   # Moderate - yellow-green
    { value = 50, color = "#FFFF00" },   # Significant - yellow
    { value = 25, color = "#FF6600" },   # Heavy - orange
    { value = 0, color = "#FF0000" }     # Overloaded - red
]
```

### Option 2: Text Display

```toml
[[widgets]]
type = "text"
name = "encum_display"
stream = "encumbrance"
x = 0
y = 35
width = 50
height = 2
show_border = true
title = "Load"
```

### Option 3: In Status Bar

```toml
[[widgets]]
type = "dashboard"
name = "status_bar"
x = 0
y = 37
width = 120
height = 1
horizontal = true

[widgets.status_bar.components]
health = { type = "progress", data = "vitals.health", width = 20 }
mana = { type = "progress", data = "vitals.mana", width = 20 }
encum = { type = "progress", data = "encumbrance", width = 15 }
stance = { type = "text", data = "stance", width = 15 }
```

## Highlights for Encumbrance Messages

```toml
# highlights.toml

# Light load - green
[[highlights]]
pattern = "Your current load is Light"
fg = "bright_green"

# Moderate - yellow
[[highlights]]
pattern = "Your current load is (Moderate|Somewhat)"
fg = "yellow"

# Heavy - orange
[[highlights]]
pattern = "Your current load is (Significant|Heavy)"
fg = "bright_yellow"
bold = true

# Overloaded - red
[[highlights]]
pattern = "Your current load is (Very Heavy|Overloaded|Extreme)"
fg = "bright_red"
bold = true
```

## Alerts

```toml
# triggers.toml

# Getting heavy
[[triggers]]
name = "encum_heavy"
pattern = "Your load (is now|has become) (Heavy|Very Heavy)"
tts = "Load getting heavy"
sound = "warning.wav"

# Overloaded
[[triggers]]
name = "encum_overloaded"
pattern = "overloaded|encumbered"
tts = "Overloaded!"
sound = "alarm.wav"
cooldown = 5000

# Back to light
[[triggers]]
name = "encum_light"
pattern = "Your load (is now|has become) Light"
sound = "success.wav"
```

## Encumbrance Levels

| Level | Value | Effect |
|-------|-------|--------|
| Light | 0-20% | No penalty |
| Moderate | 20-40% | Minor RT increase |
| Significant | 40-60% | Noticeable RT increase |
| Heavy | 60-80% | Major penalties |
| Very Heavy | 80-100% | Severe penalties |
| Overloaded | 100%+ | Can't move |

## Merchant/Looting Setup

For heavy looting sessions:

```toml
# Dedicated encumbrance window
[[widgets]]
type = "text"
name = "loot_log"
stream = "loot"
x = 85
y = 20
width = 35
height = 10
show_border = true
title = "Loot"

# Encumbrance bar always visible
[[widgets]]
type = "progress"
name = "encum"
data_source = "encumbrance"
x = 85
y = 31
width = 35
height = 2
show_text = true
title = "Load"
```

### Loot Highlights

```toml
# Valuable loot
[[highlights]]
pattern = "(drops? a|You also see).*(gem|jewel|gold|platinum|diamond)"
fg = "bright_yellow"
bold = true

# Silver/coins
[[highlights]]
pattern = "\\d+ (silver|silvers|coins)"
fg = "yellow"

# Boxes/containers
[[highlights]]
pattern = "(strongbox|chest|coffer|box|lockbox)"
fg = "cyan"
```

## Tips

1. **Position strategically** - Put encumbrance where you'll notice it
2. **Use color coding** - Visual feedback is faster than reading text
3. **Set up alerts** - Don't get stuck overloaded in a hunting ground
4. **Track loot separately** - Dedicated loot window helps manage inventory

## See Also

- [Progress Bars](../widgets/progress-bars.md)
- [Hunting HUD](./hunting-hud.md)
- [Merchant Setup](../tutorials/merchant-setup.md)
