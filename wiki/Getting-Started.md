# Getting Started with VellumFE

This guide will help you get VellumFE up and running for the first time.

## Prerequisites

Before launching VellumFE, you need:

1. **Lich** - The Ruby scripting engine for GemStone IV
2. **GemStone IV Account** - Active game account
3. **VellumFE Binary** - The `vellumfe.exe` executable

## Installation

1. Download the latest VellumFE release
2. Extract `vellumfe.exe` to a folder of your choice
3. That's it! No additional installation required

## First Launch

### Step 1: Start Lich in Detached Mode

Before launching VellumFE, you must start Lich in detached mode:

**Windows (PowerShell or Command Prompt):**
```powershell
C:\Ruby4Lich5\3.4.x\bin\rubyw.exe C:\Path\To\Lich5\lich.rbw --login YourCharacterName --gemstone --without-frontend --detachable-client=8001
```

**Important Notes:**
- Replace `3.4.x` with your actual Ruby version (e.g., `3.4.2`, `3.4.5`)
- Replace `YourCharacterName` with your character's name

**Linux/Mac:**
```bash
ruby ~/lich5/lich.rbw --login YourCharacterName --gemstone --without-frontend --detachable-client=8001
```

### Step 2: Launch VellumFE

Once Lich is running, launch VellumFE:

```bash
.\vellumfe.exe --port 8001 --character YourCharacterName --links
```

**Command-Line Arguments:**
- `--port` or `-p` - Port number (must match Lich's detachable-client port, default: 8000)
- `--character` or `-c` - Character name (loads character-specific config, optional)
- `--links` - Enable clickable links with context menus (recommended)

**Examples:**
```bash
# Basic launch (uses default config)
.\vellumfe.exe --port 8001

# Character-specific config
.\vellumfe.exe --port 8001 --character Nisugi

# All options
.\vellumfe.exe --port 8001 --character Nisugi --links
```

### Step 3: Verify Connection

After launching, you should see:
- VellumFE connects to Lich
- Game text appears in the main window
- Progress bars show your vitals (health, mana, etc.)
- You can type commands in the input box at the bottom

If you don't see game text, check:
1. Lich is running and fully started (starting VellumFE too soon)
2. Port numbers match between Lich and VellumFE
3. No firewall blocking localhost connections

## Basic Controls

### Keyboard

- **Type commands** - Just start typing in the command input
- **Send command** - Press `Enter`
- **Tab** - Cycle focus between windows
- **Page Up/Down** - Scroll in focused window
- **Ctrl+S** - Save selection
- **Ctrl+C** - Closes VellumFE
- **Esc** - Clear selection or close popups

### Mouse

- **Scroll** - Mouse wheel over any window
- **Move window** - Click and drag the title bar
- **Resize window** - Click and drag borders or corners
- **Select text** - Click and drag to select (auto-copies on release)
- **Click links** - Left-click any highlighted word (if `--links` enabled)
- **Switch tabs** - Click tab names in tabbed windows
- **Drag & Drop** - Ctrl + Left-click and drag links for wrayth style drag & drop

### Dot Commands

VellumFE has special commands that start with `.` - these are handled locally and not sent to the game:

- `.quit` - Exit VellumFE saving profile
- `.menu` - Access settings
- `.help` - Show help (lists all dot commands)

See [Commands Reference](Commands.md) for the complete list.

## First Steps

### 1. Explore the Default Layout

The default layout includes:
- **Main window** - Game output
- **Room window** - Room descriptions
- **Vitals bars** - Health, mana, stamina, spirit
- **Roundtime countdown** - Shows remaining roundtime
- **Command input** - Bottom of screen

### 2. Try Moving a Window

1. Click and hold on a window's title bar
2. Drag to move it
3. Release to place it

### 3. Try Resizing a Window

1. Click and hold on a window's border or corner
2. Drag to resize
3. Release when satisfied

### 4. Open Settings

Type `.settings` and press Enter to explore configuration options:
- Colors and themes
- Sound settings
- Connection settings
- Preset colors

Navigate with arrow keys, press Enter to edit values.

### 5. Save Your Layout

After arranging windows, save your layout:
```
.savelayout mylayout
```

Load it later with:
```
.loadlayout mylayout
```

## Next Steps

Now that you're up and running, check out:

- [Windows and Layouts](Windows-and-Layouts.md) - Learn about window types and positioning
- [Configuration](Configuration.md) - Customize colors, presets, and more
- [Highlights](Highlights.md) - Set up custom text highlights
- [Commands Reference](Commands.md) - Master all dot commands
- [Mouse Controls](Mouse-Controls.md) - Advanced mouse operations

## Quick Tips

1. **Auto-save on exit** - VellumFE automatically saves your layout when you quit (as `auto_<character>.toml`)
2. **Character-specific configs** - Use `--character` to keep separate settings per character
3. **Debug logs** - Check `~/.vellum-fe/debug_<character>.log` if something goes wrong
4. **Shift+Mouse** - Hold Shift while selecting to use native terminal selection (bypasses VellumFE)
5. **Tab navigation** - Use Tab key to cycle window focus for keyboard scrolling

## Common Issues

### "Connection failed" or "Connection refused"
- Make sure Lich is running first
- Wait 5-10 seconds after starting Lich
- Verify port numbers match

### "No game text appearing"
- Lich may still be starting - wait a bit longer
- Check Lich logs for errors
- Try restarting Lich in detached mode

### "Can't move/resize windows"
- Make sure you're clicking the title bar (for move) or borders (for resize)
- Title bar excludes corners - click in the middle
- Your terminal must support mouse events

### "Clickable links not working"
- Make sure you launched with `--links` flag
- Links only work for game objects wrapped in `<a>` tags
- Not all text is clickable, only tagged objects

See [Troubleshooting](Troubleshooting.md) for more help.
