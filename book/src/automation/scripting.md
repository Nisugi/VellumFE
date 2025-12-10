# Scripting (Future)

> **Note**: Advanced scripting is a planned feature. This document describes the intended design.

## Overview

VellumFE will support a scripting API for complex automation beyond simple triggers and macros.

## Planned Features

### Event-Based Scripts

```lua
-- React to game events
on_event("stun", function(duration)
    notify("Stunned for " .. duration .. " seconds")
    if health() < 50 then
        send("flee")
    end
end)

on_event("death", function()
    notify("You have died!")
    -- Auto-release logic
end)
```

### State Access

```lua
-- Access game state
if health() < 30 then
    send("hide")
end

if mana() > 50 and not has_spell("Spirit Shield") then
    send("prep 107;cast")
end
```

### Timers

```lua
-- Delayed actions
after(5000, function()
    send("look")
end)

-- Repeating actions
every(60000, function()
    if not hidden() then
        send("perception")
    end
end)
```

### Variables

```lua
-- Persistent variables
set_var("kill_count", get_var("kill_count", 0) + 1)

if get_var("kill_count") >= 100 then
    notify("100 kills reached!")
end
```

## Language Options

### Lua

Lightweight, embeddable, well-suited for game scripting:

```lua
function on_combat_start(enemy)
    if enemy.name:match("dragon") then
        send("flee")
    else
        send("attack " .. enemy.noun)
    end
end
```

### Rhai

Rust-native scripting language:

```rhai
fn on_combat_start(enemy) {
    if enemy.name.contains("dragon") {
        send("flee");
    } else {
        send("attack " + enemy.noun);
    }
}
```

## Planned API

### Core Functions

| Function | Description |
|----------|-------------|
| `send(cmd)` | Send command to game |
| `notify(msg)` | Show notification |
| `log(msg)` | Write to log |
| `sleep(ms)` | Pause execution |

### State Functions

| Function | Description |
|----------|-------------|
| `health()` | Current health |
| `max_health()` | Maximum health |
| `mana()` | Current mana |
| `stamina()` | Current stamina |
| `spirit()` | Current spirit |

### Status Functions

| Function | Description |
|----------|-------------|
| `stunned()` | Is character stunned? |
| `hidden()` | Is character hidden? |
| `prone()` | Is character prone? |
| `roundtime()` | Current roundtime |

### Room Functions

| Function | Description |
|----------|-------------|
| `room_name()` | Current room name |
| `room_id()` | Current room ID |
| `exits()` | Available exits |
| `creatures()` | Creatures in room |

### Spell Functions

| Function | Description |
|----------|-------------|
| `has_spell(name)` | Check active spell |
| `spell_time(name)` | Time remaining |
| `prepared_spell()` | Currently prepared |

## Example Scripts

### Auto-Hide When Wounded

```lua
-- Auto-hide when health drops
on_event("health_change", function(current, max)
    local percent = (current / max) * 100
    if percent < 30 and not hidden() and not stunned() then
        send("hide")
    end
end)
```

### Spell Manager

```lua
-- Keep defensive spells up
local defensive_spells = {
    {spell = 107, name = "Spirit Shield"},
    {spell = 103, name = "Spirit Defense"},
}

every(10000, function()
    for _, s in ipairs(defensive_spells) do
        if not has_spell(s.name) and mana() > 20 then
            wait_for_rt()
            send("incant " .. s.spell)
            return  -- One spell at a time
        end
    end
end)
```

### Combat Assistant

```lua
-- Simple combat script
local target = nil

on_event("creature_enter", function(creature)
    if target == nil then
        target = creature
        notify("Targeting: " .. creature.name)
    end
end)

on_event("creature_death", function(creature)
    if target and target.id == creature.id then
        send("search")
        target = nil
    end
end)

on_key("f1", function()
    if target then
        wait_for_rt()
        send("attack " .. target.noun)
    else
        notify("No target")
    end
end)
```

### Herb Monitor

```lua
-- Track herb usage
local herbs_used = {
    acantha = 0,
    basal = 0,
    cactacae = 0,
}

on_pattern("eat.*acantha", function()
    herbs_used.acantha = herbs_used.acantha + 1
    if herbs_used.acantha % 10 == 0 then
        notify("Used " .. herbs_used.acantha .. " acantha")
    end
end)
```

## Script Management

### Loading Scripts

```
.script load my_script.lua
.script reload my_script
.script unload my_script
```

### Listing Scripts

```
.script list
```

### Script Directory

```
~/.vellum-fe/scripts/
├── combat.lua
├── spells.lua
├── utils.lua
└── character/
    └── my_char.lua
```

## Safety Features

### Sandboxing

Scripts will be sandboxed:
- No file system access
- No network access
- No shell commands
- Limited API surface

### Rate Limiting

```lua
-- Commands are rate-limited
send("attack")  -- Works
send("attack")  -- Queued
send("attack")  -- Queued
-- Actual sends spaced out
```

### Kill Switch

```
.script stop     # Stop all scripts
.script pause    # Pause execution
.script resume   # Resume execution
```

## Current Alternatives

Until scripting is implemented, use:

1. **Triggers** - Pattern-based automation
2. **Macros** - Key-bound command sequences
3. **Cmdlists** - Context menu commands
4. **External scripts** - Lich scripts (via proxy)

## Contributing

Interested in scripting implementation? Check:
- [Development Guide](../development/README.md)
- [Contributing Guide](../development/contributing.md)
- GitHub Issues for scripting discussion

## See Also

- [Triggers](./triggers.md) - Current pattern automation
- [Macros](./macros.md) - Keybind commands
- [Architecture](../architecture/README.md) - System design

