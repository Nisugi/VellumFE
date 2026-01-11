# Creating Layouts

Design custom window arrangements for different playstyles.

## Using the Window Editor

1. Press F1 → Windows → Edit Window → [window name]
2. Modify position, size, and properties
3. Save changes (applied immediately)

Or via command: `.edit windowname`

## Adding Windows

### Via Menu

F1 → Windows → Add Window → [Category] → [Widget]

### Via Command

```
.addwindow targets
.addwindow text mywindow
```

### Via Config

Edit `~/.vellum-fe/layout.toml`:

```toml
[[windows]]
name = "mywindow"
widget_type = "text"
streams = ["main"]
row = 0
col = 0
rows = 20
cols = 60
```

## Positioning

### Grid Coordinates

```toml
row = 0       # Top edge (0 = top)
col = 0       # Left edge (0 = left)
rows = 20     # Height
cols = 60     # Width
```

### Overlapping

Windows can overlap. Later windows in the file render on top.

## Saving

- **Ctrl+S** - Save current layout
- **F1 → Config → Save Layout** - Same thing
- Changes to window positions via drag are saved automatically

## Example Layouts

### Hunting Layout

```toml
terminal_width = 160
terminal_height = 50

# Main game text
[[windows]]
name = "main"
widget_type = "text"
streams = ["main"]
row = 0
col = 0
rows = 40
cols = 100

# Targets on right
[[windows]]
name = "targets"
widget_type = "targets"
row = 0
col = 100
rows = 15
cols = 30

# Items below targets
[[windows]]
name = "items"
widget_type = "items"
row = 15
col = 100
rows = 10
cols = 30

# Vitals
[[windows]]
name = "health"
widget_type = "progress"
stat = "health"
row = 25
col = 100
rows = 1
cols = 30

# Roundtime
[[windows]]
name = "rt"
widget_type = "countdown"
id = "roundtime"
row = 26
col = 100
rows = 1
cols = 30

# Command input
[[windows]]
name = "command_input"
widget_type = "command_input"
row = 47
col = 0
rows = 3
cols = 160
```

### Minimal Layout

```toml
terminal_width = 80
terminal_height = 24

[[windows]]
name = "main"
widget_type = "text"
streams = ["main"]
row = 0
col = 0
rows = 21
cols = 80

[[windows]]
name = "command_input"
widget_type = "command_input"
row = 21
col = 0
rows = 3
cols = 80
```

## Per-Character Layouts

Save layouts per character:

```
~/.vellum-fe/characters/CharName/layout.toml
```

Use `--character NAME` to load character-specific layout.
