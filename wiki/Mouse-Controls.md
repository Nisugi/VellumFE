# Mouse Controls Guide

VellumFE has comprehensive mouse support for window management, text selection, and clickable links. This guide covers all mouse operations.

## Prerequisites

Mouse support requires:
- A terminal emulator that supports mouse events
- Mouse mode enabled (on by default in VellumFE)

**Supported terminals:**
- Windows Terminal
- iTerm2 (Mac)
- Alacritty
- Kitty
- GNOME Terminal
- Konsole
- Most modern terminal emulators

## Basic Mouse Operations

### Window Movement

**Click and drag the title bar** to move a window.

**How it works:**
1. Click on the window's title bar (top border)
2. Hold mouse button and drag
3. Release to place window

**Important:**
- Title bar excludes corners (1 cell margin on each side)
- Click the middle section of the title bar
- Cursor changes during drag operation

**Example:**
```
Before:                    After:
┌─ Main ───┐
│           │              ┌─ Main ───┐
│  Text     │   Drag →     │           │
│           │              │  Text     │
└───────────┘              └───────────┘
```

### Window Resizing

**Click and drag borders or corners** to resize a window.

**Resize operations:**
- **Left edge** - Resize width from left
- **Right edge** - Resize width from right
- **Top edge** - Resize height from top
- **Bottom edge** - Resize height from bottom
- **Corners** - Resize both width and height

**How it works:**
1. Click on a window's border or corner
2. Hold mouse button and drag
3. Release when desired size is reached

**Resize from corners:**
```
┌────────┐
│        │    Click and drag any corner
│  Text  │    to resize width and height
│        │    simultaneously
└────────┘
```

**Minimum sizes:**
- Windows have minimum width/height constraints
- Cannot resize below minimum (typically 5x5)

### Scrolling

**Scroll mouse wheel** over any window to scroll up/down.

**How it works:**
- **Scroll up** - Move backwards through history
- **Scroll down** - Move forwards (towards present)
- Scrolling stops at top/bottom of buffer

**Scroll speeds:**
- Typically scrolls 3 lines per wheel notch
- Configurable in some terminal emulators

**Alternative:**
- Use keyboard: `PgUp`/`PgDn` for focused window
- Tab key cycles window focus

### Text Selection

**Click and drag** in text windows to select text.

**How it works:**
1. Click in a text window to start selection
2. Hold mouse button and drag to extend selection
3. Release mouse button - text auto-copies to clipboard
4. Click anywhere or press `Esc` to clear selection

