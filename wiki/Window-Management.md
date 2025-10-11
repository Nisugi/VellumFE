# Window Management

Learn how to create, move, resize, and customize windows in vellum-fe.

## Table of Contents

- [Creating Windows](#creating-windows)
- [Moving Windows](#moving-windows)
- [Resizing Windows](#resizing-windows)
- [Deleting Windows](#deleting-windows)
- [Listing Windows](#listing-windows)
- [Customizing Windows](#customizing-windows)
- [Window Positioning](#window-positioning)

## Creating Windows

### From Templates

Use `.createwindow` (or `.createwin`) with a template name:

```
.createwindow loot
.createwin familiar
.createwindow health
```

See `.templates` for a complete list of available templates, or check [Widget Reference](Widget-Reference.md).

**Common templates:**
- Text windows: `main`, `thoughts`, `speech`, `familiar`, `room`, `loot`
- Progress bars: `health`, `mana`, `stamina`, `spirit`, `encumbrance`
- Timers: `roundtime`, `casttime`, `stuntime`
- Special: `compass`, `injuries`, `hands`, `active_spells`

### Custom Windows

Create a window with custom stream routing using `.customwindow`:

```
.customwindow combat combat,death
.customwin alerts warning,danger
```

**Syntax:**
```
.customwindow <name> <stream1,stream2,...>
```

**Notes:**
- Stream list is comma-separated with **no spaces**
- Window appears at default position (0,0) with size 10x40
- Use mouse to move and resize after creation
- See [Stream Routing](Stream-Routing.md) for stream names

### Window Templates

Windows created from templates have pre-configured:
- Stream routing
- Default size and position
- Border style
- Title
- Buffer size

You can change these after creation (see [Customizing Windows](#customizing-windows)).

## Moving Windows

### With Mouse

1. **Click and hold** on the window's **title bar** (top border)
2. **Drag** to desired position
3. **Release** to place

**Notes:**
- Title bar excludes the corners (1 cell margin on each side)
- Windows use absolute positioning - they can overlap or have gaps
- Windows can be moved partially off-screen (use with caution)

### Limitations

- No keyboard shortcut for moving (mouse only)
- No snap-to-grid (coming in future update)
- No window locking (windows can always be moved)

## Resizing Windows

### With Mouse

1. **Click and hold** on an edge or corner
   - **Corners**: Resize from that corner (both dimensions)
   - **Top/Bottom edges**: Resize vertically
   - **Left/Right edges**: Resize horizontally
2. **Drag** to resize
3. **Release** when done

**Resize handles:**
- Top-left corner: Resize from top-left
- Top edge: Resize height (top)
- Top-right corner: Resize from top-right
- Right edge: Resize width (right)
- Bottom-right corner: Resize from bottom-right
- Bottom edge: Resize height (bottom)
- Bottom-left corner: Resize from bottom-left
- Left edge: Resize width (left)

### Minimum Sizes

Each widget type has a minimum size:
- Text windows: 3 rows × 10 cols (room for border + 1 line of text)
- Progress bars: 3 rows × 10 cols
- Countdown timers: 3 rows × 10 cols
- Compass: 5 rows × 15 cols
- Injury doll: 15 rows × 30 cols

Attempting to resize smaller will snap to minimum size.

## Deleting Windows

Use `.deletewindow` (or `.deletewin`):

```
.deletewindow loot
.deletewin familiar
```

**Notes:**
- Deleting a window removes it permanently from the current layout
- Save your layout after deleting if you want to keep the change
- You cannot delete the window you're currently typing in

## Listing Windows

### List All Windows

```
.windows
.listwindows
```

Shows:
- Window name
- Widget type
- Stream subscriptions
- Position and size

**Example output:**
```
Active windows:
- main (text) - streams: main - pos: (0,0) size: (30,120)
- health (progress) - streams: [] - pos: (0,121) size: (3,30)
- thoughts (text) - streams: thoughts - pos: (3,0) size: (10,40)
```

### List Templates

```
.templates
```

Shows all available window templates you can create with `.createwindow`.

## Customizing Windows

### Rename Window

Change the window's display title:

```
.rename <window> <new title>
```

**Examples:**
```
.rename main Game Output
.rename thoughts My Thoughts
.rename loot $$$
```

**Notes:**
- This changes the title shown in the border, not the window's internal name
- Use the original name in commands (e.g., `.deletewindow main`)

### Change Border Style

```
.border <window> <style> [color]
```

**Available styles:**
- `single` - Single line border (─│┌┐└┘)
- `double` - Double line border (═║╔╗╚╝)
- `rounded` - Rounded corners (─│╭╮╰╯)
- `thick` - Thick border (━┃┏┓┗┛)
- `none` - No border

**Examples:**
```
.border main rounded
.border speech double #00ff00
.border thoughts single
.border loot none
```

**Color format:**
- Hex color: `#RRGGBB` (e.g., `#ff0000` for red)
- Omit color to use default

### Progress Bar Colors

For progress bar widgets only:

```
.setbarcolor <window> <bar_color> [bg_color]
```

**Examples:**
```
.setbarcolor health #ff0000 #000000
.setbarcolor mana #0000ff #1a1a1a
.setbarcolor stamina #00ff00
```

**Notes:**
- First color is the bar fill color
- Second color (optional) is the background color
- Use hex format: `#RRGGBB`

### Manual Updates

**Progress bars:**
```
.setprogress <window> <current> <max>
```

Example: `.setprogress health 150 200`

**Countdown timers:**
```
.setcountdown <window> <seconds>
```

Example: `.setcountdown roundtime 5`

**Note:** These are usually updated automatically from game data.

## Window Positioning

### Absolute Positioning

vellum-fe uses **absolute positioning** for windows:
- Each window has a fixed position: `(row, col)`
- Each window has a fixed size: `(rows, cols)`
- Windows are independent - no grid or layout constraints
- Windows can overlap, have gaps, or be moved anywhere

**Coordinates:**
- `row` - Distance from top of terminal (0 = top)
- `col` - Distance from left of terminal (0 = left)
- `rows` - Height of window
- `cols` - Width of window

**Example:**
- Position `(0, 0)` - Top-left corner
- Position `(0, 120)` - Top-right area
- Position `(30, 0)` - Bottom-left area

### Default Positions

New windows created from templates have default positions:
- `main` - (0, 0) - 30 rows × 120 cols
- Other text windows - (0, 0) - 10 rows × 40 cols
- Progress bars - (0, 0) - 3 rows × 30 cols
- Timers - (0, 0) - 3 rows × 15 cols

**Note:** Since all windows default to (0,0), you'll need to move them immediately after creation.

### Z-Order (Rendering Order)

Windows are rendered in the order they appear in the config file:
- First window in config renders first (bottom layer)
- Last window in config renders last (top layer)

**To change z-order:**
1. Save your layout: `.savelayout temp`
2. Edit `~/.vellum-fe/layouts/temp.toml`
3. Reorder the `[[windows]]` sections
4. Load layout: `.loadlayout temp`

### Off-Screen Windows

Windows can be positioned off-screen:
- If `col + cols > terminal width`, window is clipped on the right
- If `row + rows > terminal height`, window is clipped on the bottom
- You can intentionally position windows outside the visible area

**To fix off-screen windows:**
1. Resize your terminal larger
2. Edit the layout file manually
3. Delete and recreate the window

## Advanced Tips

### Overlapping Windows

Windows can overlap. This is useful for:
- Popup-style windows (e.g., loot on top of main)
- Temporary windows (e.g., combat log overlay)
- Dashboard layouts (multiple small windows in same area)

**To bring a window to front:**
- Click on it to focus
- Overlapping windows render in config order (see [Z-Order](#z-order-rendering-order))

### Window Layouts

See [Layout Management](Layout-Management.md) for:
- Saving layouts
- Loading layouts
- Multiple layouts per character
- Autosave on exit

### Configuration File

For advanced window customization, edit `~/.vellum-fe/config.toml` directly:

```toml
[[ui.windows]]
name = "custom"
widget_type = "text"
streams = ["combat", "death"]
row = 0
col = 0
rows = 20
cols = 80
buffer_size = 5000
show_border = true
border_style = "rounded"
border_color = "#ff0000"
title = "Combat Log"
```

See [Configuration Guide](Configuration-Guide.md) for details.

## Next Steps

- **[Widget Reference](Widget-Reference.md)** - Learn about all 40+ widget types
- **[Layout Management](Layout-Management.md)** - Save and load window arrangements
- **[Commands Reference](Commands-Reference.md)** - Complete command list

---

← [Quick Start](Quick-Start.md) | [Widget Reference](Widget-Reference.md) →
