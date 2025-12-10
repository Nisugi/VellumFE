# Countdowns

Countdown widgets display roundtime and cast time remaining.

## Overview

Countdown widgets:
- Show remaining seconds for roundtime or cast time
- Update every second automatically
- Support visual bar and numeric display
- Customize icons and colors

## Configuration

```toml
[[windows]]
name = "roundtime"
type = "countdown"

# Position and size
row = 5
col = 80
width = 15
height = 1

# Countdown-specific options
countdown_type = "roundtime"  # "roundtime" or "casttime"
show_bar = true               # Show progress bar
show_seconds = true           # Show numeric seconds
icon = "⏱"                    # Icon prefix

# Colors
bar_color = "#FF0000"         # Progress bar color
text_color = "#FFFFFF"        # Seconds text color
background_color = "#000000"
```

## Properties

### countdown_type (required)

Which countdown to display:

| Type | Description |
|------|-------------|
| `roundtime` | Combat/action roundtime |
| `casttime` | Spell casting time |

### show_bar

Display a progress bar:

```toml
show_bar = true       # Show visual bar (default)
show_bar = false      # Numbers only
```

### show_seconds

Display numeric seconds remaining:

```toml
show_seconds = true   # Show "5s" (default)
show_seconds = false  # Bar only
```

### icon

Icon displayed before countdown:

```toml
icon = "⏱"        # Timer emoji
icon = "RT:"       # Text prefix
icon = ""          # No icon
```

## Display Modes

### Bar with Seconds

```
⏱ ██████░░░░ 6s
```

Shows visual progress and numeric value.

### Bar Only

```
██████████░░░░░░░░░░
```

Clean visual indicator.

### Seconds Only

```
RT: 6
```

Minimal numeric display.

### Compact

```toml
width = 8
height = 1
show_bar = false
icon = ""
```

Result: `6s`

## Visual Behavior

### Active Countdown

When time is remaining:
- Bar fills proportionally
- Seconds count down
- Color indicates urgency

### Zero/Expired

When countdown reaches zero:
- Bar empty or hidden
- Shows "0" or disappears
- Can trigger alert

### Color Progression

Optional color changes based on time:

```toml
# High time (>5s)
high_color = "#00FF00"

# Medium time (2-5s)
medium_color = "#FFFF00"

# Low time (<2s)
low_color = "#FF0000"
```

## Examples

### Standard Roundtime

```toml
[[windows]]
name = "roundtime"
type = "countdown"
countdown_type = "roundtime"
row = 5
col = 80
width = 20
height = 1
icon = "RT:"
show_bar = true
show_seconds = true
bar_color = "#FF4444"
```

### Cast Time

```toml
[[windows]]
name = "casttime"
type = "countdown"
countdown_type = "casttime"
row = 6
col = 80
width = 20
height = 1
icon = "CT:"
show_bar = true
show_seconds = true
bar_color = "#4444FF"
```

### Minimal RT

```toml
[[windows]]
name = "rt"
type = "countdown"
countdown_type = "roundtime"
row = 0
col = 0
width = 5
height = 1
icon = ""
show_bar = false
show_border = false
text_color = "#FF0000"
```

### Dual Countdowns

```toml
# Side by side
[[windows]]
name = "roundtime"
type = "countdown"
countdown_type = "roundtime"
row = 10
col = 80
width = 15
height = 1
icon = "RT"
bar_color = "#FF0000"

[[windows]]
name = "casttime"
type = "countdown"
countdown_type = "casttime"
row = 10
col = 96
width = 15
height = 1
icon = "CT"
bar_color = "#0088FF"
```

### Progress Bar Style

```toml
[[windows]]
name = "roundtime"
type = "countdown"
countdown_type = "roundtime"
row = 15
col = 80
width = 30
height = 1
show_seconds = false
show_border = false
bar_color = "#FF6600"
transparent_background = true
```

## Data Source

Countdown widgets receive data from XML elements:

```xml
<roundTime value='1234567890'/>
<castTime value='1234567895'/>
```

The `value` is a Unix timestamp when the countdown ends. VellumFE calculates remaining seconds.

## Accuracy

Countdowns update based on:
- Game server timestamps
- Local system clock
- ~1 second precision

Minor drift from game may occur due to network latency.

## Alerts

Configure alerts when countdown ends:

```toml
# In config.toml
[alerts]
roundtime_end_sound = "sounds/ding.wav"
roundtime_end_flash = true
```

## Troubleshooting

### Countdown not appearing

1. Check `countdown_type` is correct
2. Verify game is sending countdown data
3. Actions must trigger roundtime

### Time seems wrong

1. Check system clock accuracy
2. Network latency can cause slight drift
3. Verify countdown is for expected action

### Bar not showing

1. Check `show_bar = true`
2. Verify `width` is sufficient
3. Check `bar_color` is visible

## See Also

- [Progress Bars](./progress-bars.md) - Static progress display
- [Indicators](./indicators.md) - Status indicators
- [Sound Alerts](../customization/sound-alerts.md) - Alert configuration
