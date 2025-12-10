# Macros

Macros allow you to bind multiple commands to a single key press.

## Overview

Macros send game commands when you press a key:

```toml
# In keybinds.toml
[keybinds."f1"]
macro = "attack target"

[keybinds."ctrl+1"]
macro = "prep 101;cast"
```

## Basic Macros

### Single Command

```toml
[keybinds."f1"]
macro = "attack target"
```

Press F1 → Sends "attack target" to game

### Multiple Commands

Separate commands with semicolons:

```toml
[keybinds."f2"]
macro = "stance offensive;attack target"
```

Press F2 → Sends both commands in sequence

## Command Sequences

### Combat Sequence

```toml
[keybinds."f5"]
macro = "stance offensive;attack target;stance defensive"
```

### Spell Casting

```toml
[keybinds."ctrl+1"]
macro = "prep 101;cast"

[keybinds."ctrl+2"]
macro = "prep 103;cast target"

[keybinds."ctrl+3"]
macro = "incant 107"
```

### Movement

```toml
[keybinds."numpad8"]
macro = "go north"

[keybinds."numpad2"]
macro = "go south"

[keybinds."numpad4"]
macro = "go west"

[keybinds."numpad6"]
macro = "go east"

[keybinds."numpad5"]
macro = "out"
```

## Delays

Add delays between commands (in milliseconds):

```toml
[keybinds."f5"]
macro = "prep 101;{500};cast"
# Wait 500ms between prep and cast
```

### Delay Syntax

```toml
macro = "command1;{1000};command2"
# {1000} = 1 second delay
```

### Example: Careful Spellcasting

```toml
[keybinds."f6"]
macro = "prep 901;{2000};cast target"
# Prep spell, wait 2 seconds for mana, cast
```

## Variables

### Input Prompt

```toml
[keybinds."ctrl+g"]
macro = "go $input"
```

Press Ctrl+G → Prompts for direction → Sends "go <input>"

### Target Variable

```toml
[keybinds."f1"]
macro = "attack $target"
```

Uses current target if set, or prompts.

### Last Target

```toml
[keybinds."f2"]
macro = "attack $lasttarget"
```

Attacks the last targeted creature.

## Conditional Macros

### With Input Check

```toml
[keybinds."f5"]
macro = "$input:prep $input;cast"
# Only executes if input provided
```

### Multiple Inputs

```toml
[keybinds."f6"]
macro = "give $input1 to $input2"
# Prompts for two inputs
```

## Common Macro Patterns

### Quick Attacks

```toml
[keybinds."f1"]
macro = "attack target"

[keybinds."f2"]
macro = "attack left target"

[keybinds."f3"]
macro = "attack right target"

[keybinds."f4"]
macro = "feint target"
```

### Defensive Moves

```toml
[keybinds."f5"]
macro = "stance defensive"

[keybinds."f6"]
macro = "hide"

[keybinds."f7"]
macro = "evade"
```

### Stance Cycling

```toml
[keybinds."ctrl+up"]
macro = "stance offensive"

[keybinds."ctrl+down"]
macro = "stance defensive"

[keybinds."ctrl+left"]
macro = "stance neutral"
```

### Looting

```toml
[keybinds."ctrl+l"]
macro = "search;loot"

[keybinds."ctrl+shift+l"]
macro = "search;loot;skin"
```

### Information

```toml
[keybinds."i"]
macro = "inventory"

[keybinds."ctrl+i"]
macro = "inventory full"

[keybinds."ctrl+e"]
macro = "experience"

[keybinds."ctrl+w"]
macro = "wealth"
```

### Healing

```toml
# Empath healing
[keybinds."ctrl+h"]
macro = "transfer $target"

# Herb usage
[keybinds."ctrl+1"]
macro = "get acantha from my herb pouch;eat my acantha"

[keybinds."ctrl+2"]
macro = "get basal from my herb pouch;eat my basal"
```

### Social

```toml
[keybinds."ctrl+s"]
macro = "smile"

[keybinds."ctrl+b"]
macro = "bow"

[keybinds."ctrl+w"]
macro = "wave"

[keybinds."ctrl+g"]
macro = "greet $target"
```

## Macro Organization

### By Activity

```toml
# === COMBAT ===
[keybinds."f1"]
macro = "attack target"

[keybinds."f2"]
macro = "stance defensive"

# === SPELLS ===
[keybinds."ctrl+1"]
macro = "prep 101;cast"

# === MOVEMENT ===
[keybinds."numpad8"]
macro = "go north"

# === SOCIAL ===
[keybinds."ctrl+s"]
macro = "smile"
```

### By Profession

Create character-specific keybinds:

```
~/.vellum-fe/characters/Warrior/keybinds.toml
~/.vellum-fe/characters/Wizard/keybinds.toml
```

## Advanced Macros

### Complex Combat Routine

```toml
[keybinds."f10"]
macro = "stance offensive;{100};attack target;{500};stance defensive"
```

### Merchant Interaction

```toml
[keybinds."ctrl+m"]
macro = "order $input;buy"
```

### Container Management

```toml
[keybinds."ctrl+p"]
macro = "put $input in my backpack"

[keybinds."ctrl+shift+p"]
macro = "get $input from my backpack"
```

## Troubleshooting

### Macro Not Executing

1. Check key format is correct
2. Verify keybinds.toml syntax
3. Run `.reload keybinds`
4. Check for key conflicts

### Commands Out of Order

Add delays:
```toml
macro = "command1;{500};command2"
```

### Input Prompt Not Appearing

1. Check `$input` syntax
2. Verify focus is on game
3. Check for conflicting keybinds

### Multiple Commands Failing

Some games have command limits:
- Add delays between commands
- Break into separate macros
- Check game's command queue limit

## Best Practices

1. **Keep macros simple** - Complex macros are harder to debug
2. **Add delays when needed** - Prevents command flooding
3. **Test in safe areas** - Verify before combat use
4. **Document your macros** - Add comments to keybinds.toml
5. **Use consistent keys** - Group related macros logically

## See Also

- [Keybind Actions](../customization/keybind-actions.md) - All keybind options
- [Keybinds Configuration](../configuration/keybinds-toml.md) - Full reference
- [Cmdlists](./cmdlists.md) - Context menu commands

