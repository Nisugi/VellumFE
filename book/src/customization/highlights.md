# Highlight Patterns

Color and style game text based on patterns.

## Basic Highlight

```toml
[[highlights]]
name = "player_speech"
pattern = " says,"
foreground = "#87CEEB"
```

## Pattern Types

### Literal (Default)

Fast exact matching:

```toml
pattern = "appears dead"
```

### Regex

Flexible pattern matching:

```toml
pattern = "^A .+ attacks"
is_regex = true
```

## Styling Options

```toml
[[highlights]]
name = "important"
pattern = "IMPORTANT"
foreground = "#FF0000"      # Text color
background = "#330000"      # Background color
bold = true
italic = true
underline = true
```

## Sound Alerts

```toml
[[highlights]]
name = "whisper_alert"
pattern = "whispers to you"
foreground = "#DDA0DD"
sound = "whisper.wav"       # In ~/.vellum-fe/sounds/
sound_volume = 0.8          # 0.0 to 1.0
```

## Text Replacement

```toml
[[highlights]]
name = "shorten"
pattern = "The death cry of"
replace = "†"
foreground = "#FF0000"
```

## Stream Redirect

Route lines to another window:

```toml
[[highlights]]
name = "loot_redirect"
pattern = "^You gather"
is_regex = true
redirect = "loot"           # Window name
```

## Common Patterns

```toml
# Combat
[[highlights]]
name = "creature_dead"
pattern = "appears dead"
foreground = "#00FF00"
bold = true
sound = "kill.wav"

[[highlights]]
name = "you_hit"
pattern = "^You .+ (swing|slash|thrust)"
is_regex = true
foreground = "#FFFF00"

[[highlights]]
name = "you_miss"
pattern = "^A clean miss"
foreground = "#808080"

# Communication
[[highlights]]
name = "says"
pattern = " says,"
foreground = "#87CEEB"

[[highlights]]
name = "whisper"
pattern = "whispers,"
foreground = "#DDA0DD"
italic = true

[[highlights]]
name = "thoughts"
pattern = "^You hear .+ thinking,"
is_regex = true
foreground = "#9370DB"

# Warnings
[[highlights]]
name = "stunned"
pattern = "You are stunned"
foreground = "#FF4500"
bold = true
sound = "alert.wav"

[[highlights]]
name = "bleeding"
pattern = "Blood runs down"
foreground = "#FF0000"
sound = "danger.wav"
```

## Disabling Highlights

In `config.toml`:

```toml
[highlights]
sounds_enabled = false      # Disable sounds
coloring_enabled = false    # Disable colors
replace_enabled = false     # Disable replacements
```
