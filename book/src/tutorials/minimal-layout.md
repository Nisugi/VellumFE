# Minimal Layout

Create a clean, distraction-free interface with maximum text visibility.

## Goal

Build a minimalist layout with:

- Maximum screen space for game text
- Essential information only
- Keyboard-centric design
- Low resource usage
- Quick load times

## Prerequisites

- Basic VellumFE familiarity
- Preference for simple interfaces
- Keyboard navigation skills

## Design Philosophy

The minimal layout follows these principles:

1. **Less is More** - Only show what you actively need
2. **Text First** - Game output takes priority
3. **Keyboard Driven** - Reduce mouse dependency
4. **Performance** - Fewer widgets = faster rendering

## Layout Overview

```
┌────────────────────────────────────────────────────────────┐
│                                                            │
│                                                            │
│                                                            │
│                     Game Text                              │
│                   (Full Screen)                            │
│                                                            │
│                                                            │
│                                                            │
├────────────────────────────────────────────────────────────┤
│ HP: ████████░░ 82%  MP: ████░░░░░░ 40%  RT: 3  > [input]   │
└────────────────────────────────────────────────────────────┘
```

## Step 1: Create the Layout

Create `~/.vellum-fe/layout.toml`:

```toml
# Minimal Layout - Maximum Text, Minimum UI
# Clean, fast, keyboard-driven

# ═══════════════════════════════════════════════════════════
# MAIN TEXT - Full Width, Maximum Height
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "text"
name = "main"
title = ""
x = 0
y = 0
width = 100
height = 90
streams = ["main", "room", "combat", "thoughts", "speech"]
scrollback = 3000
auto_scroll = true
border = false
padding = 1

# ═══════════════════════════════════════════════════════════
# STATUS BAR - Compact Bottom Row
# ═══════════════════════════════════════════════════════════

# Health (compact)
[[widgets]]
type = "progress"
name = "health"
title = "HP"
x = 0
y = 91
width = 15
height = 3
data_source = "vitals.health"
color = "health"
show_percentage = true
border = false

# Mana (compact)
[[widgets]]
type = "progress"
name = "mana"
title = "MP"
x = 16
y = 91
width = 15
height = 3
data_source = "vitals.mana"
color = "mana"
show_percentage = true
border = false

# Roundtime (compact)
[[widgets]]
type = "countdown"
name = "roundtime"
title = "RT"
x = 32
y = 91
width = 8
height = 3
data_source = "roundtime"
border = false

# Command Input (rest of row)
[[widgets]]
type = "command_input"
name = "input"
x = 41
y = 91
width = 59
height = 9
history_size = 500
prompt = "> "
border = false
```

That's it. Four widgets total.

## Step 2: Minimal Theme

Create `~/.vellum-fe/colors.toml`:

```toml
[theme]
name = "Minimal"

# Clean, high-contrast colors
background = "#000000"
text = "#cccccc"
text_dim = "#666666"

# No visible borders
border = "#000000"
border_focused = "#333333"

# Vitals - visible but not distracting
health = "#00aa00"
health_low = "#aaaa00"
health_critical = "#aa0000"
mana = "#0066aa"
stamina = "#aa6600"

# Status bar background
status_bg = "#111111"
```

## Step 3: Keyboard-Centric Keybinds

