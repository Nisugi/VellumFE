# Highlight Patterns

Recipes for common highlighting tasks. Full field reference:
[highlights.toml](../configuration/highlights-toml.md). The in-app editor
(`.addhighlight`) walks you through the same fields.

## Color Important Text

```toml
[creature_dead]
pattern = "appears dead"
fg = "#00ff00"
bold = true
category = "Combat"

[stunned]
pattern = "You are stunned"
fg = "#ff4500"
bold = true
sound = "alert.wav"
category = "Warnings"
```

## Highlight Names (Friends and Enemies)

Use `fast_parse` for lists of literal words — it's much faster than regex:

```toml
[friends]
pattern = "Mandrill|Monolis|Chiora"
fg = "#ff00ff"
bold = true
fast_parse = true
category = "Players"

[enemies]
pattern = "Sihtric|Ehria"
fg = "#ffffff"
bg = "#8b0000"
bold = true
fast_parse = true
category = "Players"
```

## Hide Spam (Squelch)

```toml
[ambient_spam]
pattern = "A cool breeze|The wind blows|A leaf falls"
fast_parse = true
squelch = true
category = "Squelch"
```

## Route Lines to Another Window

```toml
[loot_lines]
pattern = "^You gather"
redirect_to = "loot"
redirect_mode = "redirect_copy"    # "redirect_only" (the default) to move instead of copy
```

## Rewrite Text

Capture groups from the pattern are available as `$1`, `$2`:

```toml
[shorten_deaths]
pattern = "The death cry of (\\w+)"
replace = "† $1"
fg = "#ff0000"
```

## Limit to One Stream or Window

```toml
[thought_names]
pattern = "^\\[(\\w+)\\]"
fg = "#9370db"
stream = "thoughts"       # only applies to the thoughts stream
```

For replacement patterns, `window = "..."` limits the *replacement* to one
window by name (colors still apply everywhere). Both filter fields are
editable in the TUI highlight form (`.edithighlight`).

## Test Your Patterns

Don't wait for the game — inject a line:

```
.testline The death cry of Grimswarm echoes!
```

## Tips

- Patterns are regexes: escape literal `.` `(` `[`, use `(?i)` for
  case-insensitive, anchor with `^` where you can.
- Use `category` — the `.highlights` browser groups by it.
- Save variants per activity: `.savehighlights hunting`,
  `.loadhighlights hunting`.
