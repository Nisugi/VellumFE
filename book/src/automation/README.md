# Automation Overview

VellumFE provides automation features to streamline repetitive tasks and enhance gameplay efficiency.

## Automation Features

| Feature | Description | Use Case |
|---------|-------------|----------|
| **Cmdlists** | Context menu commands | Right-click actions |
| **Macros** | Multi-command sequences | Complex actions |
| **Triggers** | Event-based responses | Automatic reactions |
| **Scripting** | Advanced automation | Custom logic |

## Quick Start

### Simple Macro

```toml
# In keybinds.toml
[keybinds."f1"]
macro = "attack target"

[keybinds."f2"]
macro = "stance defensive;hide"
```

### Context Menu (Cmdlist)

```toml
# In cmdlist.toml
[[cmdlist]]
noun = "sword"
commands = ["look", "get", "drop", "put in backpack"]
```

### Trigger

```toml
# In triggers.toml
[[triggers]]
pattern = "You are stunned"
command = ".say Stunned!"
```

## Automation Philosophy

### Enhancement, Not Replacement

VellumFE automation is designed to:
- **Reduce tedium** - Automate repetitive tasks
- **Speed up common actions** - Quick access to frequent commands
- **Provide alerts** - Notify of important events

### Not Designed For

- Fully automated hunting
- AFK gameplay
- Botting

## Automation Levels

### Level 1: Keybind Macros

Simple command sequences bound to keys:

```toml
[keybinds."ctrl+1"]
macro = "prep 101;cast"
```

**Best for:** Frequent, simple commands

### Level 2: Context Menus

Right-click commands for objects:

```toml
[[cmdlist]]
noun = "creature"
commands = ["attack", "look", "assess"]
```

**Best for:** Object-specific actions

### Level 3: Triggers

Automatic responses to game events:

```toml
[[triggers]]
pattern = "roundtime"
command = ".notify"
```

**Best for:** Status monitoring, alerts

### Level 4: Scripts (Future)

Complex conditional logic:

```lua
-- Future scripting API
on_event("stun", function()
    if health() < 50 then
        send("flee")
    end
end)
```

**Best for:** Complex decision-making

## Safety Guidelines

### Avoid Detection Issues

- Don't automate too quickly
- Add reasonable delays
- Keep human-like timing
- Don't automate core gameplay

### Respect Game Rules

Check game policies on automation:
- Some automation is allowed
- Unattended automation may not be
- When in doubt, don't automate

### Test Safely

Test automation in safe areas:
- Town squares
- Non-combat zones
- With backup plans

## Automation Guides

This section covers:

- [Cmdlists](./cmdlists.md) - Context menu system
- [Macros](./macros.md) - Multi-command keybinds
- [Triggers](./triggers.md) - Event-based automation
- [Scripting](./scripting.md) - Advanced automation (future)

## Common Automation Tasks

### Combat Efficiency

- Quick spell casting (macros)
- Target selection (cmdlist)
- Status alerts (triggers)

### Inventory Management

- Quick get/drop commands
- Container management
- Sorting actions

### Navigation

- Direction macros
- Room scanning
- Exit shortcuts

### Social

- Quick emotes
- Response macros
- Greeting templates

## See Also

- [Keybind Actions](../customization/keybind-actions.md) - All keybind options
- [Highlight Patterns](../customization/highlight-patterns.md) - Pattern matching
- [Sound Alerts](../customization/sound-alerts.md) - Audio notifications

