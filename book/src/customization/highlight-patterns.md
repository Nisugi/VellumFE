# Highlight Patterns

Highlights let you colorize text based on patterns. This guide covers advanced pattern creation.

## Pattern Basics

Highlights use regex (regular expressions):

```toml
[[highlights]]
name = "creatures"
pattern = "(?i)\\b(goblin|orc|troll)\\b"
fg = "#FF6600"
bold = true
```

## Regex Syntax

### Literal Text

```toml
# Exact match
pattern = "goblin"

# Multiple words
pattern = "goblin|orc|troll"
```

### Character Classes

```toml
# Any digit
pattern = "\\d+"          # Matches: 123, 45, 6789

# Any word character
pattern = "\\w+"          # Matches: word, Word123

# Any whitespace
pattern = "\\s+"          # Matches spaces, tabs

# Custom class
pattern = "[A-Z][a-z]+"   # Matches: Hello, World
```

### Quantifiers

```toml
# Zero or more
pattern = "a*"            # Matches: "", a, aa, aaa

# One or more
pattern = "a+"            # Matches: a, aa, aaa

# Zero or one
pattern = "colou?r"       # Matches: color, colour

# Exactly n
pattern = "\\d{3}"        # Matches: 123, 456

# n to m
pattern = "\\d{2,4}"      # Matches: 12, 123, 1234
```

### Anchors

```toml
# Start of line
pattern = "^You attack"

# End of line
pattern = "falls dead!$"

# Word boundary
pattern = "\\bgoblin\\b"  # Won't match "goblins" or "hobgoblin"
```

### Groups

```toml
# Non-capturing group
pattern = "(?:goblin|orc)"

# Capturing group (for future use)
pattern = "(\\w+) says,"
```

### Modifiers

```toml
# Case insensitive
pattern = "(?i)goblin"    # Matches: goblin, Goblin, GOBLIN
```

## Common Patterns

### Creature Names

```toml
[[highlights]]
name = "creatures"
pattern = "(?i)\\b(goblin|orc|troll|kobold|giant|wolf|bear)s?\\b"
fg = "#FF6600"
bold = true
```

### Player Speech

```toml
[[highlights]]
name = "speech"
pattern = "^(\\w+) (says|asks|exclaims|whispers),"
fg = "#00FF00"
```

### Your Character Name

```toml
[[highlights]]
name = "my_name"
pattern = "\\bYourCharacterName\\b"
fg = "#FFFF00"
bold = true
```

### Damage Numbers

```toml
[[highlights]]
name = "damage"
pattern = "\\b\\d+\\s+points?\\s+of\\s+damage"
fg = "#FF0000"
bold = true
```

### Currency

```toml
[[highlights]]
name = "silver"
pattern = "\\d+\\s+silvers?"
fg = "#C0C0C0"

[[highlights]]
name = "gold"
pattern = "\\d+\\s+gold"
fg = "#FFD700"
```

### Room Exits

```toml
[[highlights]]
name = "exits"
pattern = "Obvious (paths|exits):"
fg = "#00FFFF"
```

### ESP/Thoughts

```toml
[[highlights]]
name = "esp"
pattern = "^\\[.*?\\]"
fg = "#FF00FF"
stream = "thoughts"
```

### Critical Hits

```toml
[[highlights]]
name = "critical"
pattern = "(?i)critical|devastating|massive"
fg = "#FF0000"
bg = "#400000"
bold = true
```

### Healing

```toml
[[highlights]]
name = "healing"
pattern = "heal|restore|recover"
fg = "#00FF00"
```

## Stream Filtering

Limit patterns to specific streams:

```toml
[[highlights]]
name = "combat"
pattern = "You (hit|miss)"
fg = "#FF0000"
stream = "main"

[[highlights]]
name = "esp_highlight"
pattern = "\\[ESP\\]"
fg = "#00FFFF"
stream = "thoughts"
```

## Priority System

Higher priority wins when multiple patterns match:

```toml
[[highlights]]
name = "important"
pattern = "critical"
fg = "#FF0000"
priority = 100          # Checked first

[[highlights]]
name = "general"
pattern = ".*"
fg = "#808080"
priority = 0            # Checked last
```

## Fast Parse Mode

