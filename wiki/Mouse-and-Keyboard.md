# Mouse and Keyboard Guide

This guide covers all keyboard shortcuts and mouse operations available in profanity-rs.

## Table of Contents

- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Mouse Operations](#mouse-operations)
- [Text Selection](#text-selection)
- [Window Focus](#window-focus)
- [Scrolling](#scrolling)
- [Command History](#command-history)
- [Mouse Mode Toggle](#mouse-mode-toggle)

---

## Keyboard Shortcuts

### Command Input

| Key | Action |
|-----|--------|
| `Enter` | Send command to game server |
| `Backspace` | Delete character before cursor |
| `Delete` | Delete character at cursor |
| `Left Arrow` | Move cursor left |
| `Right Arrow` | Move cursor right |
| `Home` | Move cursor to beginning of line |
| `End` | Move cursor to end of line |
| `Up Arrow` | Previous command in history |
| `Down Arrow` | Next command in history |
| `Ctrl+C` | Clear current input |
| `Ctrl+U` | Clear entire line |

### Window Navigation

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus to next window |
| `Shift+Tab` | Cycle focus to previous window |
| `Page Up` | Scroll up in focused window |
| `Page Down` | Scroll down in focused window |
| `Home` | (In window) Scroll to top of buffer |
| `End` | (In window) Scroll to bottom of buffer |

### Application

| Key | Action |
|-----|--------|
| `F11` | Toggle mouse mode on/off (default) |
| `Ctrl+Q` | (Planned) Quick quit |
| `Esc` | (Planned) Cancel current operation |

### Custom Keybinds (Planned)

Keybind support is planned but not yet implemented. See [Configuration Guide](Configuration-Guide.md#keybinds) for the planned format.

---

## Mouse Operations

**Note:** Mouse operations require mouse mode to be enabled. Press `F11` (default) to toggle mouse mode on/off.

### Window Resizing

Click and drag window edges or corners to resize:

| Mouse Target | Action |
|--------------|--------|
| **Top edge** | Resize from top (moves top, keeps bottom fixed) |
| **Bottom edge** | Resize from bottom (keeps top fixed) |
| **Left edge** | Resize from left (moves left, keeps right fixed) |
| **Right edge** | Resize from right (keeps left fixed) |
| **Top-left corner** | Resize from top-left corner |
| **Top-right corner** | Resize from top-right corner |
| **Bottom-left corner** | Resize from bottom-left corner |
| **Bottom-right corner** | Resize from bottom-right corner |

**How it works:**
1. Click on edge or corner
2. Hold mouse button down
3. Drag to new position
4. Release to finish resize

**Tips:**
- Corner resizing changes both dimensions simultaneously
- Edge resizing changes only one dimension
- Resize is incremental (delta-based) for smooth operation

### Window Moving

Click and drag the window title bar to move:

| Mouse Target | Action |
|--------------|--------|
| **Title bar** | Click and drag to move window |

**How it works:**
1. Click on the title bar (top border, not the corners)
2. Hold mouse button down
3. Drag to new position
4. Release to drop window

**Notes:**
- Title bar excludes the corners (1 cell margin on each side)
- Corners are reserved for resizing
- Moving uses incremental deltas, not absolute positioning

### Scrolling

Scroll windows with the mouse wheel:

| Mouse Action | Result |
|--------------|--------|
| **Scroll Up** | Scroll text window up (view older text) |
| **Scroll Down** | Scroll text window down (view newer text) |

**Notes:**
- Scrolling works on text windows with scrollback history
- Progress bars and countdown timers don't have scrollable content
- Scroll amount: 3 lines per wheel notch (configurable per terminal)

### Window Focus

Click anywhere in a window to focus it:

| Mouse Action | Result |
|--------------|--------|
| **Click in window** | Focus that window |

**Focused window:**
- Receives keyboard scrolling commands (`Page Up`/`Page Down`)
- Visually indicated (implementation may vary)

---

## Text Selection

**Note:** Text selection requires mouse mode to be **disabled**.

### Enabling Text Selection Mode

1. Press `F11` (or configured toggle key) to disable mouse mode
2. Mouse operations (move/resize) will not work
3. Terminal's native text selection is active

### Selecting Text

With mouse mode off:

| Mouse Action | Result |
|--------------|--------|
| **Click and drag** | Select text |
| **Shift+Click** | Extend selection |
| **Shift+Drag** | Select rectangular area (terminal-dependent) |
| **Double-click** | Select word (terminal-dependent) |
| **Triple-click** | Select line (terminal-dependent) |

### Copying Text

**Method 1: Terminal Built-in**
- **Windows Terminal:** Ctrl+Shift+C or right-click
- **iTerm2 (macOS):** Cmd+C
- **GNOME Terminal:** Ctrl+Shift+C
- **Alacritty:** Ctrl+Shift+C

**Method 2: Right-Click Menu**
- Some terminals support right-click → Copy

**Method 3: Auto-copy**
- Some terminals auto-copy selection to clipboard

### Toggling Back to Mouse Mode

Press `F11` again to re-enable mouse mode for window operations.

---

## Window Focus

### Focus Behavior

- **Focused window** receives keyboard scrolling commands
- Only one window can be focused at a time
- Focus cycles through windows with `Tab`/`Shift+Tab`

### Focus Indicators

The focused window may show visual indicators:
- Brighter border (implementation-dependent)
- Different border color
- Title bar highlighting

### Focusing a Window

**Keyboard:**
- Press `Tab` repeatedly to cycle through windows
- Press `Shift+Tab` to cycle backwards

**Mouse:**
- Click anywhere in the window

---

## Scrolling

### Keyboard Scrolling

With a window focused:

| Key | Action | Distance |
|-----|--------|----------|
| `Page Up` | Scroll up | 10 lines |
| `Page Down` | Scroll down | 10 lines |
| `Home` | Jump to top | (All the way) |
| `End` | Jump to bottom | (All the way) |

### Mouse Scrolling

Scroll wheel over any text window:

| Action | Result |
|--------|--------|
| Scroll up | Scroll text up (older) |
| Scroll down | Scroll text down (newer) |

### Scrollback Buffer

Each text window maintains a scrollback buffer:

- Default size: 1000 lines
- Configurable per window: `buffer_size = 10000`
- Older lines are discarded when buffer fills
- Scrollback persists until window is deleted or app restarts

**Example config:**
```toml
[[ui.windows]]
name = "main"
buffer_size = 10000  # Keep 10,000 lines
```

### Auto-scrolling

Text windows auto-scroll to bottom when:
- New text arrives
- You're already at the bottom

Text windows **don't** auto-scroll when:
- You've scrolled up to view history
- Allows reading older text without interruption

To resume auto-scrolling:
- Press `End`
- Scroll all the way down with mouse
- Click at bottom of window

---

## Command History

### Navigating History

| Key | Action |
|-----|--------|
| `Up Arrow` | Previous command |
| `Down Arrow` | Next command |

### How It Works

1. Type commands and press `Enter`
2. Each command is saved to history
3. Press `Up` to recall previous commands
4. Press `Down` to move forward through history
5. Edit recalled command and press `Enter` to send

### History Size

- Default: 100 commands (implementation-dependent)
- History persists during session
- Cleared when application exits
- (Planned: Persistent history across sessions)

### History Tips

**Repeat Last Command:**
```
(Press Up once, then Enter)
```

**Edit Previous Command:**
```
(Press Up, edit text, press Enter)
```

**Search History (Planned):**
```
Ctrl+R (not yet implemented)
```

---

## Mouse Mode Toggle

### What is Mouse Mode?

**Mouse Mode ON:**
- Click and drag to move windows
- Click edges/corners to resize
- Click to focus windows
- Scroll wheel scrolls text windows
- **Cannot select text with mouse**

**Mouse Mode OFF:**
- Terminal's native text selection works
- Can copy text to clipboard
- **Cannot move/resize windows**
- **Cannot click to focus**

### Toggling Mouse Mode

Default key: `F11`

**To change the toggle key:**

Edit `config.toml`:
```toml
[ui]
mouse_mode_toggle_key = "F11"  # Change to F12, F10, etc.
```

**Available keys:**
- Function keys: `F1` through `F12`
- Other special keys (implementation-dependent)

### When to Use Mouse Mode

**Use Mouse Mode ON when:**
- Setting up your layout
- Moving/resizing windows
- Actively playing (scrolling, focusing windows)

**Use Mouse Mode OFF when:**
- Copying text from windows
- Selecting error messages
- Copying commands or loot lists
- Taking screenshots with text selection

### Terminal Requirements

Not all terminals support mouse operations. Works best with:

**Recommended Terminals:**
- ✅ Windows Terminal (Windows)
- ✅ iTerm2 (macOS)
- ✅ Alacritty (All platforms)
- ✅ GNOME Terminal (Linux)
- ✅ Konsole (Linux)
- ⚠️ CMD.exe (Limited support)
- ⚠️ PowerShell ISE (Limited support)

**Testing Mouse Support:**
1. Launch profanity-rs
2. Ensure mouse mode is on (press `F11` if needed)
3. Try clicking on a window border and dragging
4. If it works, your terminal supports mouse operations

---

## Tips and Tricks

### Quick Layout Adjustment

1. Press `F11` to enable mouse mode
2. Drag windows to desired positions
3. Resize as needed
4. Press `.savelayout hunting` to save
5. Press `F11` to disable mouse mode
6. Select and copy any text you need

### Efficient Scrolling

- Use `Page Up`/`Page Down` for quick jumps
- Use mouse wheel for fine control
- Press `End` to jump back to live text

### Multi-Window Workflow

1. Set up windows side-by-side
2. Use `Tab` to quickly switch focus
3. Scroll independently in each window
4. Keep combat in one, loot in another

### Copy Text Without Leaving Mouse Mode

Some terminals support Ctrl+Shift+C even in mouse mode. Try:
1. Leave mouse mode ON
2. Click and drag to try selecting (might not show)
3. Press Ctrl+Shift+C
4. Paste to test if it copied

(Terminal-dependent; YMMV)

---

## Keyboard Shortcuts Quick Reference

```
┌─────────────────────────────────────────────────┐
│ Command Input                                   │
├─────────────────────────────────────────────────┤
│ Enter          Send command                     │
│ Up/Down        Command history                  │
│ Left/Right     Move cursor                      │
│ Home/End       Start/end of line                │
│ Ctrl+C         Clear input                      │
│ Ctrl+U         Clear entire line                │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│ Window Navigation                               │
├─────────────────────────────────────────────────┤
│ Tab            Next window                      │
│ Shift+Tab      Previous window                  │
│ Page Up        Scroll up                        │
│ Page Down      Scroll down                      │
│ Home           Scroll to top                    │
│ End            Scroll to bottom                 │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│ Application                                     │
├─────────────────────────────────────────────────┤
│ F11            Toggle mouse mode                │
│ .quit          Exit application                 │
└─────────────────────────────────────────────────┘
```

---

[← Previous: Stream Routing](Stream-Routing.md) | [Next: Troubleshooting →](Troubleshooting.md)
