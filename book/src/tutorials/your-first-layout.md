# Your First Layout

Create a functional VellumFE layout from scratch in 30 minutes.

## Goal

By the end of this tutorial, you'll have:

- A main text window for game output
- Health, mana, and stamina bars
- A compass for navigation
- A command input area
- Basic keybinds for common actions

## Prerequisites

- VellumFE installed
- Connection to game (via Lich or direct)
- Text editor for config files

## Step 1: Understanding the Grid

VellumFE uses a **percentage-based grid** where:

- `x` and `y` are positions (0-100)
- `width` and `height` are sizes (0-100)
- Origin (0, 0) is top-left corner

```
(0,0)─────────────────────────(100,0)
  │                                │
  │     Your Terminal Window       │
  │                                │
(0,100)───────────────────────(100,100)
```

## Step 2: Plan Your Layout

Before coding, sketch your layout:

```
┌─────────────────────────────────────┐
│  Room Info (top bar)                │
├──────────────────────────┬──────────┤
│                          │ Compass  │
│                          ├──────────┤
│     Main Text            │ Health   │
│                          │ Mana     │
│                          │ Stamina  │
├──────────────────────────┴──────────┤
│  Command Input (bottom)             │
└─────────────────────────────────────┘
```

## Step 3: Create the Main Text Window

Open `~/.vellum-fe/layout.toml` and start fresh:

```toml
# Main game text window
[[widgets]]
type = "text"
name = "main"
title = "Game"
x = 0
y = 5
width = 75
height = 85
streams = ["main", "combat", "room"]
scrollback = 5000
```

**What this does**:
- Creates a text window named "main"
- Positions it from 0% left, 5% from top
- Takes 75% width, 85% height
- Shows main game output, combat, and room descriptions
- Keeps 5000 lines of history

## Step 4: Add the Room Info Bar

Add a room info display at the top:

```toml
# Room information bar
[[widgets]]
type = "room"
name = "room_info"
x = 0
y = 0
width = 100
height = 5
show_exits = true
show_creatures = true
```

## Step 5: Add the Compass

Position the compass in the right sidebar:

```totml
# Navigation compass
[[widgets]]
type = "compass"
name = "compass"
x = 76
y = 5
width = 24
height = 15
style = "unicode"
clickable = true
```

**Style options**:
- `"ascii"` - Basic ASCII characters
- `"unicode"` - Unicode arrows (recommended)
- `"minimal"` - Just letters (N, S, E, W)

## Step 6: Add Vital Bars

Add health, mana, and stamina bars below the compass:

```toml
# Health bar
[[widgets]]
type = "progress"
name = "health"
title = "Health"
x = 76
y = 21
width = 24
height = 3
data_source = "vitals.health"
color = "health"
show_text = true

# Mana bar
[[widgets]]
type = "progress"
name = "mana"
title = "Mana"
x = 76
y = 25
width = 24
height = 3
data_source = "vitals.mana"
color = "mana"
show_text = true

# Stamina bar
[[widgets]]
type = "progress"
name = "stamina"
title = "Stamina"
x = 76
y = 29
width = 24
height = 3
data_source = "vitals.stamina"
color = "stamina"
show_text = true
```

## Step 7: Add Command Input

Add the command input at the bottom:

```toml
# Command input
[[widgets]]
type = "command_input"
name = "input"
x = 0
y = 91
width = 100
height = 9
history_size = 500
prompt = "> "
```

## Step 8: Complete Layout File

Your complete `layout.toml` should look like:

```toml
# VellumFE Layout - Basic Setup
# Created following the "Your First Layout" tutorial

# Room information bar
[[widgets]]
type = "room"
name = "room_info"
x = 0
y = 0
width = 100
height = 5
show_exits = true
show_creatures = true

# Main game text window
[[widgets]]
type = "text"
name = "main"
title = "Game"
x = 0
y = 5
width = 75
height = 85
streams = ["main", "combat", "room"]
scrollback = 5000

# Navigation compass
[[widgets]]
type = "compass"
name = "compass"
x = 76
y = 5
width = 24
height = 15
style = "unicode"
clickable = true

# Health bar
[[widgets]]
type = "progress"
name = "health"
title = "Health"
x = 76
y = 21
width = 24
height = 3
data_source = "vitals.health"
color = "health"
show_text = true

# Mana bar
[[widgets]]
type = "progress"
name = "mana"
title = "Mana"
x = 76
y = 25
width = 24
height = 3
data_source = "vitals.mana"
color = "mana"
show_text = true

# Stamina bar
[[widgets]]
type = "progress"
name = "stamina"
title = "Stamina"
x = 76
y = 29
width = 24
height = 3
data_source = "vitals.stamina"
color = "stamina"
show_text = true

# Command input
[[widgets]]
type = "command_input"
name = "input"
x = 0
y = 91
width = 100
height = 9
history_size = 500
prompt = "> "
```

