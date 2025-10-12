# Highlight Management

VellumFE includes a powerful highlight system that allows you to colorize and style text matching specific patterns in game output. Highlights can use regex patterns for flexible matching, apply custom colors and styling, play sounds, and even use optimized Aho-Corasick matching for literal text.

## Table of Contents

- [Overview](#overview)
- [Opening the Highlight Form](#opening-the-highlight-form)
- [Form Fields](#form-fields)
- [Navigation](#navigation)
- [Saving and Managing Highlights](#saving-and-managing-highlights)
- [Dot Commands](#dot-commands)
- [Configuration File Format](#configuration-file-format)
- [Examples](#examples)
- [Tips and Best Practices](#tips-and-best-practices)

## Overview

Highlights in VellumFE allow you to:
- Match text using **regex patterns** or **literal text**
- Apply **foreground and background colors** to matched text
- Make text **bold**
- Color the **entire line** containing a match (not just the matched text)
- Play **sounds** when patterns match (with custom volume)
- Use **fast Aho-Corasick matching** for literal text patterns (much faster than regex)

All highlights are stored in your character-specific config file and can be managed through an interactive TUI form or via dot commands.

## Opening the Highlight Form

There are two ways to open the highlight management form:

### Create a New Highlight
Type `.addhighlight` or `.addhl` in the command input:

```
.addhighlight
```

This opens an empty form where you can define a new highlight.

### Edit an Existing Highlight
Type `.edithighlight <name>` or `.edithl <name>` where `<name>` is the highlight name:

```
.edithighlight swing
```

This opens the form pre-filled with the existing highlight's settings.

## Form Fields

The highlight form contains the following fields:

### Name
**Required field**. The unique identifier for this highlight. Used to edit or delete the highlight later.

- Must be unique across all your highlights
- Alphanumeric characters, underscores, and hyphens recommended
- Example: `combat_swing`, `player_names`, `treasure`

### Pattern
**Required field**. The regex pattern or literal text to match in game output.

- Uses Rust regex syntax (similar to Perl/PCRE)
- Real-time validation shows errors immediately
- Examples:
  - Regex: `You swing.*at the` (matches "You swing a sword at the kobold")
  - Literal: `Mandrill|Monolis|Chiora` (when using fast_parse)

**Common Regex Patterns:**
- `.*` - Match any characters (greedy)
- `\d+` - Match one or more digits
- `\w+` - Match word characters (letters, numbers, underscore)
- `^` - Start of line
- `$` - End of line
- `(group1|group2)` - Match either group1 or group2

### Foreground Color
The text color for matched text. Use hex color format: `#RRGGBB`

- Optional (leave blank for no color change)
- Live color preview shows the color as you type
- Examples: `#ff0000` (red), `#00ff00` (green), `#ffff00` (yellow)

### Background Color
The background color for matched text. Use hex color format: `#RRGGBB`

- Optional (leave blank for no background)
- Live color preview shows the color as you type
- Examples: `#000000` (black), `#ffffff` (white)

### Bold
Checkbox. When checked, matched text is displayed in bold.

- Use for important highlights that need to stand out
- Works in combination with colors

### Color Entire Line
Checkbox. When checked, colors the entire line containing a match (not just the matched text).

- Useful for highlighting entire combat messages
- Applies foreground and background colors to the whole line

### Fast Parse (Aho-Corasick)
Checkbox. When checked, uses optimized literal string matching instead of regex.

- **Much faster** than regex for literal text matching
- Split pattern on `|` to match multiple literal strings
- Example: `Mandrill|Monolis|Chiora` matches any of those three names
- **Limitations**: Only matches literal text (no regex features like `.*`, `\d+`, etc.)
- Recommended for: Player names, item names, simple text matching

### Sound
Optional. Path to a sound file to play when the pattern matches.

- Relative to VellumFE directory or absolute path
- Supported formats: WAV, MP3, OGG (via rodio)
- Example: `sounds/combat/sword_swing.wav`

### Sound Volume
Optional. Volume level for the sound (0.0 = silent, 1.0 = full volume).

- Decimal number between 0.0 and 1.0
- Default: Uses global volume setting if not specified
- Example: `0.8` (80% volume)

## Navigation

### Keyboard Navigation
- **Tab** - Move to next field/button
- **Shift+Tab** - Move to previous field/button
- **Space** - Toggle checkboxes (when focused on checkbox)
- **Enter** - Activate button (Save/Cancel/Delete)
- **Esc** - Close form without saving
- **Arrow keys** - Move cursor within text fields (when focused)
- **Home/End** - Jump to start/end of text field
- **Backspace/Delete** - Edit text in fields

### Visual Indicators
- **Focused text fields**: Yellow border
- **Unfocused text fields**: Dark gray border
- **Focused checkboxes**: Yellow text with bold
- **Focused buttons**: Inverted colors (e.g., black text on green background)
- **Invalid regex**: Red error message below pattern field
- **Color previews**: Small colored boxes next to color fields

## Saving and Managing Highlights

### Save Button
Press **Enter** when focused on the Save button (or Tab until it's highlighted and press Enter).

- Validates all fields before saving
- Shows error if name or pattern is empty
- Shows error if pattern is invalid regex
- Saves to character-specific config file
- Automatically reloads highlights in all windows
- Shows confirmation message: "Highlight 'name' saved"

### Cancel Button
Press **Enter** when focused on the Cancel button (or press **Esc** anywhere).

- Closes form without saving
- Discards all changes

### Delete Button
Press **Enter** when focused on the Delete button (only shown in Edit mode).

- Removes the highlight from config
- Shows confirmation message: "Highlight 'name' deleted"
- Cannot be undone (except by manually re-creating the highlight)

## Dot Commands

VellumFE provides several dot commands for highlight management:

### Create New Highlight
```
.addhighlight
.addhl
```
Opens the highlight form in Create mode.

### Edit Existing Highlight
```
.edithighlight <name>
.edithl <name>
```
Opens the highlight form in Edit mode with the specified highlight loaded.

**Example:**
```
.edithl combat_swing
```

### Delete Highlight
```
.deletehighlight <name>
.delhl <name>
```
Immediately deletes the specified highlight (no confirmation prompt).

**Example:**
```
.delhl old_highlight
```

### List All Highlights
```
.listhighlights
.listhl
.highlights
```
Shows a count and comma-separated list of all configured highlights.

**Example output:**
```
5 highlights: combat_swing, player_names, treasure, death_message, poison_warning
```

## Configuration File Format

Highlights are stored in your character-specific config file at:
```
~/.vellum-fe/configs/<character>.toml
```

### Example Configuration
```toml
[highlights]

# Combat highlight with sound
swing = { pattern = "You swing.*at the", fg = "#ff0000", bold = true, sound = "sounds/sword.wav", sound_volume = 0.8 }

# Player name highlights with fast Aho-Corasick matching
friends = { pattern = "Mandrill|Monolis|Chiora|Exdeo|Kawhi|Aldhelm", fg = "#ff00ff", bold = true, fast_parse = true }

# Full-line highlight for deaths
death = { pattern = "has been slain", fg = "#ffffff", bg = "#ff0000", color_entire_line = true }

# Treasure highlight
treasure = { pattern = "\\b(gold|silver|gems?)\\b", fg = "#ffff00", bold = true }

# Simple text highlight (no colors, just for matching)
poison = { pattern = "You have been poisoned!" }
```

### Field Reference
- `pattern` (string, required) - Regex pattern or literal text to match
- `fg` (string, optional) - Foreground color in hex format (`#RRGGBB`)
- `bg` (string, optional) - Background color in hex format (`#RRGGBB`)
- `bold` (bool, default: false) - Apply bold styling
- `color_entire_line` (bool, default: false) - Color the entire line
- `fast_parse` (bool, default: false) - Use Aho-Corasick for literal matching
- `sound` (string, optional) - Path to sound file
- `sound_volume` (f32, optional) - Volume level (0.0-1.0)

## Examples

### Example 1: Combat Highlight
Highlight your combat swings in red with a sound effect:

**Name:** `my_swings`
**Pattern:** `^You swing`
**Foreground:** `#ff0000`
**Bold:** ☑
**Sound:** `sounds/sword_swing.wav`
**Sound Volume:** `0.7`

### Example 2: Player Names (Fast)
Highlight friend names in magenta using fast literal matching:

**Name:** `friend_names`
**Pattern:** `Mandrill|Monolis|Chiora|Exdeo`
**Foreground:** `#ff00ff`
**Bold:** ☑
**Fast Parse:** ☑

### Example 3: Death Messages
Highlight death messages with red background on entire line:

**Name:** `death_alerts`
**Pattern:** `has been slain|falls to the ground`
**Foreground:** `#ffffff`
**Background:** `#aa0000`
**Color Entire Line:** ☑

### Example 4: Treasure Loot
Highlight mentions of valuable items:

**Name:** `treasure_loot`
**Pattern:** `\b(gold|silver|platinum|gems?|jewel|diamond)\b`
**Foreground:** `#ffff00`
**Bold:** ☑

### Example 5: Poison Warning
Full-line highlight with background color and sound alert:

**Name:** `poison_warning`
**Pattern:** `poisoned`
**Foreground:** `#00ff00`
**Background:** `#004400`
**Color Entire Line:** ☑
**Sound:** `sounds/poison_alert.wav`
**Sound Volume:** `1.0`

### Example 6: Room Arrivals
Highlight when specific players arrive in your room:

**Name:** `vip_arrivals`
**Pattern:** `(Lord|Lady) \w+ (arrives|just arrived)`
**Foreground:** `#00ffff`
**Bold:** ☑

## Tips and Best Practices

### Performance Tips
1. **Use `fast_parse` for literal text** - Much faster than regex for simple string matching
2. **Avoid complex regex** - Simple patterns perform better
3. **Limit the number of highlights** - Too many can impact performance
4. **Test patterns** - Use `.edithighlight` to verify patterns match correctly

### Pattern Tips
1. **Anchor patterns** - Use `^` (start) and `$` (end) to make matches more specific
2. **Escape special characters** - Use `\` before regex special chars: `. * + ? [ ] ( ) { } | \`
3. **Case sensitivity** - Patterns are case-sensitive by default
4. **Word boundaries** - Use `\b` to match whole words only

### Color Tips
1. **Contrast matters** - Ensure text is readable against background
2. **Test on your terminal** - Colors may look different in different terminals
3. **Use preview** - The color preview box shows exactly what the color will look like
4. **Common colors**:
   - Red: `#ff0000`
   - Green: `#00ff00`
   - Blue: `#0000ff`
   - Yellow: `#ffff00`
   - Cyan: `#00ffff`
   - Magenta: `#ff00ff`
   - White: `#ffffff`
   - Gray: `#808080`

### Sound Tips
1. **Use short sounds** - Long audio clips can be annoying
2. **Adjust volume** - Start around 0.5-0.8 and adjust to taste
3. **Supported formats** - WAV is most compatible, MP3 and OGG also work
4. **File paths** - Use forward slashes even on Windows: `sounds/alert.wav`

### Organization Tips
1. **Use descriptive names** - Makes highlights easier to manage later
2. **Group by category** - Use prefixes like `combat_`, `loot_`, `social_`
3. **Document complex patterns** - Add comments in config file
4. **Back up your config** - Highlights are stored in `~/.vellum-fe/configs/`

## Troubleshooting

### Pattern doesn't match
- Check regex syntax - use online regex tester
- Verify case sensitivity
- Check for escaped characters (`\` before special chars)
- Test with simpler pattern first

### Colors don't show
- Verify hex format: `#RRGGBB` (must include `#`)
- Check terminal color support
- Try different colors
- Verify foreground/background aren't the same

### Sound doesn't play
- Check file path is correct
- Verify file format is supported (WAV, MP3, OGG)
- Check volume is not 0.0
- Verify sound system is working (test with other highlights)

### Form won't save
- Check name field is not empty
- Check pattern field is not empty
- Check pattern is valid regex (no red error message)
- Check for duplicate name (names must be unique)

### Can't see text input
- Fields must have height 3 to display properly
- Check terminal size (minimum 80x40 recommended)
- Try toggling focus (Tab through fields)

## Related Documentation

- [Window Management](https://github.com/Nisugi/VellumFE/wiki/Window-Management) - Managing text windows that display highlights

## See Also

- [Aho-Corasick Algorithm](https://en.wikipedia.org/wiki/Aho%E2%80%93Corasick_algorithm) - Fast literal string matching
- [Rust Regex Syntax](https://docs.rs/regex/latest/regex/#syntax) - Full regex pattern reference
- [Hex Color Picker](https://www.google.com/search?q=color+picker) - Find hex codes for colors
