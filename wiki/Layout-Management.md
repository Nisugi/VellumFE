# Layout Management

Save and load custom window arrangements to quickly switch between different setups.

## Table of Contents

- [What Are Layouts?](#what-are-layouts)
- [Saving Layouts](#saving-layouts)
- [Loading Layouts](#loading-layouts)
- [Listing Layouts](#listing-layouts)
- [Autosave](#autosave)
- [Layout Files](#layout-files)
- [Use Cases](#use-cases)

## What Are Layouts?

A **layout** is a saved snapshot of your window configuration, including:
- Which windows exist
- Window positions (row, col)
- Window sizes (rows, cols)
- Window titles
- Border styles and colors
- Stream routing
- Widget-specific settings (progress bar colors, etc.)

Layouts let you:
- Save different setups for different activities (hunting, town, combat)
- Share layouts with other users
- Quickly restore your preferred window arrangement
- Have different layouts per character

## Saving Layouts

### Save Current Layout

```
.savelayout <name>
```

**Examples:**
```
.savelayout hunting
.savelayout town
.savelayout default
.savelayout combat
```

**What gets saved:**
- All active windows and their configurations
- Window positions and sizes
- Stream routing
- Border styles and colors
- Progress bar colors
- All widget settings

**What doesn't get saved:**
- Window content (scrollback buffers are empty on load)
- Command history
- Connection settings
- Keybinds and highlights (stored in main config)

### Layout Storage Location

Layouts are saved to: `~/.vellum-fe/layouts/<name>.toml`

**Example paths:**
- Linux/Mac: `~/.vellum-fe/layouts/hunting.toml`
- Windows: `C:\Users\YourName\.vellum-fe\layouts\hunting.toml`

## Loading Layouts

### Load a Saved Layout

```
.loadlayout <name>
```

**Examples:**
```
.loadlayout hunting
.loadlayout town
.loadlayout default
```

**What happens when you load:**
1. All current windows are closed
2. Windows from the layout are created
3. Windows are positioned and sized as saved
4. Stream routing is restored
5. Widget settings are applied

**Note:** Loading a layout **replaces** your current windows. Save first if you want to keep your current setup!

### Default Layout

If you create a layout named `default`, it will be loaded automatically when you specify no name:

```
.savelayout default    # Save as default
.loadlayout           # Loads 'default'
```

## Listing Layouts

### View All Saved Layouts

```
.layouts
```

Shows all layout files in `~/.vellum-fe/layouts/`.

**Example output:**
```
Available layouts:
- autosave
- default
- hunting
- town
- combat
```

## Autosave

vellum-fe automatically saves your layout when you exit gracefully.

### How Autosave Works

1. When you exit with `.quit` or `Ctrl+C`:
   - Current layout is saved to `~/.vellum-fe/layouts/autosave.toml`
   - Autosave is created automatically

2. On next launch:
   - If `autosave.toml` exists, it's loaded automatically
   - Your windows are restored exactly as you left them

### Important Notes

**Autosave ONLY works if you exit properly:**
- ✅ Type `.quit` in command input
- ✅ Press `Ctrl+C`
- ❌ Close terminal window with X button (kills process, no autosave)
- ❌ Kill process with Task Manager/`kill -9`

**To disable autosave:**
- Delete `~/.vellum-fe/layouts/autosave.toml`
- It will be recreated on next exit

**To prevent autosave overwrite:**
1. Save your layout: `.savelayout mysetup`
2. Exit normally (autosave is created)
3. Next launch: `.loadlayout mysetup` (restores your saved layout)

## Layout Files

### File Format

Layout files are TOML format containing window configurations and command input settings:

```toml
# Command input configuration (optional)
[command_input]
row = 0
col = 0
height = 3
width = 0  # 0 = full terminal width
show_border = true
border_style = "single"  # "single", "double", "rounded", "thick"
border_color = "#ffffff"
title = "Command"
background_color = "#1a1a1a"  # Optional background color

# Window configurations
[[windows]]
name = "main"
widget_type = "text"
streams = ["main"]
row = 0
col = 0
rows = 30
cols = 120
buffer_size = 10000
show_border = true
border_style = "single"
title = "Main"

[[windows]]
name = "health"
widget_type = "progress"
streams = []
row = 0
col = 121
rows = 3
cols = 30
show_border = true
title = "Health"
bar_color = "#ff0000"
bar_background_color = "#000000"
```

### Command Input Configuration

The `[command_input]` section controls the command input widget appearance and position:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `row` | integer | 0 | Row position (0 = top) |
| `col` | integer | 0 | Column position (0 = left) |
| `height` | integer | 3 | Height in rows |
| `width` | integer | 0 | Width in columns (0 = full terminal width) |
| `show_border` | boolean | true | Show border around input |
| `border_style` | string | "single" | Border style: "single", "double", "rounded", "thick" |
| `border_color` | string | - | Border color in hex (e.g., "#ffffff") |
| `title` | string | None | Title shown in border (omit for no title) |
| `background_color` | string | - | Background color in hex (transparent if not set) |

**Example:**
```toml
# Borderless command input at bottom with dark background
[command_input]
row = 67
col = 0
height = 3
width = 0
show_border = false
background_color = "#1a1a1a"
```

### Manual Editing

You can edit layout files directly:

1. Open layout file: `~/.vellum-fe/layouts/hunting.toml`
2. Modify window positions, sizes, colors, etc.
3. Save file
4. Load in vellum-fe: `.loadlayout hunting`

**Use cases for manual editing:**
- Fine-tune window positions with pixel precision
- Bulk-edit multiple windows (e.g., change all border colors)
- Fix off-screen windows
- Duplicate windows (copy/paste `[[windows]]` sections)

### Sharing Layouts

Layout files can be shared with other users:

1. Save your layout: `.savelayout mysetup`
2. Copy layout file: `~/.vellum-fe/layouts/mysetup.toml`
3. Share with friend
4. Friend places in their `~/.vellum-fe/layouts/` directory
5. Friend loads: `.loadlayout mysetup`

**Note:** Shared layouts may need adjustment if users have different terminal sizes.

## Use Cases

### Multiple Character Layouts

Save different layouts for different characters:

```
# Playing as a warrior
.savelayout warrior

# Playing as a wizard
.savelayout wizard

# Switch characters
.loadlayout wizard
```

Each character can have different window arrangements, sizes, and priorities.

### Activity-Based Layouts

Different layouts for different activities:

**Hunting layout:**
- Large main window
- Combat log
- Health/mana/stamina bars
- Roundtime timer
- Injury doll

```
.savelayout hunting
```

**Town layout:**
- Main window
- Speech window (larger)
- Thoughts window
- Minimal vitals

```
.savelayout town
```

**Scripting layout:**
- Small main window
- Large loot window
- Experience window
- Minimal distractions

```
.savelayout scripting
```

### Terminal Size Layouts

Save layouts for different terminal sizes:

```
.savelayout fullscreen    # For maximized terminal
.savelayout laptop        # For smaller laptop screen
.savelayout mobile        # For phone terminal (Termux)
```

Load the appropriate layout based on your current screen.

### Experimental Layouts

Test new window arrangements without losing your current setup:

```
.savelayout backup        # Save current layout
# ... experiment with windows ...
.loadlayout backup        # Restore if you don't like it
# OR
.savelayout experiment    # Save experiment for later
```

## Tips and Best Practices

### 1. Save Early, Save Often

Create a backup before making major changes:
```
.savelayout backup
# ... make changes ...
.savelayout newsetup
```

### 2. Name Layouts Descriptively

Use clear, descriptive names:
- ✅ `hunting-warrior`, `town-social`, `scripting-minimal`
- ❌ `layout1`, `test`, `new`

### 3. Version Your Layouts

When iterating on a layout:
```
.savelayout hunting-v1
# ... make improvements ...
.savelayout hunting-v2
# ... more improvements ...
.savelayout hunting-v3
```

Keep older versions until you're sure the new one is better.

### 4. Clean Up Unused Layouts

Periodically delete old layouts:

```bash
# View layouts
.layouts

# Delete unused layout files manually
rm ~/.vellum-fe/layouts/old-layout.toml
```

### 5. Terminal Size Matters

Layouts saved on a large terminal may not fit smaller terminals:
- Windows may appear off-screen
- Overlapping windows may look cramped
- Consider creating size-specific layouts

### 6. Use Autosave as Scratch Space

Since autosave is recreated on every exit:
- Use it for temporary/experimental setups
- Save permanent layouts with specific names
- Autosave is your "most recent" layout

## Troubleshooting

### Layout Doesn't Load

**Problem:** `.loadlayout mysetup` shows error or nothing happens

**Solutions:**
- Check layout exists: `.layouts`
- Check filename: `~/.vellum-fe/layouts/mysetup.toml` (must be exact)
- Check file syntax: Open in text editor, look for TOML errors
- Try autosave: `.loadlayout autosave`

### Windows Off-Screen

**Problem:** Layout loads but windows aren't visible

**Solutions:**
- Resize terminal larger
- Edit layout file and adjust `row`/`col` values
- Delete problem windows from layout file
- Create new layout at current terminal size

### Autosave Not Working

**Problem:** Layout not restored on next launch

**Solutions:**
- Exit with `.quit` or `Ctrl+C` (not window X button)
- Check file exists: `~/.vellum-fe/layouts/autosave.toml`
- Check file permissions (must be writable)
- Check terminal output for save errors

### Lost My Layout

**Problem:** Accidentally loaded wrong layout and lost current setup

**Solutions:**
- Check if autosave has your old layout (before you loaded)
- Check for backup layouts you may have created
- Recreate windows manually and save new layout
- **Prevention:** Always save before loading: `.savelayout backup` then `.loadlayout new`

## Next Steps

- **[Configuration Guide](https://github.com/Nisugi/VellumFE/wiki/Configuration-Guide)** - Advanced config options
- **[Window Management](https://github.com/Nisugi/VellumFE/wiki/Window-Management)** - Creating and customizing windows
- **[Commands Reference](https://github.com/Nisugi/VellumFE/wiki/Commands-Reference)** - Complete command list

---

← [Widget Reference](https://github.com/Nisugi/VellumFE/wiki/Widget-Reference) | [Commands Reference](https://github.com/Nisugi/VellumFE/wiki/Commands-Reference) →
