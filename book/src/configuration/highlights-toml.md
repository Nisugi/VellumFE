# highlights.toml

Text highlighting rules for coloring, sounds, and redirects.

## Basic Format

```toml
[[highlights]]
name = "my_highlight"
pattern = "pattern to match"
foreground = "#FF0000"
```

## Pattern Matching

### Literal Match (Fast)

```toml
[[highlights]]
name = "death_cry"
pattern = "death cry"           # Exact text match
foreground = "#FF0000"
```

### Regex Match

```toml
[[highlights]]
name = "creature_attack"
pattern = "^A .+ (swings|claws|lunges)"
is_regex = true
foreground = "#FFA500"
```

## Styling Options

```toml
[[highlights]]
name = "whisper"
pattern = "whispers,"
foreground = "#9370DB"          # Text color
background = "#1a1a2e"          # Background color
bold = true
italic = true
underline = true
```

### Colors

Use hex colors (`#RRGGBB`) or named colors:
- Basic: `red`, `green`, `blue`, `yellow`, `cyan`, `magenta`, `white`, `black`
- Extended: `gray`, `orange`, `purple`, `pink`, `brown`

## Sound Alerts

```toml
[[highlights]]
name = "dead"
pattern = "appears dead"
foreground = "#00FF00"
sound = "ding.wav"              # File in ~/.vellum-fe/sounds/
sound_volume = 0.8              # 0.0 to 1.0
```

## Text Replacement

```toml
[[highlights]]
name = "shorten_deaths"
pattern = "The death cry of"
replace = "†"
foreground = "#FF0000"
```

## Stream Redirect

Route matching lines to another window:

```toml
[[highlights]]
name = "loot_to_window"
pattern = "^You gather"
redirect = "loot"               # Window name
```

## Full Example

```toml
# Creature deaths
[[highlights]]
name = "creature_dead"
pattern = "appears dead"
foreground = "#00FF00"
bold = true
sound = "kill.wav"

# Player speech
[[highlights]]
name = "says"
pattern = ' (says|asks|exclaims),'
is_regex = true
foreground = "#87CEEB"

# Whispers (with background)
[[highlights]]
name = "whisper"
pattern = "whispers,"
foreground = "#DDA0DD"
background = "#2a1a2a"
italic = true

# Stun warning
[[highlights]]
name = "stunned"
pattern = "You are stunned"
foreground = "#FF4500"
bold = true
sound = "alert.wav"

# Treasure (redirect to loot window)
[[highlights]]
name = "loot"
pattern = "^(You gather|You search)"
is_regex = true
foreground = "#FFD700"
redirect = "loot"
```

## Priority

Highlights are applied in order. Later patterns can override earlier ones.

## Disabling

Disable without deleting via config.toml:

```toml
[highlights]
sounds_enabled = false          # Disable all sounds
coloring_enabled = false        # Disable all coloring
```
