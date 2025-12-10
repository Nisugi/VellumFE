# Migrating from Profanity

Guide for Profanity users transitioning to VellumFE.

## Overview

Profanity and VellumFE are both terminal-based clients. Many concepts translate directly, though configuration syntax differs.

## Key Similarities

| Feature | Profanity | VellumFE |
|---------|-----------|----------|
| Terminal-based | ✓ | ✓ |
| Lich support | ✓ | ✓ |
| Text windows | ✓ | ✓ |
| Macros | ✓ | ✓ |
| Triggers | ✓ | ✓ |
| Highlights | ✓ | ✓ |

## Key Differences

| Aspect | Profanity | VellumFE |
|--------|-----------|----------|
| Config format | Custom format | TOML |
| Language | Ruby | Rust |
| Widget system | Fixed types | Flexible types |
| Layout | Relative | Percentage-based |

## Configuration Translation

### Window Layout

**Profanity** (in configuration):
```
window main 0 0 80 20
window health 81 0 20 3
```

**VellumFE** (`layout.toml`):
```toml
[[widgets]]
type = "text"
name = "main"
x = 0
y = 0
width = 80
height = 75

[[widgets]]
type = "progress"
name = "health"
x = 81
y = 0
width = 19
height = 5
data_source = "vitals.health"
```

### Highlights

**Profanity**:
```
highlight "stunned" bold yellow
highlight /whispers,/ magenta
```

**VellumFE** (`highlights.toml`):
```toml
[[highlights]]
pattern = "stunned"
fg = "bright_yellow"
bold = true

[[highlights]]
pattern = "whispers,"
fg = "magenta"
```

### Macros

**Profanity**:
```
macro F1 attack target
macro ctrl-1 "prep 101;cast"
```

**VellumFE** (`keybinds.toml`):
```toml
[keybinds."f1"]
macro = "attack target"

[keybinds."ctrl+1"]
macro = "prep 101;cast"
```

### Triggers

**Profanity**:
```
trigger "You are stunned" echo "STUNNED!"
trigger /falls dead/ "search;loot"
```

**VellumFE** (`triggers.toml`):
```toml
[[triggers]]
pattern = "You are stunned"
command = ".notify STUNNED!"

[[triggers]]
pattern = "falls dead"
command = "search;loot"
```

### Colors

**Profanity**:
```
color health green
color mana blue
color background black
```

**VellumFE** (`colors.toml`):
```toml
[theme]
health = "#00ff00"
mana = "#0080ff"
background = "#000000"
```

## Lich Integration

Both clients connect through Lich the same way. Your Lich scripts continue to work:

```bash
# Same connection method
vellum-fe --host 127.0.0.1 --port 8000
```

## Feature Comparison

### Text Windows

| Profanity | VellumFE |
|-----------|----------|
| `main` window | `text` widget with `streams = ["main"]` |
| `thoughts` window | `text` widget with `streams = ["thoughts"]` |
| Multiple windows | Multiple `text` widgets |

### Progress Bars

| Profanity | VellumFE |
|-----------|----------|
| Built-in health bar | `progress` widget with `data_source = "vitals.health"` |
| Fixed appearance | Customizable colors and style |

### Compass

| Profanity | VellumFE |
|-----------|----------|
| ASCII compass | `compass` widget with `style = "ascii"` or `"unicode"` |
| Fixed location | Any position |

## Step-by-Step Migration

### 1. Document Your Profanity Setup

List:
- Window positions
- Macros
- Triggers
- Highlights
- Color preferences

### 2. Create VellumFE Config Directory

```bash
mkdir -p ~/.vellum-fe
```

### 3. Create Layout

Translate window positions to `layout.toml`:

```toml
# Approximate your Profanity layout

[[widgets]]
type = "text"
name = "main"
x = 0
y = 0
width = 80
height = 80
streams = ["main", "room"]

# Add other widgets...
```

### 4. Create Colors

Translate color preferences:

```toml
[theme]
background = "#000000"
text = "#c0c0c0"
health = "#00ff00"
# Add more...
```

### 5. Migrate Macros

```toml
# keybinds.toml

[keybinds."f1"]
macro = "attack target"

[keybinds."f2"]
macro = "hide"

# Add more...
```

### 6. Migrate Triggers

```toml
# triggers.toml

[[triggers]]
name = "stun_alert"
pattern = "You are stunned"
command = ".notify STUNNED!"

# Add more...
```

### 7. Migrate Highlights

```toml
# highlights.toml

[[highlights]]
pattern = "stunned"
fg = "bright_yellow"
bold = true

# Add more...
```

## Common Adjustments

### Layout Units

Profanity uses character columns/rows.
VellumFE uses percentages (0-100).

**Converting**:
- Estimate percentage of screen
- Or: `(columns / terminal_width) * 100`

### Regex Patterns

Both support regex, but syntax may vary:
- Test patterns in VellumFE
- Adjust escaping as needed

### Macro Syntax

| Profanity | VellumFE |
|-----------|----------|
| `;` separates commands | `;` separates commands |
| `$input` for prompt | `$input` for prompt |
| Delays | `{ms}` for delays |

## What's New in VellumFE

Features you might not have in Profanity:

- **Tabbed windows**: Multiple tabs in one widget
- **Unicode compass**: Modern arrow characters
- **Percentage layout**: Adapts to terminal size
- **Active effects widget**: Track spell durations
- **Browser system**: Popup editors

## Troubleshooting

### Macros Not Working

- Check key name format (`"f1"` not `"F1"`)
- Verify TOML syntax (quotes around keys with special chars)

### Colors Different

- VellumFE uses hex colors or preset names
- Verify color name mappings

### Layout Looks Wrong

- Check percentage calculations
- Widgets shouldn't overlap (x + width ≤ 100)

## See Also

- [Your First Layout](../tutorials/your-first-layout.md)
- [Configuration Reference](../configuration/README.md)
- [Keybinds](../configuration/keybinds-toml.md)