Create `~/.vellum-fe/keybinds.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# CORE NAVIGATION - Always Available
# ═══════════════════════════════════════════════════════════

# Cardinal directions
[keybinds."numpad8"]
macro = "north"

[keybinds."numpad2"]
macro = "south"

[keybinds."numpad4"]
macro = "west"

[keybinds."numpad6"]
macro = "east"

# Diagonal
[keybinds."numpad7"]
macro = "northwest"

[keybinds."numpad9"]
macro = "northeast"

[keybinds."numpad1"]
macro = "southwest"

[keybinds."numpad3"]
macro = "southeast"

# Special movement
[keybinds."numpad5"]
macro = "out"

[keybinds."numpad_plus"]
macro = "go"

[keybinds."numpad_minus"]
macro = "climb"

# ═══════════════════════════════════════════════════════════
# ESSENTIAL COMMANDS - Function Keys
# ═══════════════════════════════════════════════════════════

[keybinds."f1"]
macro = "look"

[keybinds."f2"]
macro = "inventory"

[keybinds."f3"]
macro = "experience"

[keybinds."f4"]
macro = "health"

[keybinds."f5"]
macro = "stance defensive"

[keybinds."f6"]
macro = "stance offensive"

[keybinds."f7"]
macro = "hide"

[keybinds."f8"]
macro = "search"

# ═══════════════════════════════════════════════════════════
# QUICK ACTIONS - Ctrl Keys
# ═══════════════════════════════════════════════════════════

[keybinds."ctrl+a"]
macro = "attack target"

[keybinds."ctrl+s"]
macro = "search;loot"

[keybinds."ctrl+g"]
macro = "get $input"

[keybinds."ctrl+d"]
macro = "drop $input"

# ═══════════════════════════════════════════════════════════
# SCROLLING - Page Keys
# ═══════════════════════════════════════════════════════════

[keybinds."page_up"]
action = "scroll_up"

[keybinds."page_down"]
action = "scroll_down"

[keybinds."home"]
action = "scroll_top"

[keybinds."end"]
action = "scroll_bottom"

# Half-page scroll
[keybinds."ctrl+page_up"]
action = "scroll_half_up"

[keybinds."ctrl+page_down"]
action = "scroll_half_down"

# ═══════════════════════════════════════════════════════════
# FOCUS
# ═══════════════════════════════════════════════════════════

[keybinds."escape"]
action = "focus_input"

# Toggle between main and input (since only 2 widgets)
[keybinds."tab"]
action = "next_widget"
```

## Step 4: Minimal Highlights

Create `~/.vellum-fe/highlights.toml`:

```toml
# Minimal highlighting - only essential patterns

# ═══════════════════════════════════════════════════════════
# CRITICAL ALERTS
# ═══════════════════════════════════════════════════════════

[[highlights]]
pattern = "(?i)you are stunned"
fg = "black"
bg = "yellow"
bold = true

[[highlights]]
pattern = "(?i)you have died"
fg = "black"
bg = "red"
bold = true

# ═══════════════════════════════════════════════════════════
# PLAYER INTERACTION
# ═══════════════════════════════════════════════════════════

# Whispers (important)
[[highlights]]
pattern = "(\\w+) whispers,"
fg = "magenta"

# Your name mentioned
[[highlights]]
pattern = "\\bYOURNAME\\b"
fg = "cyan"
bold = true

# ═══════════════════════════════════════════════════════════
# COMBAT (Subtle)
# ═══════════════════════════════════════════════════════════

# Hits
[[highlights]]
pattern = "\\*\\*.+\\*\\*"
fg = "red"

# Death
[[highlights]]
pattern = "falls dead"
fg = "yellow"

# ═══════════════════════════════════════════════════════════
# NAVIGATION AIDS
# ═══════════════════════════════════════════════════════════

# Room name
[[highlights]]
pattern = "^\\[.+\\]$"
fg = "white"
bold = true

# Exits
[[highlights]]
pattern = "Obvious (exits|paths):"
fg = "gray"
```

## Step 5: No Triggers (Optional)

For truly minimal setup, skip triggers entirely. Or add only critical ones:

Create `~/.vellum-fe/triggers.toml`:

```toml
# Only life-saving triggers

[[triggers]]
name = "death_alert"
pattern = "You have died"
command = ".notify DEAD!"
priority = 100

[[triggers]]
name = "stun_alert"
pattern = "(?i)you are stunned"
command = ".notify Stunned"
priority = 100
cooldown = 2000
```

## Alternative: Ultra-Minimal

