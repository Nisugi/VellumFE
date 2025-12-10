# Triggers

Triggers automatically execute commands when specific text patterns appear in the game.

## Overview

Triggers watch for patterns and respond:

```toml
# In triggers.toml
[[triggers]]
pattern = "You are stunned"
command = ".say Stunned!"

[[triggers]]
pattern = "falls dead"
command = "search;loot"
```

## Basic Triggers

### Simple Trigger

```toml
[[triggers]]
name = "stun_alert"
pattern = "You are stunned"
command = ".notify Stunned!"
```

### Multiple Commands

```toml
[[triggers]]
name = "death_loot"
pattern = "falls dead"
command = "search;loot"
```

## Trigger Properties

```toml
[[triggers]]
name = "my_trigger"           # Unique name
pattern = "regex pattern"     # Pattern to match
command = "game command"      # Command to execute

# Optional properties
enabled = true                # Enable/disable
priority = 100                # Higher = checked first
stream = "main"               # Limit to specific stream
cooldown = 1000               # Minimum ms between triggers
category = "combat"           # Grouping
```

## Pattern Syntax

Triggers use regex patterns:

### Literal Text

```toml
pattern = "You are stunned"
```

### Case Insensitive

```toml
pattern = "(?i)you are stunned"
```

### Word Boundaries

```toml
pattern = "\\bstunned\\b"
```

### Capture Groups

```toml
pattern = "(\\w+) falls dead"
command = "search #{noun}"
```

### Multiple Patterns

```toml
pattern = "(stun|web|prone)"
```

## Command Options

### Game Commands

```toml
command = "attack target"
```

### Client Commands

```toml
command = ".notify Alert!"      # VellumFE command
command = ".sound alert.wav"    # Play sound
command = ".tts Danger!"        # Text-to-speech
```

### Multiple Commands

```toml
command = "search;loot;skin"
```

### With Delay

```toml
command = "prep 101;{500};cast"
```

## Captured Variables

Use captured groups in commands:

```toml
[[triggers]]
pattern = "(\\w+) whispers,"
command = ".notify Whisper from $1"
# $1 = captured name
```

### Variable Reference

| Variable | Description |
|----------|-------------|
| `$0` | Entire match |
| `$1` | First capture group |
| `$2` | Second capture group |
| `$n` | Nth capture group |

## Stream Filtering

Limit triggers to specific streams:

```toml
[[triggers]]
pattern = "ESP"
command = ".notify ESP received"
stream = "thoughts"

[[triggers]]
pattern = "attack"
command = ".beep"
stream = "combat"
```

## Cooldowns

Prevent rapid-fire triggers:

```toml
[[triggers]]
pattern = "You are hit"
command = ".sound hit.wav"
cooldown = 500     # Only trigger once per 500ms
```

## Common Triggers

### Combat Alerts

```toml
# Stunned
[[triggers]]
name = "stun_alert"
pattern = "You are stunned"
command = ".notify Stunned!"
category = "combat"

# Webbed
[[triggers]]
name = "web_alert"
pattern = "webs stick to you"
command = ".notify Webbed!"
category = "combat"

# Low health
[[triggers]]
name = "low_health"
pattern = "You feel weak"
command = ".notify Low Health!;stance defensive"
category = "combat"
priority = 100
```

### Social Alerts

```toml
# Whispers
[[triggers]]
name = "whisper"
pattern = "(\\w+) whispers,"
command = ".notify Whisper from $1"
category = "social"

# Name mentioned
[[triggers]]
name = "name_mention"
pattern = "\\bYourCharacter\\b"
command = ".sound mention.wav"
category = "social"
```

### Loot Triggers

```toml
# Auto-search on kill
[[triggers]]
name = "auto_search"
pattern = "falls dead"
command = "search"
cooldown = 1000
category = "loot"

# Treasure alert
[[triggers]]
name = "treasure"
pattern = "(?i)treasure|gold|gems"
command = ".notify Treasure!"
category = "loot"
```