**Features:**
- Selected text highlighted with visible color
- Automatically copied to system clipboard on release
- Respects window boundaries (won't select across windows)
- Only works in text windows

**Important:**
- To use **native terminal selection**, hold `Shift` while clicking/dragging
- Native selection bypasses VellumFE and uses terminal's built-in selection

**Example workflow:**
```
1. Click start: "You swing at the orc"
                 ↑

2. Drag to end:  "You swing at the orc"
                 ^^^^^^^^^^^^^^^^

3. Release:      [Text copied to clipboard]

4. Paste elsewhere: Ctrl+V
```

### Tabbed Windows

**Click tab names** to switch between tabs.

**How it works:**
1. Click on a tab name in the tab bar
2. That tab becomes active
3. Unread indicator clears for that tab

**Tab bar positions:**
- Top (default) - Tabs above window content
- Bottom - Tabs below window content

**Unread indicators:**
- Tabs with new messages show unread indicator (e.g., `* Speech`)
- Clicking tab clears unread status
- Customizable: `tab_unread_prefix`, `tab_unread_color` in config

**Example:**
```
┌─[ * Speech | Thoughts | Whisper ]───┐
│                                      │
│  [New messages in Speech tab]        │
│                                      │
└──────────────────────────────────────┘

Click "Speech" → Clears unread indicator
```

### Clickable Links

**Left-click any clickable word** to open a context menu (if `--links` enabled).

**How it works:**
1. Game objects appear as highlighted/colored text
2. Click any word in the clickable link
3. Context menu appears with available actions
4. Click action or press Enter to execute

**Link detection:**
- Game wraps objects in `<a exist="..." noun="...">` tags
- Multi-word links prioritized (e.g., "raven feather" over "raven")
- Recent links cached (last 100) for quick lookup

**Context menu:**
- Positioned at click location
- Shows available commands for that object
- Hierarchical menu (categories → subcategories)
- Categories in lowercase

**Menu navigation:**
- `↑/↓` - Navigate options
- `Enter` - Select action
- `Esc` or `←` - Close menu/submenu
- `→` or `Enter` on submenu - Open nested menu
- Click action directly with mouse

**Example:**
```
You see a wooden box.
         ^^^^^^^^^^^ (clickable)

Click "wooden" or "box" →

┌─ Actions ─────────┐
│ look               │
│ get                │
│ roleplay ▸         │  ← Submenu indicator
│ interact ▸         │
└────────────────────┘
```

**Submenus:**
```
Click "roleplay" →

┌─ Roleplay ────────┐
│ bow                │
│ curtsy             │
│ wave               │
│ swear ▸            │  ← Nested submenu
└────────────────────┘
```

## Advanced Mouse Operations

### Multi-Window Layouts

Mouse operations work independently on each window:
- Click any window to interact
- No need to focus first
- Drag operations locked to one window at a time

### Overlapping Windows

When windows overlap:
- Top-most window (last in config) receives mouse events
- Clicking visible area of lower window has no effect
- Arrange windows to avoid overlaps

### Mouse State Tracking

VellumFE tracks:
- Current drag operation (move/resize)
- Selection start/end positions
- Last click position
- Active popup menus

**Drag cancellation:**
- Dragging outside terminal bounds may cancel operation
- Release mouse button to finalize
- Esc key does not cancel drags (only clears selection)

### Terminal Size Changes

When terminal is resized:
- Mouse coordinates recalculated automatically
- Window positions remain absolute
- Drag operations may behave unexpectedly during resize

**Best practice:** Complete mouse operations before resizing terminal.

## Mouse Configuration

### Enabling/Disabling Mouse

Mouse is enabled by default. To disable mouse support at runtime:
- Not currently exposed as config option
- Requires code change

### Terminal Configuration

Some terminals require mouse mode to be enabled:

**Windows Terminal:**
```json
{
  "profiles": {
    "defaults": {
      "altGrAliasing": false
    }
  }
}
```

**iTerm2:**
- Preferences → Profiles → Terminal
- Check "Report mouse events"

**Alacritty:**
Mouse events enabled by default.

## Troubleshooting

### Mouse Not Working

**Check terminal support:**
1. Verify terminal emulator supports mouse events
2. Check terminal settings/preferences
3. Try different terminal emulator

**Check VellumFE:**
1. Mouse support is always enabled
2. Verify terminal size is reasonable (not too small)
3. Check debug logs for mouse event errors

### Can't Move Window

**Title bar not responding:**
- Click the middle of the title bar (not corners)
- Corners are reserved for resizing
- Try clicking closer to the window title text

**Window not moving:**
- Ensure you're holding mouse button while dragging
- Release at desired position
- Check window isn't locked (not currently a feature)

### Can't Resize Window

**Border not responding:**
- Click directly on border line
- For corners, click the corner cell exactly
- Try clicking edge borders (easier targets)

**Window not resizing:**
- Check minimum size constraints (5x5 typically)
- Cannot resize below minimum
- Try resizing from different edge

### Text Selection Not Working

**Selection not highlighting:**
- Only works in text windows
- Progress bars, countdown timers not selectable
- Verify clicking inside text window bounds

**Text not copying:**
- Release mouse button to trigger copy
- Check clipboard after release
- Try pasting (Ctrl+V) to verify

**Wrong text selected:**
- Ensure dragging within same window
- Selection doesn't cross window boundaries
- Clear selection with Esc and retry

### Clickable Links Not Working

**Links not clickable:**
- Verify launched with `--links` flag
- Not all text is clickable, only game objects
- Game must wrap objects in `<a>` tags

**Context menu not appearing:**
- Click directly on linked text
- Menu request sent to server (may have latency)
- Check command list is loaded (`defaults/cmdlist1.xml`)

**Menu not responding:**
- Use arrow keys if click not working
- Press Enter to select
- Press Esc to close

### Native Terminal Selection

If VellumFE's selection interferes with terminal selection:

**Use Shift modifier:**
- Hold `Shift` while clicking/dragging
- Bypasses VellumFE selection
- Uses terminal's native selection

**When to use:**
- Selecting across multiple windows
- Selecting UI elements (borders, titles)
- Copying window coordinates from logs

## Mouse Tips and Tricks

### Quick Window Arrangement

1. **Stack vertically:**
   - Move windows to same column, different rows

2. **Stack horizontally:**
   - Move windows to same row, different columns

3. **Grid layout:**
   - Resize windows to equal sizes
   - Arrange in rows and columns

### Efficient Resizing

1. **Corner resize** - Fastest for both dimensions
2. **Edge resize** - Precise control of one dimension
3. **Multiple passes** - Rough resize, then fine-tune

### Text Selection Workflow

1. **Select and copy:**
   - Drag to select
   - Paste immediately (text auto-copied)

2. **Review before copy:**
   - Drag to select
   - Verify selection
   - Release to copy
   - Esc if wrong

### Clickable Link Speed

1. **Common actions:**
   - Create highlights for frequent commands
   - Use macros/keybinds for very frequent actions

2. **Exploration:**
   - Click object to see all available actions
   - Learn new commands from menu

3. **Efficient clicking:**
   - Click shortest word in multi-word links
   - Menus cached for recent links (faster)

## See Also

- [Windows and Layouts](Windows-and-Layouts.md) - Window positioning
- [Getting Started](Getting-Started.md#basic-controls) - Basic mouse operations
- [Commands Reference](Commands.md) - Window management commands
- [Configuration](Configuration.md) - UI settings
