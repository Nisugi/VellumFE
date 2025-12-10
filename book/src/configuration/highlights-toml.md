# highlights.toml Reference

The highlights configuration file defines text patterns and their visual styling.

## Location

`~/.vellum-fe/highlights.toml`

---

## Structure Overview

```toml
[[highlights]]
name = "creatures"
pattern = "(goblin|orc|troll)"
fg = "#FF0000"
bold = true

[[highlights]]
name = "player_speech"
pattern = '^\w+ (says|asks|exclaims),'
fg = "#00FF00"
stream = "main"
```

---

## Highlight Properties

Each `[[highlights]]` entry defines one highlighting rule:

```toml
[[highlights]]
# Identity
name = "my_highlight"       # Unique name (required)
pattern = "text to match"   # Regex pattern (required)

# Colors
fg = "#FF0000"              # Foreground color
bg = "#000000"              # Background color

# Styling
bold = false                # Bold text
italic = false              # Italic text
underline = false           # Underlined text

# Scope
stream = "main"             # Limit to stream (optional)
span_type = "Normal"        # Only match span type (optional)

# Performance
fast_parse = false          # Use literal matching
enabled = true              # Enable/disable
priority = 0                # Match priority (higher = first)
```

### Required Properties

| Property | Type | Description |
|----------|------|-------------|
| `name` | string | Unique identifier |
| `pattern` | string | Regex pattern to match |

### Color Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `fg` | string | - | Foreground color |
| `bg` | string | - | Background color |

**Color formats:**
```toml
fg = "#FF5500"        # Hex RGB
fg = "bright_red"     # Palette name
fg = "@speech"        # Preset reference
```

### Style Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `bold` | boolean | `false` | Bold text |
| `italic` | boolean | `false` | Italic text |
| `underline` | boolean | `false` | Underline text |

### Scope Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `stream` | string | all | Limit to specific stream |
| `span_type` | string | all | Only match span type |

**Stream values:** `main`, `speech`, `thoughts`, `combat`, etc.

**Span types:**
- `Normal` - Regular text
- `Link` - Clickable links
- `Monsterbold` - Creature names
- `Speech` - Player dialogue
- `Spell` - Spell names

### Performance Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `fast_parse` | boolean | `false` | Use Aho-Corasick |
| `enabled` | boolean | `true` | Enable this highlight |
| `priority` | integer | `0` | Match order (higher first) |

---

## Pattern Syntax

Highlights use Rust regex syntax. Key features:

### Basic Patterns

```toml
# Literal text
pattern = "goblin"

# Case insensitive
pattern = "(?i)goblin"

# Word boundaries
pattern = "\\bgoblin\\b"

# Multiple words
pattern = "goblin|orc|troll"
```

### Character Classes

```toml
# Any digit
pattern = "\\d+"

# Any word character
pattern = "\\w+"

# Any whitespace
pattern = "\\s+"

# Custom class
pattern = "[A-Z][a-z]+"
```

### Anchors

```toml
# Start of line
pattern = "^You attack"

# End of line
pattern = "falls dead!$"

# Word boundary
pattern = "\\bgoblin\\b"
```

### Groups and Captures

```toml
# Non-capturing group
pattern = "(?:goblin|orc)"

# Capturing group (for future use)
pattern = "(\\w+) says,"
```

### Escaping Special Characters

These characters must be escaped with `\\`:
```
. * + ? ^ $ { } [ ] ( ) | \
```

```toml
# Match literal period
pattern = "Mr\\. Smith"

# Match literal asterisk
pattern = "\\*\\*emphasis\\*\\*"
```

---

## Fast Parse Mode

For simple literal patterns, `fast_parse = true` uses Aho-Corasick algorithm:

```toml
[[highlights]]
name = "names"
pattern = "Bob|Alice|Charlie"
fg = "#FFFF00"
fast_parse = true
```

**When to use fast_parse:**
- Pattern is literal strings (no regex)
- Pattern has many alternatives (names, items)
- High-frequency matching needed

**When NOT to use fast_parse:**
- Pattern uses regex features
- Pattern needs word boundaries
- Pattern uses anchors (^, $)

---

## Priority System

When multiple highlights match, priority determines which wins:

```toml
# Higher priority wins
[[highlights]]
name = "important"
pattern = "critical"
fg = "#FF0000"
priority = 100

# Lower priority, same match
[[highlights]]
name = "general"
pattern = ".*"
fg = "#808080"
priority = 0
```

**Default priorities:**
- Monsterbold spans: Highest
- Link spans: High
- User highlights: Medium (configurable)
- Preset colors: Low

---

## Stream Filtering

Limit highlights to specific streams:

```toml
# Only in main window
[[highlights]]
name = "combat"
pattern = "You (hit|miss)"
fg = "#FF0000"
stream = "main"

# Only in thoughts
[[highlights]]
name = "esp"
pattern = "\\[ESP\\]"
fg = "#00FFFF"
stream = "thoughts"
```

---

## Common Patterns

### Creature Names

```toml
[[highlights]]
name = "creatures"
pattern = "(?i)\\b(goblin|orc|troll|kobold|giant)s?\\b"
fg = "#FF6600"
bold = true
```

### Player Speech

```toml
[[highlights]]
name = "speech"
pattern = '^(\\w+) (says|asks|exclaims|whispers),'
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

### Silver/Gold

```toml
[[highlights]]
name = "silver"
pattern = "\\d+\\s+silver"
fg = "#C0C0C0"
bold = true

[[highlights]]
name = "gold"
pattern = "\\d+\\s+gold"
fg = "#FFD700"
bold = true
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

---

## Complete Example

```toml
# VellumFE Highlights Configuration

# Creature highlighting
[[highlights]]
name = "creatures"
pattern = "(?i)\\b(goblin|orc|troll|kobold|giant rat|wolf|bear)s?\\b"
fg = "#FF6600"
bold = true
priority = 50

# Player names (customize this list)
[[highlights]]
name = "friends"
pattern = "\\b(Alice|Bob|Charlie)\\b"
fg = "#00FF00"
fast_parse = true
priority = 60

# Damage taken
[[highlights]]
name = "damage_taken"
pattern = "strikes you|hits you|bites you"
fg = "#FF0000"
bold = true
priority = 70

# Damage dealt
[[highlights]]
name = "damage_dealt"
pattern = "You (hit|strike|slash)"
fg = "#00FF00"
priority = 70

# Currency
[[highlights]]
name = "silver"
pattern = "\\d+\\s+silvers?"
fg = "#C0C0C0"

[[highlights]]
name = "gold"
pattern = "\\b(gold|golden)\\b"
fg = "#FFD700"

# Important messages
[[highlights]]
name = "roundtime"
pattern = "Roundtime:"
fg = "#FF00FF"
bold = true

[[highlights]]
name = "death"
pattern = "You have died|DEAD"
fg = "#FF0000"
bg = "#400000"
bold = true
priority = 100
```

---

## See Also

- [Highlight Patterns Guide](../customization/highlight-patterns.md) - Advanced patterns
- [Colors Reference](./colors-toml.md) - Color values and palettes
- [Performance](../architecture/performance.md) - Highlight optimization