### Status Triggers

```toml
# Roundtime tracker
[[triggers]]
name = "roundtime"
pattern = "Roundtime: (\\d+)"
command = ".countdown $1"
category = "status"

# Hidden status
[[triggers]]
name = "hidden"
pattern = "You melt into the shadows"
command = ".notify Hidden"
category = "status"
```

## Priority System

Higher priority triggers are checked first:

```toml
[[triggers]]
name = "critical_danger"
pattern = "dragon"
command = "flee"
priority = 100      # Highest priority

[[triggers]]
name = "general_alert"
pattern = "attacks you"
command = ".beep"
priority = 10       # Lower priority
```

## Conditional Triggers

### With Capture Check

```toml
[[triggers]]
pattern = "(\\w+) says, \"(.+)\""
command = ".log $1: $2"
# Only triggers if both captures match
```

### Negative Lookahead

```toml
pattern = "(?!You )\\w+ attacks"
# Matches "Goblin attacks" but not "You attack"
```

## Enable/Disable

### In Configuration

```toml
[[triggers]]
name = "my_trigger"
pattern = "something"
command = "respond"
enabled = false     # Disabled by default
```

### Via Commands

```
.trigger enable my_trigger
.trigger disable my_trigger
.trigger toggle my_trigger
```

### Trigger Groups

```
.trigger enable combat    # Enable all combat triggers
.trigger disable loot     # Disable all loot triggers
```

## Testing Triggers

1. Add trigger to `triggers.toml`
2. Run `.reload triggers`
3. Look for matching text in game
4. Verify trigger fires

### Debug Mode

```
.trigger debug on
```

Shows when triggers match.

## Safety Considerations

### Avoid Automation Abuse

- Don't automate core gameplay
- Keep human-like delays
- Don't trigger in loops
- Use cooldowns

### Test Thoroughly

- Test in safe areas
- Verify pattern accuracy
- Check for false positives
- Monitor for issues

## Complete Example

```toml
# triggers.toml

# === COMBAT ALERTS ===
[[triggers]]
name = "stun_alert"
pattern = "(?i)you are stunned"
command = ".notify Stunned!;.sound stun.wav"
category = "combat"
priority = 100
cooldown = 1000

[[triggers]]
name = "web_alert"
pattern = "(?i)webs? (stick|entangle)"
command = ".notify Webbed!"
category = "combat"
priority = 90

[[triggers]]
name = "prone_alert"
pattern = "(?i)knock.*down|fall.*prone"
command = ".notify Prone!"
category = "combat"
priority = 90

# === HEALTH ALERTS ===
[[triggers]]
name = "low_health"
pattern = "(?i)feel (weak|faint|dizzy)"
command = ".notify Low Health!;.sound warning.wav"
category = "health"
priority = 100

[[triggers]]
name = "death"
pattern = "You have died"
command = ".notify DEAD!;.sound death.wav"
category = "health"
priority = 100

# === SOCIAL ===
[[triggers]]
name = "whisper"
pattern = "(\\w+) whispers,"
command = ".notify Whisper from $1;.sound whisper.wav"
category = "social"
stream = "main"

[[triggers]]
name = "name_mention"
pattern = "\\bYourCharacter\\b"
command = ".sound mention.wav"
category = "social"
cooldown = 5000

# === LOOT ===
[[triggers]]
name = "auto_search"
pattern = "falls dead"
command = "search"
category = "loot"
cooldown = 2000
enabled = false    # Disabled by default

# === STATUS ===
[[triggers]]
name = "roundtime"
pattern = "Roundtime: (\\d+)"
command = ".rt $1"
category = "status"
```

## See Also

- [Highlight Patterns](../customization/highlight-patterns.md) - Pattern syntax
- [Sound Alerts](../customization/sound-alerts.md) - Audio notifications
- [Macros](./macros.md) - Keybind automation

