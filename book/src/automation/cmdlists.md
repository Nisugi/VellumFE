# Command Lists (Cmdlists)

Command lists provide context menu actions for clickable objects in the game.

## Overview

When you right-click on a game object (creature, item, player), VellumFE shows a context menu with relevant commands.

```
┌─────────────────┐
│ a rusty sword   │
├─────────────────┤
│ Look            │
│ Get             │
│ Drop            │
│ Put in backpack │
│ Appraise        │
└─────────────────┘
```

## Configuration

### Basic Cmdlist

In `cmdlist.toml`:

```toml
[[cmdlist]]
noun = "sword"
commands = ["look", "get", "drop", "appraise"]
```

### Cmdlist Properties

```toml
[[cmdlist]]
noun = "creature"       # Object noun to match
commands = [            # Available commands
    "attack",
    "look",
    "assess"
]
priority = 100          # Higher = appears first
category = "combat"     # Optional grouping
```

## Noun Matching

### Exact Match

```toml
[[cmdlist]]
noun = "sword"
commands = ["look", "get"]
```

Matches: "sword", not "longsword"

### Partial Match

```toml
[[cmdlist]]
noun = "sword"
match_mode = "contains"
commands = ["look", "get"]
```

Matches: "sword", "longsword", "shortsword"

### Pattern Match

```toml
[[cmdlist]]
noun = "(?i).*sword.*"
match_mode = "regex"
commands = ["look", "get"]
```

Matches any noun containing "sword" (case-insensitive)

## Command Syntax

### Simple Command

```toml
commands = ["look", "get", "drop"]
# Results in: look {noun}, get {noun}, drop {noun}
```

### Custom Command Format

```toml
commands = [
    "look",
    "get",
    "put in backpack:put {noun} in my backpack",
    "give to:give {noun} to {target}"
]
```

Format: `label:command` or just `command`

### Variables

| Variable | Description |
|----------|-------------|
| `{noun}` | Object noun |
| `{exist}` | Object ID |
| `{target}` | Prompted target |
| `{input}` | Prompted input |

## Categories

### Default Categories

```toml
# Creature commands
[[cmdlist]]
category = "creature"
noun = ".*"
match_mode = "regex"
commands = ["attack", "look", "assess"]
priority = 10

# Container commands
[[cmdlist]]
category = "container"
noun = "(?i)(backpack|bag|pouch|sack)"
match_mode = "regex"
commands = ["look in", "open", "close"]
priority = 20
```

### Custom Categories

```toml
[[cmdlist]]
category = "alchemy"
noun = "(?i)(vial|potion|herb)"
match_mode = "regex"
commands = [
    "look",
    "get",
    "drink:drink my {noun}",
    "mix:mix my {noun}"
]
```

## Common Cmdlists

### Creatures

```toml
[[cmdlist]]
category = "combat"
noun = ".*"  # Default for all creatures
match_mode = "regex"
commands = [
    "attack",
    "attack left",
    "attack right",
    "look",
    "assess"
]
priority = 10
```

### Weapons

```toml
[[cmdlist]]
category = "weapon"
noun = "(?i)(sword|dagger|axe|mace|staff)"
match_mode = "regex"
commands = [
    "look",
    "get",
    "drop",
    "wield",
    "sheath",
    "appraise"
]
```

### Containers

```toml
[[cmdlist]]
category = "container"
noun = "(?i)(backpack|bag|pouch|cloak|sack)"
match_mode = "regex"
commands = [
    "look in",
    "open",
    "close",
    "get from:get {input} from my {noun}"
]
```

### Players

```toml
[[cmdlist]]
category = "player"
noun = "^[A-Z][a-z]+$"  # Capitalized names
match_mode = "regex"
commands = [
    "look",
    "smile",
    "bow",
    "whisper:whisper {noun} {input}",
    "give:give {input} to {noun}"
]
```

### Herbs/Alchemy

