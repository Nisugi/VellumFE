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
rumble = "long"           # buzz the controller too (optional)
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

## Custom Statuses

A rule can drive a status icon of your own invention. Set these in the
GUI highlight editor:

- **Set status** — a status id to activate when the rule matches
  (e.g. `POISONED`).
- **Status duration** — seconds until it clears itself; leave empty to
  keep it on until a clearing rule matches.
- **Clear status** — a status id to deactivate on match.

Any indicator or dashboard entry whose id matches lights up, exactly
like the game's built-in statuses — so it gets skin/pool icon art, the
per-indicator icon picker, grayscale-when-inactive, and TUI glyphs for
free. Typical use: regex a spell's wear-off message into a status so
you can *see* the buff drop:

```toml
[silver_lace_down]
pattern = "The silvery luminescence fades"
set_status = "SILVERLACE_DOWN"
status_duration = 30.0
```

Then add an indicator window with id `SILVERLACE_DOWN` (Indicator
Templates editor) and pick an icon for it.

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