For simple literal patterns, use `fast_parse`:

```toml
[[highlights]]
name = "names"
pattern = "Bob|Alice|Charlie"
fg = "#FFFF00"
fast_parse = true       # Uses Aho-Corasick algorithm
```

**When to use fast_parse:**
- Pattern is literal strings only
- Pattern has many alternatives
- High-frequency matching needed

**When NOT to use:**
- Pattern uses regex features
- Pattern needs word boundaries
- Pattern uses anchors

## Special Characters

Escape these with `\\`:

```
. * + ? ^ $ { } [ ] ( ) | \
```

```toml
# Match literal period
pattern = "Mr\\. Smith"

# Match literal asterisk
pattern = "\\*\\*emphasis\\*\\*"

# Match literal parentheses
pattern = "\\(optional\\)"
```

## Sounds

Add sounds to highlights:

```toml
[[highlights]]
name = "whisper_alert"
pattern = "whispers to you"
fg = "#00FFFF"
sound = "~/.vellum-fe/sounds/whisper.wav"
```

## Squelch (Hide)

Hide matching text entirely:

```toml
[[highlights]]
name = "spam_filter"
pattern = "A gentle breeze|The sun|Birds chirp"
squelch = true
```

## Redirect

Send matching text to different window:

```toml
[[highlights]]
name = "combat_redirect"
pattern = "You (attack|hit|miss|dodge)"
redirect = "combat"
redirect_mode = "copy"  # or "move"
```

## Testing Patterns

### Test in Browser

```
.highlights
```

Navigate to your highlight and check the preview.

### Test with Game Output

1. Add highlight
2. Run `.reload highlights`
3. Trigger the pattern in-game
4. Verify appearance

### Online Regex Testers

Use regex101.com or similar to test patterns before adding.

## Performance Tips

1. **Use fast_parse** for simple patterns
2. **Use word boundaries** `\b` to limit matching
3. **Be specific** - avoid `.*` when possible
4. **Limit stream** - use `stream = "main"` when appropriate
5. **Keep count low** - fewer than 100 patterns recommended

## Debugging

### Pattern Not Matching

1. Test pattern in regex101.com
2. Check escaping (use `\\` not `\`)
3. Check case sensitivity
4. Verify stream setting

### Pattern Too Broad

Add boundaries:

```toml
# Too broad
pattern = "hit"        # Matches "white", "smith"

# Better
pattern = "\\bhit\\b"  # Only matches "hit" as word
```

### Escaping Issues

TOML strings need double backslashes:

```toml
# Wrong
pattern = "\d+"

# Correct
pattern = "\\d+"
```

## Complete Example

```toml
# Combat Highlights
[[highlights]]
name = "damage_taken"
pattern = "(?i)(strikes|hits|bites|claws|slashes)\\s+you"
fg = "#FF4444"
bold = true
priority = 80

[[highlights]]
name = "damage_dealt"
pattern = "You\\s+(strike|hit|slash|stab)"
fg = "#44FF44"
priority = 80

[[highlights]]
name = "critical"
pattern = "(?i)critical|devastating"
fg = "#FF0000"
bg = "#400000"
bold = true
priority = 100

# Creatures
[[highlights]]
name = "creatures"
pattern = "(?i)\\b(goblin|orc|troll|kobold|wolf|bear|rat)s?\\b"
fg = "#FF8800"
bold = true
priority = 50

# Social
[[highlights]]
name = "my_name"
pattern = "\\bYourCharacter\\b"
fg = "#FFFF00"
bold = true
priority = 90

[[highlights]]
name = "friends"
pattern = "\\b(Alice|Bob|Charlie)\\b"
fg = "#00FF00"
fast_parse = true
priority = 60

# Currency
[[highlights]]
name = "silver"
pattern = "\\d+\\s+silvers?"
fg = "#C0C0C0"
bold = true

[[highlights]]
name = "treasure"
pattern = "(?i)(gold|gems?|jewel|treasure)"
fg = "#FFD700"
```

## See Also

- [Highlights Configuration](../configuration/highlights-toml.md) - Full reference
- [Theme System](../architecture/theme-system.md) - How colors work
- [Performance](../architecture/performance.md) - Optimization

