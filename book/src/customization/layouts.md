# Creating Layouts

Design custom window arrangements for different playstyles.

## Using the Window Editor

1. `.menu` → Windows → Edit Window → [window name]
2. Modify position, size, and properties
3. Save changes (applied immediately)

Or via command: `.editwindow <name>` (no name opens a picker)

On a text window's **Streams** field, `Ctrl+P` opens a picker of stream
ids seen this session — the easy way to wire a window to a Lich script's
custom stream (see
[Custom Streams](../widgets/text-windows.md#custom-streams)).

## Adding Windows

### Via Menu

`.menu` → Windows → Add Window → [Category] → [Widget]

### Via Command

```
.addwindow                                  # opens a picker
.addwindow loot text 100 0 30 10            # name type x y width [height]
```

### Via Config

Edit your layout file directly:

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

## Saving and Switching

```
.savelayout hunting     # save to ~/.vellum-fe/layouts/hunting.toml
.loadlayout hunting
.layouts                # list saved layouts
.resize                 # refit layout to the current terminal size
```

Window positions also auto-save per character: any layout change (moving
or resizing a window, editing its streams, adding or hiding windows) is
written to the per-character auto-save a few seconds later, and
`.savelayout` / `.loadlayout` update it immediately — so your layout
comes back on next launch even if the session ends without a clean
`.quit` (closed terminal, crash). On startup the client loads the
per-character auto-save first, falling back to your saved `default`
layout (`.savelayout` with no name) if no auto-save exists. Use
`.resize` to refit the current layout to the terminal, or
`.loadlayout <name>` to switch layouts.

The same three commands work in the [Desktop GUI](../frontends/gui.md)
on its own layout format: named checkpoints of the GUI arrangement,
saved to the same shared `~/.vellum-fe/layouts/` folder (as
`<name>.json` next to the TUI's `<name>.toml`), so any character can
load a layout any character saved — exactly like the TUI. The two
formats don't cross-load.

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
id = "health"
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

When you launch with `--character NAME`, the current layout auto-saves to:

```
~/.vellum-fe/profiles/CharName/layout.toml
```

Validate a layout file from the command line:

```bash
vellum-fe validate-layout hunting.toml
```
