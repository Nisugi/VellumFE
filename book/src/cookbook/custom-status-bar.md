# Custom Status Bar

Create a personalized status display with exactly the information you need.

## Goal

Build a compact status bar showing vitals, timers, and status in a format you design.

## Basic Status Bar

Single-line status at bottom of screen:

```toml
[[widgets]]
type = "dashboard"
name = "statusbar"
x = 0
y = 98
width = 100
height = 2
border = false
layout = "horizontal"
components = ["health", "mana", "stamina", "spirit", "rt", "stance"]
spacing = 2

[widgets.statusbar.health]
type = "text"
format = "HP:{vitals.health}%"
color_condition = [
    { if = "value >= 70", color = "green" },
    { if = "value >= 30", color = "yellow" },
    { if = "value < 30", color = "red" }
]

[widgets.statusbar.mana]
type = "text"
format = "MP:{vitals.mana}%"
color = "blue"

[widgets.statusbar.stamina]
type = "text"
format = "ST:{vitals.stamina}%"
color = "orange"

[widgets.statusbar.spirit]
type = "text"
format = "SP:{vitals.spirit}%"
color = "magenta"

[widgets.statusbar.rt]
type = "text"
format = "RT:{roundtime}"
visible_when = "roundtime > 0"
color = "cyan"

[widgets.statusbar.stance]
type = "text"
format = "[{stance}]"
color = "white"
```

## Mini Vitals Bar

Compact progress bars:

```toml
[[widgets]]
type = "dashboard"
name = "mini_vitals"
x = 0
y = 0
width = 60
height = 1
border = false
layout = "horizontal"

[widgets.mini_vitals.components]
hp = { type = "progress", width = 12, data = "vitals.health", format = "♥{value}" }
mp = { type = "progress", width = 12, data = "vitals.mana", format = "♦{value}" }
st = { type = "progress", width = 12, data = "vitals.stamina", format = "⚡{value}" }
sp = { type = "progress", width = 12, data = "vitals.spirit", format = "✧{value}" }
```

## Icon Status Bar

Using Unicode symbols:

```toml
[[widgets]]
type = "dashboard"
name = "icon_status"
x = 0
y = 99
width = 100
height = 1
border = false

# Format: ❤95 ♦100 ⚡87 ✧100 ⏱3 🛡Off 👁Hid
components = [
    { icon = "❤", data = "vitals.health", color = "health" },
    { icon = "♦", data = "vitals.mana", color = "mana" },
    { icon = "⚡", data = "vitals.stamina", color = "stamina" },
    { icon = "✧", data = "vitals.spirit", color = "spirit" },
    { icon = "⏱", data = "roundtime", visible_when = "> 0" },
    { icon = "🛡", data = "stance", format = "{short}" },
    { icon = "👁", data = "hidden", format = "{status}" }
]
```

## Two-Line Status

More detailed status bar:

```toml
[[widgets]]
type = "dashboard"
name = "status_two_line"
x = 0
y = 97
width = 100
height = 3

[widgets.status_two_line.line1]
layout = "horizontal"
items = [
    "HP: {vitals.health}%",
    "MP: {vitals.mana}%",
    "ST: {vitals.stamina}%",
    "SP: {vitals.spirit}%"
]

[widgets.status_two_line.line2]
layout = "horizontal"
items = [
    "RT: {roundtime}s",
    "CT: {casttime}s",
    "Stance: {stance}",
    "Room: {room.name}"
]
```

## Status with Indicators

Include status flags:

```toml
[[widgets]]
type = "dashboard"
name = "full_status"
x = 0
y = 95
width = 100
height = 5

[widgets.full_status.sections]

vitals = { row = 1, content = "HP:{health}% MP:{mana}% ST:{stamina}% SP:{spirit}%" }

timers = { row = 2, content = "RT:{rt} CT:{ct}" }

flags = {
    row = 3,
    type = "indicators",
    items = ["hidden", "invisible", "stunned", "webbed", "prone"],
    style = "compact"
}

location = { row = 4, content = "[{room.name}]", align = "center" }
```

## Minimal Status

Just the essentials:

```toml
[[widgets]]
type = "text"
name = "minimal_status"
x = 0
y = 99
width = 100
height = 1
border = false
content = "{health}❤ {mana}♦ {rt}⏱ {room.short_name}"
```

## Contextual Status

Shows different info based on situation:

```toml
[[widgets]]
type = "dashboard"
name = "context_status"

[widgets.context_status.modes]

# Default mode
default = "HP:{health} MP:{mana} [{room.name}]"

# Combat mode (when in combat)
combat = "HP:{health} MP:{mana} RT:{rt} Target:{target}"

# Town mode (when in town)
town = "Silver:{silver} [{room.name}]"

[widgets.context_status.mode_triggers]
combat = "roundtime > 0 OR target != ''"
town = "room.type == 'town'"
```

## Color Themes

### Light Theme Status

```toml
[widgets.statusbar.theme]
background = "#e0e0e0"
text = "#333333"
health = "#00aa00"
health_low = "#aaaa00"
health_critical = "#aa0000"
mana = "#0000aa"
```

### Dark Theme Status

```toml
[widgets.statusbar.theme]
background = "#1a1a1a"
text = "#c0c0c0"
health = "#00ff00"
health_low = "#ffff00"
health_critical = "#ff0000"
mana = "#0080ff"
```

## Dynamic Updates

### Flashing on Change

```toml
[widgets.statusbar.health]
flash_on_decrease = true
flash_duration = 500
flash_color = "red"
```

### Smooth Transitions

```toml
[widgets.statusbar.health]
animate_changes = true
animation_duration = 200
```

## Tips

1. **Keep It Compact**: Status bar should be quick to read
2. **Prioritize Information**: Most important data first/largest
3. **Use Color Wisely**: Colors should convey meaning
4. **Test Readability**: Ensure text is readable at a glance

## Complete Example

```toml
# A complete status bar setup

[[widgets]]
type = "dashboard"
name = "main_status"
x = 0
y = 98
width = 100
height = 2
border = false
background = "#1a1a1a"

[widgets.main_status.row1]
items = [
    { text = "❤ ", color = "red" },
    { data = "vitals.health", width = 3, align = "right" },
    { text = "% " },
    { text = "♦ ", color = "blue" },
    { data = "vitals.mana", width = 3, align = "right" },
    { text = "% " },
    { text = "⚡ ", color = "orange" },
    { data = "vitals.stamina", width = 3, align = "right" },
    { text = "% " },
    { text = "│ ", color = "gray" },
    { text = "RT:", visible_when = "roundtime > 0" },
    { data = "roundtime", visible_when = "roundtime > 0" },
    { text = " │ ", color = "gray" },
    { indicators = ["hidden", "stunned", "prone"], style = "icons" }
]
```

## See Also

- [Dashboard Widget](../widgets/dashboard.md)
- [Progress Bars](../widgets/progress-bars.md)
- [Creating Layouts](../customization/creating-layouts.md)