For absolute minimalism, use just two widgets:

```toml
# Ultra-minimal - text and input only

[[widgets]]
type = "text"
name = "main"
x = 0
y = 0
width = 100
height = 92
streams = ["main", "room", "combat", "thoughts", "speech"]
scrollback = 2000
border = false

[[widgets]]
type = "command_input"
name = "input"
x = 0
y = 93
width = 100
height = 7
prompt = "> "
border = false
```

No status bars, no indicators. Pure text adventure.

## Alternative: Vertical Split

If you want some separation without complexity:

```toml
# Two-column minimal

[[widgets]]
type = "text"
name = "main"
x = 0
y = 0
width = 70
height = 92
streams = ["main", "room", "combat"]
border = false

[[widgets]]
type = "text"
name = "chat"
x = 71
y = 0
width = 29
height = 92
streams = ["thoughts", "speech", "whisper"]
border_color = "gray"

[[widgets]]
type = "command_input"
name = "input"
x = 0
y = 93
width = 100
height = 7
prompt = "> "
border = false
```

## Testing Your Setup

### Performance Test

1. Launch VellumFE
2. Check startup time (should be fast)
3. Scroll rapidly (should be smooth)
4. Send many commands (no lag)

### Functionality Test

- [ ] Text displays clearly
- [ ] Scrolling works (Page Up/Down)
- [ ] Commands execute
- [ ] Vital bars update
- [ ] RT countdown works

### Keyboard-Only Test

Try a session without using the mouse:
1. Navigate rooms with numpad
2. Use F-keys for common commands
3. Type commands directly
4. Scroll with Page keys

## Performance Tips

### Reduce Scrollback

Less history = less memory:

```toml
scrollback = 1000  # Instead of 5000+
```

### Disable Animations

If supported:

```toml
[performance]
animations = false
smooth_scroll = false
```

### Simple Highlights

Complex regex patterns slow parsing:

```toml
# Fast (literal string)
pattern = "You are stunned"

# Slower (complex regex)
pattern = "(?i)you\\s+are\\s+(?:completely\\s+)?stunned"
```

### Fewer Streams

Combining streams reduces processing:

```toml
# All in one (fastest)
streams = ["main"]

# Separated (more processing)
streams = ["main", "room", "combat", "thoughts"]
```

## Customization

### Adding Status on Demand

Create a keybind to show/hide status:

```toml
[keybinds."ctrl+h"]
action = "toggle_widget"
widget = "health"
```

### Temporary Expansion

For complex situations, temporarily switch layouts:

```
.layout hunting    # Switch to full layout
.layout minimal    # Back to minimal
```

### Color Adjustments

For different lighting conditions:

**Day Mode** (brighter):
```toml
background = "#1a1a1a"
text = "#e0e0e0"
```

**Night Mode** (darker):
```toml
background = "#000000"
text = "#909090"
```

## Troubleshooting

### Missing Information

If you need data not shown:
1. Add minimal widget for that data
2. Use keybind macros (F3 = experience)
3. Create triggers for notifications

### Accidental Scrollback

Enable auto-scroll:
```toml
auto_scroll = true
```

Or use End key to jump to bottom.

### Can't See Status

Check border settings - borders might be same color as background:
```toml
border = false  # Hide borders entirely
```

### Input Too Small

Increase input height:
```toml
height = 9  # More space for long commands
```

## Philosophy

The minimal layout works best when you:

- Know the game well
- Prefer typing over clicking
- Want maximum text visibility
- Value performance over features
- Play on smaller screens or terminals

It doesn't work well when you:

- Need constant visual status monitoring
- Rely heavily on context menus
- Are new to the game
- Want rich visual feedback

Choose the approach that matches your playstyle.

## See Also

- [Your First Layout](./your-first-layout.md) - For more features
- [Performance Optimization](../architecture/performance.md)
- [Creating Themes](../customization/creating-themes.md)