## Step 9: Add Basic Keybinds

Create `~/.vellum-fe/keybinds.toml`:

```toml
# Basic navigation
[keybinds."numpad8"]
macro = "north"

[keybinds."numpad2"]
macro = "south"

[keybinds."numpad4"]
macro = "west"

[keybinds."numpad6"]
macro = "east"

[keybinds."numpad7"]
macro = "northwest"

[keybinds."numpad9"]
macro = "northeast"

[keybinds."numpad1"]
macro = "southwest"

[keybinds."numpad3"]
macro = "southeast"

[keybinds."numpad5"]
macro = "out"

# Quick actions
[keybinds."f1"]
macro = "look"

[keybinds."f2"]
macro = "inventory"

[keybinds."f3"]
macro = "experience"

# Widget navigation
[keybinds."tab"]
action = "next_widget"

[keybinds."shift+tab"]
action = "prev_widget"

[keybinds."page_up"]
action = "scroll_up"

[keybinds."page_down"]
action = "scroll_down"

# Escape to focus input
[keybinds."escape"]
action = "focus_input"
```

## Step 10: Test Your Layout

1. **Save both files**

2. **Reload configuration**:
   ```
   .reload
   ```

3. **Verify widgets appear**:
   - Main text window shows game output
   - Compass displays available exits
   - Vital bars show current values
   - Command input accepts typing

4. **Test keybinds**:
   - Press numpad keys for movement
   - Press F1-F3 for quick commands
   - Use Tab to cycle widgets

## Testing Checklist

- [ ] Main window shows text
- [ ] Room info updates when moving
- [ ] Compass shows exits
- [ ] Health bar reflects actual health
- [ ] Mana bar works
- [ ] Stamina bar works
- [ ] Can type commands
- [ ] Command history works (up/down arrows)
- [ ] Keybinds execute commands

## Customization Ideas

### Adjust Sizes

If the sidebar feels too wide:

```toml
# Narrower sidebar (20% instead of 24%)
width = 80    # Main text
x = 81        # Sidebar widgets
width = 19    # Sidebar width
```

### Add More Bars

Spirit bar for some professions:

```toml
[[widgets]]
type = "progress"
name = "spirit"
title = "Spirit"
x = 76
y = 33
width = 24
height = 3
data_source = "vitals.spirit"
color = "spirit"
show_text = true
```

### Add Roundtime Display

```toml
[[widgets]]
type = "countdown"
name = "roundtime"
title = "RT"
x = 76
y = 37
width = 24
height = 3
data_source = "roundtime"
```

### Add Status Indicators

```toml
[[widgets]]
type = "indicator"
name = "status"
x = 76
y = 41
width = 24
height = 8
indicators = ["hidden", "stunned", "webbed", "prone", "kneeling"]
```

## Troubleshooting

### Widgets Overlap

Check your coordinate math:
- Widget end = x + width (or y + height)
- Ensure widgets don't exceed 100%
- Leave small gaps between widgets

### Bars Not Updating

Verify data source names:
- `vitals.health`
- `vitals.mana`
- `vitals.stamina`
- `vitals.spirit`

### Compass Not Showing Exits

The game must send exit data. Check that:
- You're logged in
- You've moved to a room
- Brief mode isn't hiding room info

### Keybinds Not Working

1. Check key syntax (`"numpad8"` not `"num8"`)
2. Verify no conflicts with system keys
3. Reload config: `.reload keybinds`

## Next Steps

Congratulations! You've created your first layout.

Consider exploring:
- [Hunting Setup](./hunting-setup.md) - Combat optimization
- [Creating Layouts](../customization/creating-layouts.md) - Advanced techniques
- [Widget Reference](../widgets/README.md) - All widget types

## See Also

- [Layout Configuration](../configuration/layout-toml.md)
- [Keybinds Configuration](../configuration/keybinds-toml.md)
- [Compass Widget](../widgets/compass.md)
- [Progress Bars](../widgets/progress-bars.md)

