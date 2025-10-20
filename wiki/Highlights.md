# Highlights Guide

VellumFE's highlight system allows you to color-code and add sounds to specific game text patterns using regular expressions. This guide covers creating, managing, and optimizing highlights.

## What are Highlights?

Highlights are rules that match text patterns and apply custom styling:

- **Foreground color** - Text color
- **Background color** - Background highlight
- **Bold** - Bold text
- **Sounds** - Audio alerts
- **Entire line** - Color whole line vs just matched text

## Managing Highlights

### Opening the Highlights Browser

```bash
.highlights
.listhl        # Alias
```

**Features:**
- Grouped by category with yellow headers
- Color previews `[#RRGGBB]` for foreground/background
- Sound indicator ♫ for highlights with audio
- Sorted by category then name

**Navigation:**
- `↑/↓` - Navigate highlights
- `PgUp/PgDn` - Page up/down
- `Enter` - Edit selected highlight
- `Delete` - Delete selected highlight
- `Esc` - Close browser

### Creating Highlights

```bash
.addhl
```

Opens the highlight form. You can also press `Enter` from the highlights browser to edit an existing one.

### Highlight Form Fields

**Name** (required)
- Unique identifier for this highlight
- Used to reference the highlight
- Example: `my_attacks`, `arrivals`, `lnet`

**Pattern** (required)
- Regular expression to match text
- Case-sensitive by default
- Examples:
  ```regex
  ^You (?:swing|thrust|slice)       # Your attacks
  arrives?$                          # Arrivals
  ^\[LNet\]                         # LNet messages
  ^\w+ just arrived                 # Player arrivals
  ```

**Category** (optional)
- Groups related highlights in browser
- Examples: `Combat`, `Social`, `System`, `Loot`
- Helps organize large highlight sets

**FG Color** (optional)
- Foreground (text) color in hex: `#RRGGBB`
- Examples: `#ff0000` (red), `#00ff00` (green)
- Leave empty for no color change

**BG Color** (optional)
- Background color in hex: `#RRGGBB`
- Use `-` for transparent/no background
- Examples: `#330000` (dark red), `-` (none)

**Bold** (checkbox)
- Make matched text bold
- Useful for making text stand out

**Color Entire Line** (checkbox)
- **Checked**: Colors the entire line when pattern matches
- **Unchecked**: Only colors the matched text
- Example: Match "LNet" but color the whole line

**Fast Parse** (checkbox)
- Uses Aho-Corasick algorithm for literal string matching
- **Much faster** for simple patterns
- Only use for **exact string matches** (not regex)
- Example: Fast parse for `LNet`, regex for `^LNet.*important`

**Sound File** (optional)
- Path to `.wav` file to play when matched
- Examples:
  - `C:\Sounds\alert.wav` (Windows)
  - `/home/user/sounds/alert.wav` (Linux)
  - `~/sounds/alert.wav` (cross-platform)

**Volume** (0.0 - 1.0)
- Volume for this highlight's sound
- `0.0` = silent, `1.0` = full volume
- Default: `0.5`
- Requires sound file to be set

## Pattern Syntax (Regular Expressions)

Highlights use Rust's `regex` crate, which is similar to PCRE.

### Basic Patterns

**Literal text:**
```regex
LNet
wizard
You swing
```

**Case-insensitive:**
```regex
(?i)lnet
```

**Start of line:**
```regex
^You swing
```

**End of line:**
```regex
arrives?$
```

**Word boundaries:**
```regex
\bwizard\b
```

### Character Classes

**Digit:**
```regex
\d      # Any digit (0-9)
\d+     # One or more digits
\d{2,4} # 2 to 4 digits
```

**Word character:**
```regex
\w      # Letter, digit, or underscore
\w+     # One or more word characters
```

**Whitespace:**
```regex
\s      # Space, tab, newline
\s+     # One or more whitespace
```

**Any character:**
```regex
.       # Any character except newline
.*      # Zero or more of any character
.+      # One or more of any character
```

### Alternation and Grouping

**OR (alternation):**
```regex
swing|thrust|slice
```

**Non-capturing group:**
```regex
(?:swing|thrust|slice)
```

**Optional:**
```regex
arrives?              # "arrive" or "arrives"
colou?r               # "color" or "colour"
```

### Quantifiers

```regex
*       # 0 or more
+       # 1 or more
?       # 0 or 1 (optional)
{n}     # Exactly n
{n,}    # n or more
{n,m}   # Between n and m
```

### Anchors

```regex
^       # Start of line
$       # End of line
\b      # Word boundary
```

## Highlight Examples

### Combat

**Your attacks:**
```
Name: my_attacks
Pattern: ^You (?:swing|thrust|slice|punch|kick|bash)
FG Color: #ffff00
Bold: ✓
Color Entire Line: ✓
```

**Enemy attacks:**
```
Name: enemy_attacks
Pattern: (?:swings|thrusts|slices|punches|kicks|bashes) (?:a|an|his|her) .* at you
FG Color: #ff0000
Bold: ✓
Color Entire Line: ✓
Sound: C:\Sounds\danger.wav
Volume: 0.8
```

**Roundtime:**
```
Name: roundtime_message
Pattern: ^Roundtime: \d+ sec
FG Color: #ff6600
Bold: ✓
```

### Social

**Arrivals:**
```
Name: arrivals
Pattern: arrives?$
FG Color: #00ff00
Category: Social
```

**Departures:**
```
Name: departures
Pattern: (?:went|just left|just went) (?:north|south|east|west|up|down|out)
FG Color: #ff0000
Category: Social
```

