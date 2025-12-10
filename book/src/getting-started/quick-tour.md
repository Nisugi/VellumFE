# Quick Tour

A 5-minute introduction to VellumFE's essential controls and features.

## The Interface

```
┌─────────────────────────────────────────────────────────────────────────┐
│ VellumFE v0.1.0                                                    [?]  │
├─────────────────────────────────────────────────────────────────────────┤
│ ┌─ Main ───────────────────────────┐ ┌─ Room ─────────────────────────┐ │
│ │                                  │ │ Town Square Central            │ │
│ │ A goblin attacks you!            │ │ This is the heart of the town. │ │
│ │ You swing your sword at goblin!  │ │ Obvious paths: n, e, s, w, out │ │
│ │ You hit for 45 damage!           │ │                                │ │
│ │ The goblin falls dead!           │ └────────────────────────────────┘ │
│ │                                  │ ┌─ Vitals ───┐ ┌─ Compass ──────┐ │
│ │ >                                │ │ HP ████████│ │     [N]        │ │
│ │                                  │ │ MP ██████░░│ │  [W] + [E]     │ │
│ │                                  │ │ ST ████████│ │     [S]        │ │
│ └──────────────────────────────────┘ └────────────┘ └────────────────┘ │
│ ┌─ Input ──────────────────────────────────────────────────────────────┐│
│ │ > attack goblin                                                      ││
│ └──────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Essential Keybinds

### Navigation

| Key | Action |
|-----|--------|
| `Page Up` / `Page Down` | Scroll main window |
| `Home` / `End` | Jump to top/bottom |
| `Scroll Wheel` | Scroll focused window |
| `Ctrl+Tab` | Cycle through windows |
| `Tab` | Focus next tabbed window |

### Input

| Key | Action |
|-----|--------|
| `Enter` | Send command |
| `Up` / `Down` | Command history |
| `Ctrl+C` | Copy selection |
| `Ctrl+V` | Paste |
| `Escape` | Clear input / Cancel |

### Menus & Popups

| Key | Action |
|-----|--------|
| `Ctrl+M` | Open main menu |
| `Ctrl+H` | Open highlight browser |
| `Ctrl+K` | Open keybind browser |
| `Ctrl+E` | Open window editor |
| `Ctrl+?` | Show keybind help |
| `Escape` | Close popup |

### Client Control

| Key | Action |
|-----|--------|
| `Ctrl+Q` | Quit VellumFE |
| `F5` | Reload configuration |
| `Ctrl+L` | Clear main window |

---

## Try It Now

### 1. Send a Command
Type `look` and press Enter. The room description appears in the main window.

### 2. Scroll History
Press `Page Up` to scroll back through game output. Press `End` to return to the bottom.

### 3. Use Command History
Press `Up Arrow` to recall your previous command. Press `Down Arrow` to go forward.

### 4. Open the Menu
Press `Ctrl+M`. Use arrow keys to navigate, Enter to select, Escape to close.

### 5. Focus a Window
Click on a window or use `Ctrl+Tab` to cycle focus. The focused window has a highlighted border.

---

## Window Types

VellumFE displays several types of windows:

### Text Windows
Scrollable text output (main game feed, thoughts, speech).

**Controls:**
- Scroll with Page Up/Down or mouse wheel
- Select text by clicking and dragging
- Copy selection with Ctrl+C

### Progress Bars
Visual health, mana, stamina, etc.

**Display:**
```
HP ████████████████ 100%
MP ██████████░░░░░░  67%
```

### Compass
Shows available exits with directional indicators.

### Hands
Displays items in your left/right hands and prepared spell.

### Indicators
Status icons for conditions (stunned, hidden, etc.).

---

## The Main Menu

Press `Ctrl+M` to access:

```
┌─ Main Menu ────────────────────────┐
│ ► Window Editor                    │
│   Highlight Browser                │
│   Keybind Browser                  │
│   Color Browser                    │
│   ─────────────────────────        │
│   Reload Configuration             │
│   ─────────────────────────        │
│   Quit                             │
└────────────────────────────────────┘
```

| Option | Description |
|--------|-------------|
| Window Editor | Modify window positions, sizes, styles |
| Highlight Browser | View and edit text highlighting rules |
| Keybind Browser | View and edit keybindings |
| Color Browser | View and edit color settings |
| Reload Configuration | Apply changes from config files |
| Quit | Exit VellumFE |

---

## Quick Customization

### Change a Keybind
1. Press `Ctrl+K` to open the keybind browser
2. Navigate to the keybind you want to change
3. Press Enter to edit
4. Press the new key combination
5. Press Escape to close

### Edit Highlights
1. Press `Ctrl+H` to open the highlight browser
2. Find an existing highlight or add a new one
3. Edit the pattern, colors, or conditions
4. Changes take effect immediately

### Adjust a Window
1. Press `Ctrl+E` to open the window editor
2. Select a window from the list
3. Modify position, size, border, colors
4. Press Escape to close (changes auto-save)

---

## Keyboard Reference Card

Print this for quick reference:

```
┌─────────────────────────────────────────────────────────────┐
│                  VELLUM-FE QUICK REFERENCE                    │
├─────────────────────────────────────────────────────────────┤
│ NAVIGATION          │ MENUS              │ INPUT             │
│ PgUp/PgDn  Scroll   │ Ctrl+M  Main Menu  │ Enter   Send      │
│ Home/End   Top/Bot  │ Ctrl+H  Highlights │ Up/Down History   │
│ Ctrl+Tab   Windows  │ Ctrl+K  Keybinds   │ Ctrl+C  Copy      │
│ Tab        Tabs     │ Ctrl+E  Editor     │ Ctrl+V  Paste     │
│                     │ Escape  Close      │ Escape  Clear     │
├─────────────────────┴────────────────────┴───────────────────┤
│ F5 = Reload Config    Ctrl+L = Clear    Ctrl+Q = Quit       │
└─────────────────────────────────────────────────────────────┘
```

---

## Next Steps

Now that you know the basics:

1. **[Customize Your Layout](../customization/creating-layouts.md)** - Make it yours
2. **[Set Up Highlights](../customization/highlight-patterns.md)** - Color your world
3. **[Configure Keybinds](../customization/keybind-actions.md)** - Optimize your workflow
4. **[Explore Widgets](../widgets/README.md)** - Learn all widget types

---

## See Also

- [Keybind Actions Reference](../reference/keybind-actions.md) - Complete keybind list
- [Configuration Guide](../configuration/README.md) - Config file details
- [Tutorials](../tutorials/README.md) - Step-by-step guides
