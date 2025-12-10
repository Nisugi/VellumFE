# Widget Quick Reference

## Text Window

Displays scrollable game text.

```toml
[[widgets]]
type = "text"
name = "main"
stream = "main"
x = 0
y = 0
width = 80
height = 30
buffer_size = 2000
show_border = true
title = "Game"
```

## Tabbed Text Window

Multiple streams in tabs.

```toml
[[widgets]]
type = "tabbed_text"
name = "channels"
x = 0
y = 0
width = 80
height = 30
tabs = [
    { name = "Main", stream = "main" },
    { name = "Speech", stream = "speech" },
    { name = "Thoughts", stream = "thoughts" }
]
```

## Command Input

Text entry for commands.

```toml
[[widgets]]
type = "input"
name = "command"
x = 0
y = 35
width = 100
height = 3
history_size = 100
prompt = "> "
```

## Progress Bar

Health, mana, etc.

```toml
[[widgets]]
type = "progress"
name = "health"
data_source = "vitals.health"
x = 70
y = 0
width = 30
height = 1
color = "health"
show_text = true
```

**Data Sources:**
- `vitals.health` - Health
- `vitals.mana` - Mana
- `vitals.stamina` - Stamina
- `vitals.spirit` - Spirit
- `encumbrance` - Load

## Countdown Timer

Roundtime/casttime display.

```toml
[[widgets]]
type = "countdown"
name = "roundtime"
countdown_id = "roundtime"
x = 70
y = 5
width = 15
height = 1
color = "yellow"
```

**IDs:**
- `roundtime` - Attack roundtime
- `casttime` - Spell casttime

## Compass

Direction display.

```toml
[[widgets]]
type = "compass"
name = "compass"
x = 85
y = 0
width = 15
height = 5
style = "graphical"  # or "text"
```

**Styles:**
```
Graphical:       Text:
    N            N NE E
  W + E          SE S SW
    S            W NW
```

## Hands Display

Equipment in hands.

```toml
[[widgets]]
type = "hands"
name = "hands"
x = 70
y = 10
width = 30
height = 3
show_spell = true
```

## Status Indicators

Status icons (hidden, stunned, etc.)

```toml
[[widgets]]
type = "indicator"
name = "status"
x = 70
y = 15
width = 30
height = 2
indicators = ["hidden", "stunned", "webbed", "prone", "kneeling"]
compact = true
```

**Available Indicators:**
- `standing`, `sitting`, `kneeling`, `prone`
- `hidden`, `invisible`
- `stunned`, `webbed`, `dead`
- `joined`, `grouped`

## Injury Display

Body injury diagram.

```toml
[[widgets]]
type = "injury"
name = "injuries"
x = 85
y = 10
width = 15
height = 10
style = "doll"  # or "list"
```

## Active Effects

Buffs, debuffs, cooldowns.

```toml
[[widgets]]
type = "effects"
name = "buffs"
category = "Buffs"
x = 70
y = 20
width = 30
height = 10
show_timers = true
```

**Categories:**
- `Buffs` - Beneficial effects
- `Debuffs` - Negative effects
- `Cooldowns` - Ability cooldowns
- `ActiveSpells` - Active spells

## Room Window

Room description display.

```toml
[[widgets]]
type = "room"
name = "room"
x = 0
y = 0
width = 70
height = 10
show_exits = true
show_players = true
show_objects = true
```

## Dashboard

Combined widget display.

```toml
[[widgets]]
type = "dashboard"
name = "combat_hud"
x = 70
y = 0
width = 30
height = 20
title = "Combat"

[widgets.combat_hud.components]
health = { type = "progress", data = "vitals.health" }
mana = { type = "progress", data = "vitals.mana" }
rt = { type = "countdown", data = "roundtime" }
status = { type = "indicator", items = ["stunned", "webbed"] }
```

## Performance Monitor

Debug/performance info.

```toml
[[widgets]]
type = "performance"
name = "perf"
x = 0
y = 37
width = 50
height = 3
show_fps = true
show_latency = true
```

## Common Properties

All widgets support:

| Property | Type | Description |
|----------|------|-------------|
| `type` | string | Widget type (required) |
| `name` | string | Unique name (required) |
| `x` | int | Column position |
| `y` | int | Row position |
| `width` | int | Width in columns |
| `height` | int | Height in rows |
| `show_border` | bool | Show border |
| `border_style` | string | `single`, `double`, `rounded` |
| `title` | string | Window title |
| `visible` | bool | Initially visible |
| `focusable` | bool | Can receive focus |