```toml
[[cmdlist]]
category = "herb"
noun = "(?i)(acantha|ambrominas|basal|cactacae)"
match_mode = "regex"
commands = [
    "look",
    "get",
    "eat",
    "put in:put my {noun} in my {input}"
]
```

### Gems/Treasure

```toml
[[cmdlist]]
category = "treasure"
noun = "(?i)(gem|jewel|coin|gold|silver)"
match_mode = "regex"
commands = [
    "look",
    "get",
    "appraise",
    "sell"
]
```

## Menu Separators

Add visual separators:

```toml
[[cmdlist]]
noun = "sword"
commands = [
    "look",
    "get",
    "---",           # Separator
    "wield",
    "sheath",
    "---",
    "appraise",
    "sell"
]
```

## Submenus

Create nested menus:

```toml
[[cmdlist]]
noun = "sword"
commands = [
    "look",
    "get",
    "Combat>attack,attack left,attack right",
    "Container>put in backpack:put {noun} in my backpack,put in sack:put {noun} in my sack"
]
```

## Priority System

Higher priority cmdlists appear first:

```toml
# Specific override
[[cmdlist]]
noun = "magic sword"
commands = ["invoke", "look", "get"]
priority = 100         # High priority

# General fallback
[[cmdlist]]
noun = "(?i).*sword.*"
match_mode = "regex"
commands = ["look", "get"]
priority = 10          # Lower priority
```

## Game Object Links

VellumFE extracts object data from game XML:

```xml
<a exist="12345" noun="sword">a rusty sword</a>
```

- `exist`: Object ID for direct commands
- `noun`: Used for cmdlist matching
- Text: Display name

## Direct Commands

Use object ID for precise targeting:

```toml
[[cmdlist]]
noun = "sword"
commands = [
    "look",
    "_inspect {exist}",      # Uses object ID
    "get #{exist}",          # Direct reference
]
```

## Testing Cmdlists

1. Save `cmdlist.toml`
2. Run `.reload cmdlist`
3. Right-click on matching object
4. Verify menu appears correctly

## Troubleshooting

### Menu Not Appearing

1. Check noun matches object exactly
2. Try `match_mode = "contains"`
3. Verify cmdlist.toml syntax
4. Run `.reload cmdlist`

### Wrong Commands

1. Check priority order
2. Verify pattern matching
3. Test with exact noun match first

### Variables Not Replaced

1. Check variable syntax `{noun}`, `{exist}`
2. Ensure object has required data
3. Check for typos in variable names

## Complete Example

```toml
# cmdlist.toml

# Combat creatures
[[cmdlist]]
category = "combat"
noun = ".*"
match_mode = "regex"
commands = [
    "attack",
    "attack left",
    "attack right",
    "---",
    "look",
    "assess"
]
priority = 5

# Weapons
[[cmdlist]]
category = "weapon"
noun = "(?i)(sword|dagger|axe|mace|staff|bow)"
match_mode = "regex"
commands = [
    "look",
    "get",
    "---",
    "wield",
    "sheath",
    "---",
    "appraise"
]
priority = 20

# Containers
[[cmdlist]]
category = "container"
noun = "(?i)(backpack|bag|pouch|cloak|sack|chest)"
match_mode = "regex"
commands = [
    "look in",
    "open",
    "close",
    "---",
    "get from:get {input} from my {noun}"
]
priority = 20

# Currency
[[cmdlist]]
category = "currency"
noun = "(?i)(silver|gold|coins?)"
match_mode = "regex"
commands = [
    "look",
    "get",
    "count"
]
priority = 15

# Players
[[cmdlist]]
category = "player"
noun = "^[A-Z][a-z]+$"
match_mode = "regex"
commands = [
    "look",
    "---",
    "smile",
    "wave",
    "bow",
    "---",
    "whisper:whisper {noun} {input}",
    "give:give {input} to {noun}"
]
priority = 25
```

## See Also

- [Macros](./macros.md) - Keybind automation
- [Keybind Actions](../customization/keybind-actions.md) - Key commands
- [Command Input](../widgets/command-input.md) - Command entry

