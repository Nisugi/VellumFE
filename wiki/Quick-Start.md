# Quick Start Guide

Get up and running with vellum-fe in 5 minutes.

## Step 1: Start Lich in Detached Mode

vellum-fe connects to Lich via its detached client mode. You **must** start Lich first.

### Windows (PowerShell)

```powershell
C:\Ruby4Lich5\3.4.5\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

### Linux/Mac

```bash
ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

**Important:** Wait 5-10 seconds after launching Lich before starting vellum-fe.

## Step 2: Launch vellum-fe

```bash
# From the project directory
cargo run --release

# Or run the binary directly
./target/release/vellum-fe
```

You should see:
- Connection status message
- Default "Main" window appears
- Command input at the bottom

## Step 3: Create Your First Windows

vellum-fe starts with just a main window. Let's add more:

```
.createwindow health
.createwindow mana
.createwindow roundtime
.createwindow thoughts
.createwindow speech
```

Each window appears at position (0,0) by default. Move them around with your mouse!

## Step 4: Move and Resize Windows

**To move a window:**
1. Click and hold the window's title bar (top border)
2. Drag to new position
3. Release

**To resize a window:**
1. Click and hold a window's edge or corner
2. Drag to resize
3. Release

## Step 5: Save Your Layout

Once you have windows positioned how you like:

```
.savelayout default
```

This saves your layout to `~/.vellum-fe/layouts/default.toml`.

On next launch, your layout will be restored automatically (autosave).

## Step 6: Start Playing

You're ready! Type commands as normal. Game output will route to the appropriate windows:

- **Main** - General game text
- **Thoughts** - Your character's thoughts
- **Speech** - Speech and whispers
- **Vitals** - Auto-update from game data
- **Roundtime** - Auto-updates when you perform actions

## Basic Controls

### Keyboard

- **Arrow keys** - Navigate command input
- **Up/Down** - Command history
- **PageUp/PageDown** - Scroll focused window
- **Tab** - Cycle focus between windows
- **Ctrl+C** - Quit application

### Mouse

- **Click** - Focus a window
- **Scroll wheel** - Scroll window content
- **Drag title bar** - Move window
- **Drag edge/corner** - Resize window
- **Shift+drag** - Select text for copying

## Quick Command Reference

```
.createwindow <template>     # Create a window from template
.deletewindow <name>         # Delete a window
.windows                     # List all windows
.templates                   # List available templates
.savelayout <name>           # Save current layout
.loadlayout <name>           # Load a saved layout
.quit                        # Exit vellum-fe
```

## Explore More Widgets

vellum-fe has 40+ pre-built widgets. Try these:

```
.createwindow compass        # Visual exit compass
.createwindow injuries       # Injury doll
.createwindow active_spells  # Active spell effects
.createwindow loot           # Loot messages
.createwindow performance    # Performance stats
```

Use `.templates` to see the full list.

## Tips for New Users

1. **Mouse mode is enabled by default** - No need to toggle
2. **Windows can overlap** - This is intentional (absolute positioning)
3. **Autosave on exit** - Use `.quit` or Ctrl+C to save layout
4. **Closing terminal with X button kills the process** - Layout won't save!
5. **Config file** - `~/.vellum-fe/config.toml` for advanced customization

## Next Steps

- **[Window Management](https://github.com/Nisugi/VellumFE/wiki/Window-Management)** - Learn all window operations
- **[Widget Reference](https://github.com/Nisugi/VellumFE/wiki/Widget-Reference)** - Explore all 40+ widgets
- **[Commands Reference](https://github.com/Nisugi/VellumFE/wiki/Commands-Reference)** - Complete command list
- **[Configuration Guide](https://github.com/Nisugi/VellumFE/wiki/Configuration-Guide)** - Customize your setup

## Troubleshooting

**"Connection refused"**
- Ensure Lich is running in detached mode
- Wait 5-10 seconds after launching Lich
- Check port (default: 8000)

**"Can't move windows"**
- Ensure your terminal supports mouse input
- Try Windows Terminal (Windows) or iTerm2 (Mac)

**"Windows don't appear"**
- Check terminal size (must be large enough)
- Use `.windows` to list active windows
- Windows might be off-screen - try resizing terminal

See [Troubleshooting Guide](https://github.com/Nisugi/VellumFE/wiki/Troubleshooting) for more help.

---

← [Installation](https://github.com/Nisugi/VellumFE/wiki/Installation) | [Window Management](https://github.com/Nisugi/VellumFE/wiki/Window-Management) →
