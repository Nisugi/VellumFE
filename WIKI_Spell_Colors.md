# Spell Coloring

Customize the color of active spells/effects in your Active Effects widgets based on spell ID.

## Overview

The spell coloring system allows you to assign colors to spells based on their numeric IDs. This makes it easy to visually distinguish between different spell circles (Cleric, Wizard, Sorcerer, etc.) at a glance.

## Configuration

Spell colors are configured in `~/.profanity-rs/config.toml` under the `[[spell_colors]]` section.

### Format

```toml
[[spell_colors]]
spells = [101, 107, 120, 140, 150]  # List of spell IDs
color = "#00bfff"                   # Hex color code
```

### How It Works

1. Each `[[spell_colors]]` entry defines a list of spell IDs and their associated color
2. When a spell is active, the system looks up its ID in the configured lists
3. The first matching list's color is applied to the spell's progress bar
4. If no match is found, the default bar color is used

### Complete Example

```toml
# Minor Spirit spells (100 series) - Sky blue
[[spell_colors]]
spells = [101, 107, 120, 125, 130, 140, 150, 175]
color = "#00bfff"

# Cleric spells (300 series) - Yellow
[[spell_colors]]
spells = [303, 307, 310, 313, 315, 317, 318, 319, 325, 330, 335, 340]
color = "#ffd700"

# Minor Elemental (400 series) - Light blue
[[spell_colors]]
spells = [401, 406, 414, 419, 430, 435]
color = "#87ceeb"

# Major Elemental (500 series) - Dark blue
[[spell_colors]]
spells = [503, 506, 507, 508, 509, 513, 520, 525, 530, 540]
color = "#4169e1"

# Ranger spells (600 series) - Green
[[spell_colors]]
spells = [601, 602, 605, 606, 608, 613, 616, 618, 625, 630, 640]
color = "#32cd32"

# Sorcerer spells (700 series) - Red
[[spell_colors]]
spells = [701, 703, 705, 708, 712, 713, 715, 720, 725, 730, 735, 740]
color = "#ff4500"

# Wizard spells (900 series) - Purple
[[spell_colors]]
spells = [905, 911, 913, 918, 919, 920, 925, 930, 940]
color = "#9370db"

# Bard spells (1000 series) - Orange
[[spell_colors]]
spells = [1001, 1003, 1006, 1010, 1012, 1019, 1025, 1030, 1035, 1040]
color = "#ff8c00"

# Empath spells (1100 series) - Cyan
[[spell_colors]]
spells = [1101, 1107, 1109, 1115, 1120, 1125, 1130, 1140, 1150]
color = "#00ffff"

# Paladin spells (1600 series) - Pink
[[spell_colors]]
spells = [1601, 1602, 1605, 1610, 1615, 1617, 1618, 1625, 1630, 1635]
color = "#ff69b4"
```

## Customizing Your Colors

### Finding Spell IDs

Spell IDs are numeric identifiers for each spell:
- Minor Spirit: 100-175
- Major Spirit: 200-250
- Cleric: 300-350
- Minor Elemental: 400-450
- Major Elemental: 500-550
- Ranger: 600-650
- Sorcerer: 700-750
- Wizard: 900-950
- Bard: 1000-1050
- Empath: 1100-1150
- Paladin: 1600-1650

You can toggle between spell names and IDs in the Active Effects widget using:
```
.togglespellid activeeffects
```

### Choosing Colors

Colors must be in hex format with the `#` prefix. Examples:
- `#ff0000` - Red
- `#00ff00` - Green
- `#0000ff` - Blue
- `#ffff00` - Yellow
- `#ff00ff` - Magenta
- `#00ffff` - Cyan
- `#ffffff` - White
- `#000000` - Black

Online color pickers can help you choose colors: https://htmlcolorcodes.com/color-picker/

### Organizing by Profession

You can organize spell colors by your character's profession. For example, a Wizard might only configure:

```toml
# My Wizard's commonly used spells

[[spell_colors]]
spells = [905, 911, 913, 918, 919, 920, 925, 930, 940]  # Wizard
color = "#9370db"

[[spell_colors]]
spells = [503, 506, 509, 513, 520, 525]  # Major Elemental
color = "#4169e1"

[[spell_colors]]
spells = [401, 406, 414, 419, 430, 435]  # Minor Elemental
color = "#87ceeb"
```

### Highlighting Important Spells

You can create separate entries to highlight specific important spells:

```toml
# Defensive spells - Blue
[[spell_colors]]
spells = [101, 107, 202, 215, 219]
color = "#0000ff"

# Offensive spells - Red
[[spell_colors]]
spells = [410, 415, 505, 525, 540]
color = "#ff0000"

# Utility spells - Green
[[spell_colors]]
spells = [401, 403, 404, 507, 511]
color = "#00ff00"

# Buffs - Yellow
[[spell_colors]]
spells = [120, 140, 406, 414, 503]
color = "#ffff00"
```

## Overlapping Spell Lists

If a spell appears in multiple `[[spell_colors]]` entries, the **first matching entry** wins.

```toml
# This takes priority
[[spell_colors]]
spells = [120]  # Highlight Lesser Shroud specifically
color = "#ff0000"  # Red

# This won't apply to spell 120 (already matched above)
[[spell_colors]]
spells = [101, 107, 120, 125, 130]  # Minor Spirit general
color = "#00bfff"  # Blue
```

## Performance Notes

- Lookup is O(n) where n = total spells across all lists
- Typical configs with 50-100 spell IDs are very fast
- First match returns immediately (early exit)
- Sparse spell lists (common in actual gameplay) are optimal

## Troubleshooting

### Colors not showing

**Problem**: Spells appear in default color

**Solutions**:
1. Check that spell IDs are correct (use `.togglespellid` to verify)
2. Verify hex color format includes `#` prefix
3. Ensure config file syntax is correct (no missing commas, brackets)
4. Restart profanity-rs after editing config

### Wrong color applied

**Problem**: Spell shows unexpected color

**Solution**: Check for overlapping spell lists - first match wins. Reorder `[[spell_colors]]` entries to prioritize specific colors.

### Config not loading

**Problem**: Changes to config not taking effect

**Solution**:
1. Check `~/.profanity-rs/debug.log` for config parsing errors
2. Verify TOML syntax (use online TOML validator)
3. Restart profanity-rs to reload config

## Default Configuration

profanity-rs comes with pre-configured spell colors for all major spell circles. These are just examples - customize them to your preferences!

The defaults provide:
- Distinct colors for each spell circle
- Coverage of commonly used spells
- Good visual contrast for quick identification

You can completely replace or extend the default configuration in your `~/.profanity-rs/config.toml` file.

## Related Commands

```
.togglespellid <window>     # Toggle between spell names and IDs
.createwindow buffs         # Create buffs widget
.createwindow debuffs       # Create debuffs widget
.createwindow cooldowns     # Create cooldowns widget
.createwindow active_spells # Create all active spells widget
```

## See Also

- [Active Effects Widget](Widget-Reference.md#active-effects)
- [Configuration Guide](Configuration-Guide.md)
- [Commands Reference](Commands-Reference.md)