**Whispers to you:**
```
Name: whispers
Pattern: whispers?, "
FG Color: #ff00ff
Bold: ✓
Sound: C:\Sounds\whisper.wav
```

### System

**LNet messages:**
```
Name: lnet
Pattern: ^\[LNet\]
FG Color: #00ffff
BG Color: #003333
Color Entire Line: ✓
Category: System
Fast Parse: ✓
```

**Private messages:**
```
Name: pm
Pattern: ^\[Private\]
FG Color: #ffff00
BG Color: #333300
Bold: ✓
Color Entire Line: ✓
Sound: C:\Sounds\pm.wav
Volume: 0.7
```

**Death:**
```
Name: death
Pattern: ^You have been slain!
FG Color: #ff0000
BG Color: #330000
Bold: ✓
Color Entire Line: ✓
Sound: C:\Sounds\death.wav
Volume: 1.0
```

### Loot

**Boxes:**
```
Name: boxes
Pattern: \b(?:box|chest|trunk|coffer)\b
FG Color: #ffaa00
Bold: ✓
Category: Loot
```

**Coins:**
```
Name: coins
Pattern: \b(?:\d+|some) (?:silver|coin)s?\b
FG Color: #cccccc
Bold: ✓
Category: Loot
```

**Magic items:**
```
Name: magic_items
Pattern: \b(?:glowing|pulsing|shimmering|radiating)\b
FG Color: #ff00ff
Bold: ✓
Category: Loot
```

### Character-Specific

**Your name mentioned:**
```
Name: my_name
Pattern: \bNisugi\b
FG Color: #ffff00
Bold: ✓
Sound: C:\Sounds\name.wav
```

**Guild skills:**
```
Name: guild_skills
Pattern: ^You (?:gesture|channel|focus|prepare)
FG Color: #00ffff
Bold: ✓
Category: Skills
```

## Performance Optimization

### Fast Parse vs Regex

**Fast Parse (Aho-Corasick):**
- ✓ Much faster for literal strings
- ✓ Multiple patterns matched simultaneously
- ✗ No regex features (no `^`, `$`, `\d`, etc.)

**Use Fast Parse for:**
```
LNet
[Private]
wizard
arrives
```

**Use Regex for:**
```
^You swing
arrives?$
\d+ silver
(?:swing|thrust|slice)
```

### Pattern Efficiency

**Efficient patterns:**
```regex
^You swing               # Anchored at start
arrives?$                # Anchored at end
\bwizard\b               # Word boundaries
```

**Inefficient patterns:**
```regex
.*wizard.*               # Matches entire line
(?:.*){2,}               # Backtracking
([^"]*"[^"]*){3,}        # Complex backtracking
```

### Highlight Organization

1. **Use categories** - Group related highlights for easier management
2. **Name consistently** - Use prefixes like `combat_`, `social_`, `loot_`
3. **Avoid duplicates** - One pattern can match multiple things with alternation
4. **Test patterns** - Verify patterns match what you expect

## Highlight Priority

When multiple highlights match the same text:
- Last matching highlight wins
- Use specific patterns before general ones
- Order matters in config file

**Example conflict:**
```
Highlight 1: ^You .*          # Matches all "You" lines
Highlight 2: ^You swing       # More specific

Result: "You swing" will use Highlight 2 (if processed after)
```

## Configuration File Format

Highlights are stored in `~/.vellum-fe/configs/<character>.toml`:

```toml
[[highlights]]
name = "my_attacks"
pattern = "^You (?:swing|thrust|slice)"
category = "Combat"
fg_color = "#ffff00"
bg_color = "-"
bold = true
color_entire_line = true
fast_parse = false
sound_file = ""
volume = 0.5
```

## Troubleshooting

### Highlight Not Matching

1. **Test pattern** - Use a regex tester (regex101.com)
2. **Check case** - Patterns are case-sensitive by default
3. **Escape special chars** - Use `\` for `.`, `*`, `+`, `?`, etc.
4. **Verify window** - Highlight only applies to windows receiving that stream

### Pattern Matching Too Much

1. **Add anchors** - Use `^` and `$` for line boundaries
2. **Use word boundaries** - `\b` prevents partial matches
3. **Be more specific** - Add more context to pattern

### Highlight Not Showing Color

1. **Check window receives stream** - Highlight only applies to subscribed windows
2. **Verify color format** - Must be `#RRGGBB`
3. **Check terminal colors** - Some terminals have limited color support
4. **Priority issue** - Later highlights override earlier ones

### Sound Not Playing

1. **Verify file path** - Must be absolute path or relative to VellumFE directory
2. **Check file format** - Must be `.wav` file
3. **Sound enabled** - Check `.settings` → Sound → Enabled
4. **Volume too low** - Increase highlight volume or master volume

### Performance Issues

1. **Use Fast Parse** - For literal strings, enable Fast Parse
2. **Simplify patterns** - Avoid complex regex with backtracking
3. **Reduce highlight count** - Too many highlights can slow parsing
4. **Profile patterns** - Test which patterns are slow

## Best Practices

1. **Start simple** - Begin with basic patterns, add complexity as needed
2. **Test incrementally** - Add one highlight at a time
3. **Use categories** - Organize highlights by purpose
4. **Document patterns** - Add comments in config file
5. **Share configs** - Export and share highlight sets with community
6. **Backup regularly** - Save your highlights config before major changes
7. **Character-specific** - Use `--character` for different highlight sets per character

## See Also

- [Configuration](Configuration.md) - Config file format
- [Commands Reference](Commands.md) - Highlight commands
- [Advanced Streams](Advanced-Streams.md) - Stream routing for highlights
- [Regex Tutorial](https://www.regular-expressions.info/tutorial.html) - Learn regex
