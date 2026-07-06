# highlights.toml

Text highlighting rules: coloring, sounds, line filtering (squelch),
redirects, and text replacement. Edit the file directly (then
`.reload highlights`) or use the in-app browser (`.highlights` / `.addhighlight`).

## Basic Format

Each highlight is a named TOML table:

```toml
[stunned]
pattern = "You are stunned"
fg = "#ff4500"
bold = true
category = "Combat"
```

Patterns are regular expressions. For plain-word lists, set
`fast_parse = true` and separate alternatives with `|` — this uses a much
faster literal matcher:

```toml
[friends]
pattern = "Mandrill|Monolis|Chiora"
fg = "#ff00ff"
bold = true
fast_parse = true
category = "Players"
```

## All Fields

| Field | Type | Description |
|-------|------|-------------|
| `pattern` | string | Regex (or `\|`-separated literals with `fast_parse`) |
| `fg` / `bg` | color | Text / background color — hex or a palette color name |
| `bold` | bool | Bold text |
| `color_entire_line` | bool | Color the whole line, not just the match |
| `fast_parse` | bool | Literal matching via Aho-Corasick (much faster) |
| `sound` | string | Sound file to play (in `global/sounds/`) |
| `sound_volume` | float | Per-sound volume override (0.0–1.0) |
| `category` | string | Grouping in the highlights browser (e.g. `"Combat"`) |
| `squelch` | bool | **Hide matching lines entirely** |
| `silent_prompt` | bool | Suppress the prompt after squelched lines |
| `redirect_to` | string | Send matching lines to this window |
| `redirect_mode` | string | `"only"` (move) or `"copy"` (show in both) |
| `replace` | string | Replace matched text (supports `$1`, `$2` capture groups) |
| `stream` | string | Only apply to lines from this stream (e.g. `"thoughts"`) |
| `window` | string | With `replace`: only replace in this window |

## Squelch (Filtering Spam)

Hide lines you never want to see:

```toml
[ambient_spam]
pattern = "A cool breeze|The wind blows|A leaf falls"
fast_parse = true
squelch = true
category = "Squelch"

[arrival_spam]
pattern = "^[A-Z][a-z]+ (arrives|departs)"
squelch = true
category = "Squelch"
```

## Sounds

```toml
[death_alert]
pattern = "appears dead"
fg = "#00ff00"
sound = "kill.wav"        # in ~/.vellum-fe/global/sounds/
sound_volume = 0.8
```

See [Sound Alerts](../customization/sounds.md).

## Redirects

Route matching lines to another window:

```toml
[loot_lines]
pattern = "^You gather"
redirect_to = "loot"
redirect_mode = "copy"    # also keep it in the original window
```

## Text Replacement

```toml
[shorten_deaths]
pattern = "The death cry of (\\w+)"
replace = "† $1"
fg = "#ff0000"
```

## Testing

Inject a fake game line to test your patterns without waiting for the game:

```
.testline You are stunned for 3 rounds!
```

## Global Toggles

Disable whole features without deleting patterns, in `config.toml`:

```toml
[highlights]
sounds_enabled = true
replace_enabled = true
redirect_enabled = true
coloring_enabled = true
```

System highlights (monsterbold, links, room names) are not affected by
these toggles.

## Highlight Profiles

Save and swap whole highlight sets:

```
.savehighlights hunting
.loadhighlights hunting
.highlightprofiles        # list saved profiles
```

## Importing from Wrayth/StormFront

Convert an existing Wrayth or StormFront settings file:

```bash
vellum-fe import-highlights settings.xml --out my-highlights.toml
```
