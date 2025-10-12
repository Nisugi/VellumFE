# Mouse and Keyboard Guide

This guide covers all keyboard shortcuts and mouse operations available in vellum-fe.

## Table of Contents

- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Mouse Operations](#mouse-operations)
- [Clickable Links and Context Menus](#clickable-links-and-context-menus)
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
| `F12` | Toggle performance stats display |
| `Ctrl+C` | Quit application |

### Custom Keybinds

Fully implemented! See [Keybind Management](Keybind-Management.md) for details on creating and managing custom keybinds.

---

## Mouse Operations

Mouse operations work immediately - no toggle required!

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

## Clickable Links and Context Menus

VellumFE features Wrayth-style clickable links with hierarchical context menus for game objects.

### How It Works

Game objects (items, NPCs, players, etc.) appear as **clickable links** in your text windows. Click any word in a link to open a context menu showing available actions.

### Basic Usage

| Mouse Action | Result |
|--------------|--------|
| **Left-click on link word** | Open context menu at cursor |
| **Click and drag link word** | Drag and drop item (see below) |
| **Click menu item** | Execute that action |
| **Click outside menu** | Close menu |

### Drag and Drop

VellumFE supports dragging game objects (items) to perform actions like putting items in containers or dropping them.

**How it works:**

1. **Hold Ctrl** (or configured modifier key) and **click** on any word in an item link
2. **Drag** the mouse to a new location while holding Ctrl
3. **Release** to complete the action:
   - **Drop on another item link** → Sends `put my X in my Y`
   - **Drop in empty space** → Sends `drop my X`

**Without modifier key:** Clicking a link without holding Ctrl opens the context menu immediately.

**Examples:**

- Ctrl+drag "apple" onto "backpack" → `put my apple in my backpack`
- Ctrl+drag "sword" onto "scabbard" → `put my sword in my scabbard`
- Ctrl+drag "torch" to empty space → `drop my torch`
- Click "apple" (no Ctrl) → Opens context menu

**Configuration:**

The modifier key can be customized in your config file (`~/.vellum-fe/configs/default.toml`):

```toml
[ui]
drag_modifier_key = "ctrl"  # Options: "ctrl", "alt", "shift", or "none"
```

**Tips:**

- The drag must move at least 2 pixels to register as a drag (prevents accidental drags)
- You can drag items to any text window (not just the same window)
- Works with the same link detection as context menus (multi-word priority, etc.)
- Set `drag_modifier_key = "none"` to enable drag without holding any key

**Limitations:**

- Currently only supports "put X in Y" and "drop X" commands
- Does not yet support "give X to Y" for NPCs/players
- The game server determines whether the action is valid

### Keyboard Navigation

When a context menu is open:

| Key | Action |
|-----|--------|
| `Up Arrow` | Select previous menu item |
| `Down Arrow` | Select next menu item |
| `Enter` | Execute selected item |
| `Right Arrow` | Open submenu (if available) |
| `Esc` or `Left Arrow` | Close current menu (keeps parent open) |

### Menu Hierarchy

Menus support 3 levels of nesting:

1. **Main Menu** - Primary actions (look, get, drop, etc.)
2. **Category Submenu** - Grouped actions (e.g., "roleplay >")
3. **Nested Submenu** - Subcategories (e.g., "swear >" under roleplay)

**Example flow:**
```
Click "staff" → Main menu appears
Click "roleplay >" → Roleplay submenu opens (main stays visible)
Click "swear >" → Swear submenu opens (all three visible)
Click "swear at flamingly" → Command sent, menus close
```

### Available Commands

VellumFE includes 588 commands from `cmdlist1.xml`:

**Common categories:**
- **General actions:** look, examine, get, drop, put, raise, spin, turn
- **Roleplay:** bow, curtsy, wave, nod, smile, laugh, etc.
  - **Roleplay-Swear:** Various swearing actions
- **Combat maneuvers:** Specialized combat actions
- **Item manipulation:** Specific item actions based on type

### Menu Indicators

- **">"** after menu text = Submenu available (click or press Right to open)
- **No ">"** = Regular command (click or press Enter to execute)

### Smart Link Detection

- **Multi-word priority:** Clicking "raven" in "raven feather" finds the feather, not a separate raven NPC
- **Recent links cache:** Last 100 links cached for fast lookups
- **Context-aware:** Menus show actions relevant to the clicked object

### Secondary Items

Some actions require a held item (indicated by `%` placeholder):

- **"throw %"** - Throw your held item
- **"wave % at"** - Wave your held item at the target

If you're holding an item when you click a target, these actions will show the item name:
- **"throw gleaming steel baselard"**
- **"wave gleaming steel baselard at"**

### Tips

**Quick actions:**
1. Click any word in a link (not just the first word)
2. Use arrow keys for fast navigation
3. Press Esc to close menus quickly

**Nested menus:**
1. All menu levels stay visible
2. Use Left arrow to go back one level
3. Click outside to close all menus

**Learning new commands:**
1. Click objects you haven't interacted with before
2. Explore submenu categories
3. Discover roleplay and combat actions

### Limitations

- `_dialog` commands not yet supported (require text input dialogs)
- Commands not in cmdlist1.xml won't appear
- Menu positioning respects terminal boundaries

---

## Text Selection

VellumFE provides window-aware text selection that automatically copies selected text to your clipboard.

### VellumFE Text Selection (Default)

**Click and drag** (no modifiers) in any text window to select text:

| Mouse Action | Result |
|--------------|--------|
| **Click and drag** | Select text within window |
| **Mouse release** | Automatically copy to clipboard |
| **Click anywhere** | Clear selection |
| **Escape** | Clear selection |

**Features:**
- Works immediately - no setup needed
- Respects window boundaries (won't select across windows)
- Multi-line selection supported
- Automatically copies to clipboard on release
- Works with wrapped lines

**Notes:**
- Only works in text windows (not progress bars, compass, etc.)
- Selection automatically stops at window borders
- Can scroll back and select older text

### Native Terminal Selection

If you prefer your terminal emulator's native selection (which may select across VellumFE and other content):

| Mouse Action | Result |
|--------------|--------|
| **Shift + Click and drag** | Use native terminal selection |
| **Shift + Drag** | Bypasses VellumFE, uses terminal's selection |

**When to use native selection:**
- Selecting text across multiple windows
- Selecting VellumFE UI elements (borders, titles, etc.)
- Using terminal-specific features (rectangular selection, etc.)

### Copying Selected Text

**VellumFE Selection:**
- Text is automatically copied to clipboard when you release the mouse
- Paste anywhere with Ctrl+V (Windows/Linux) or Cmd+V (macOS)

**Native Terminal Selection:**
- **Windows Terminal:** Ctrl+Shift+C or right-click
- **iTerm2 (macOS):** Cmd+C or auto-copy on selection
- **GNOME Terminal:** Ctrl+Shift+C
- **Alacritty:** Ctrl+Shift+C

### Configuration

Text selection can be configured in your config file:

```toml
[ui]
selection_enabled = true  # Enable/disable VellumFE selection
selection_respect_window_boundaries = true  # Keep within single window
selection_bg_color = "#4a4a4a"  # For future visual highlighting
```

**See also:** [Text Selection Guide](Text-Selection.md) for detailed documentation

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
1. Launch vellum-fe
2. Try clicking on a window border and dragging
3. If it works, your terminal supports mouse operations

---

## Tips and Tricks

### Quick Layout Adjustment

1. Drag windows to desired positions
2. Resize as needed
3. Press `.savelayout hunting` to save
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

Some terminals support Ctrl+Shift+C for copying. Try:
1. Hold Shift and drag to select text
2. Press Ctrl+Shift+C
3. Paste to test if it copied

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
│ F12            Toggle performance stats         │
│ Ctrl+C         Exit application                 │
│ .quit          Exit application                 │
└─────────────────────────────────────────────────┘
```

---

[← Previous: Stream Routing](Stream-Routing.md) | [Next: Troubleshooting →](Troubleshooting.md)
