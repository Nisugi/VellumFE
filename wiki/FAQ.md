# Frequently Asked Questions

## General Questions

### What is VellumFE?

VellumFE is a modern, high-performance terminal frontend for GemStone IV. It connects to Lich (the Ruby scripting engine) via detached mode and provides a blazing-fast TUI with dynamic window management, custom highlights, clickable links, and full mouse support.

### Is VellumFE free?

Yes, VellumFE is free and open source.

### What platforms does VellumFE support?

VellumFE supports:
- **Windows** (Windows 10/11)
- **Linux** (most distributions)
- **macOS**

### Do I need to install Rust or build VellumFE?

No. As a user, you just need the `vellumfe.exe` (or `vellumfe` binary). Developers need Rust to build from source, but users don't.

### Can I use VellumFE without Lich?

No. VellumFE connects to Lich in detached mode. Lich handles the game connection, and VellumFE provides the user interface.

## Setup Questions

### How do I start Lich in detached mode?

**Windows:**
```powershell
C:\Ruby4Lich5\3.4.x\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8001
```

**Linux/Mac:**
```bash
ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8001
```

Replace `3.4.x` with your Ruby version and `CharacterName` with your character's name.

### How do I launch VellumFE?

```bash
.\vellumfe.exe --port 8001 --character CharacterName --links
```

- `--port` must match Lich's detachable-client port
- `--character` loads character-specific config
- `--links` enables clickable links

### Do I need to start Lich first?

Yes! Always start Lich in detached mode, wait 5-10 seconds, then launch VellumFE.

### What port should I use?

Any port from 8000-8010 works. Common choices:
- 8000 (default)
- 8001, 8002, etc. (for multiple characters)

Make sure the port matches between Lich and VellumFE.

### Can I run multiple instances?

Yes! Use different ports for each character:

**Character 1:**
```bash
# Lich: --detachable-client=8001
# VellumFE: --port 8001 --character Character1
```

**Character 2:**
```bash
# Lich: --detachable-client=8002
# VellumFE: --port 8002 --character Character2
```

## Configuration Questions

### Where are config files stored?

**Windows:**
```
C:\Users\YourName\.vellum-fe\
```

**Linux/Mac:**
```
~/.vellum-fe/
```

### How do I reset to default config?

Delete your config file:

**Windows:**
```bash
del C:\Users\YourName\.vellum-fe\configs\default.toml
```

**Linux/Mac:**
```bash
rm ~/.vellum-fe/configs/default.toml
```

VellumFE will recreate defaults on next launch.

### How do I use different configs per character?

Use the `--character` flag:

```bash
.\vellumfe.exe --port 8001 --character Nisugi
```

This creates and loads `~/.vellum-fe/configs/Nisugi.toml`.

### Where are layouts saved?

```
~/.vellum-fe/layouts/
```

Layouts are saved as TOML files: `default.toml`, `combat.toml`, etc.

### What's the auto-save layout?

VellumFE automatically saves your current layout as `auto_<character>.toml` when you quit. This has highest priority and loads automatically on next launch.

To use other layouts, delete the auto-save:
```bash
del ~/.vellum-fe/layouts/auto_CharacterName.toml
```

## Feature Questions

### Can I move and resize windows?

Yes! Click and drag:
- **Title bar** - Move window
- **Borders/corners** - Resize window

See [Mouse Controls](Mouse-Controls.md) for details.

### Can I select and copy text?

Yes! Click and drag to select text. Text automatically copies to clipboard when you release the mouse button.

Hold `Shift` while selecting to use native terminal selection (bypasses VellumFE).

### What are clickable links?

With `--links` flag, game objects become clickable. Click any word to open a context menu with available commands.

**Example:** Click "orc" → See actions like "look", "attack", "search", etc.

### How do I create custom highlights?

```bash
.addhl
```

Enter:
- Pattern (regex)
- Colors (foreground/background)
- Bold, entire line, sound options

See [Highlights](Highlights.md) for detailed guide.

### Can I add keybinds?

Yes!

```bash
.addkeybind
```

Map keys to actions or macros. See [Keybinds](Keybinds.md) for details.

### Do I need to restart for config changes?

**No restart needed:**
- Window positions/sizes
- Highlights
- Most UI settings

**Restart needed:**
- Presets
- Keybinds
- Connection settings

### Can windows overlap?

Yes. VellumFE uses absolute positioning, so windows can overlap. The window defined last in config renders on top.

To avoid overlaps, carefully plan your window positions and sizes.

### How do I create tabbed windows?

```bash
.createtabbed chat Speech:speech,Thoughts:thoughts,Whisper:whisper
```

See [Windows and Layouts](Windows-and-Layouts.md#tabbed-windows) for details.

## Performance Questions

### Why is CPU usage high?

VellumFE polls for events at ~60 FPS by default. To reduce CPU usage:

```bash
.settings
# Navigate to UI → poll_timeout_ms
# Increase from 16 to 33 or 50
```

- 16ms = ~60 FPS (high CPU, smooth)
- 33ms = ~30 FPS (medium CPU)
- 50ms = ~20 FPS (low CPU)

### Why is scrolling slow?

1. **Reduce buffer size** - Smaller buffers scroll faster
   ```bash
   .editwindow main
   # Set buffer_size to 5000 instead of 10000
   ```

2. **Increase poll timeout** - See CPU usage above

3. **Simplify highlights** - Complex regex patterns slow parsing

### Can I improve performance?

Yes:
1. Increase `poll_timeout_ms` (lower FPS, less CPU)
2. Reduce buffer sizes
3. Use "Fast Parse" for literal string highlights
4. Simplify regex patterns (use anchors: `^`, `$`)
5. Disable sounds if not needed

## Troubleshooting Questions

### Why can't I connect to Lich?

1. **Start Lich first** - Wait 5-10 seconds before launching VellumFE
2. **Port numbers match** - Lich and VellumFE must use same port
3. **Check firewall** - Ensure localhost connections allowed

See [Troubleshooting](Troubleshooting.md#connection-issues) for details.

### Why is text not appearing?

1. **Wait for Lich** - Lich needs time to fully start
2. **Check window streams** - `.windows` to verify stream subscriptions
3. **Check Lich logs** - Lich may have errors

### Why aren't my highlights working?

1. **Check pattern syntax** - Test regex at regex101.com
2. **Patterns are case-sensitive** - Use `(?i)` for case-insensitive
3. **Check window subscribes to stream** - Highlights only apply to subscribed windows
4. **Disable Fast Parse** - If using regex features, disable Fast Parse

### Why isn't mouse working?

1. **Check terminal support** - Use Windows Terminal, iTerm2, Alacritty, or Kitty
2. **Terminal settings** - Ensure mouse events enabled
3. **Try different terminal** - Some terminals have poor mouse support

### Where are debug logs?

```
~/.vellum-fe/debug.log
```

Or with `--character`:
```
~/.vellum-fe/debug_CharacterName.log
```

**View logs:**
```bash
# Windows
type C:\Users\YourName\.vellum-fe\debug.log

# Linux/Mac
cat ~/.vellum-fe/debug.log
```

## Advanced Questions

### Can I use VellumFE with other games?

VellumFE is designed for GemStone IV and expects Lich's detached mode protocol. It could theoretically work with other games using the same protocol, but hasn't been tested.

### Can I contribute to VellumFE?

Yes! VellumFE is open source. Check the GitHub repository for contribution guidelines.

### Can I write scripts for VellumFE?

VellumFE itself doesn't have scripting. Use Lich for scripting—VellumFE just displays the output.

### Does VellumFE support automation?

VellumFE is a frontend, not an automation tool. Use Lich scripts for automation. VellumFE displays the results.

### Can I customize the command input?

Yes!

```bash
.editinput
```

Edit position, size, colors, etc.

### How do I export/share my config?

Copy your config files:

```
~/.vellum-fe/configs/YourCharacter.toml
~/.vellum-fe/layouts/YourLayout.toml
```

Share these files with others. They can place them in their `.vellum-fe` directory.

### Can I have different layouts for different activities?

Yes! Create multiple layouts:

```bash
.savelayout combat
.savelayout social
.savelayout hunting
.savelayout scripting
```

Load them:
```bash
.loadlayout combat
.loadlayout social
```

### What's the difference between streams and windows?

- **Streams** - Game output channels (main, speech, thoughts, etc.)
- **Windows** - UI elements that display content

Windows subscribe to streams. Multiple windows can show the same stream, and one window can show multiple streams.

See [Advanced Streams](Advanced-Streams.md) for details.

### Can I change the default window templates?

Not directly. Templates are embedded in VellumFE. You can create windows with `.customwindow` or edit existing windows with `.editwindow`.

### How do I update VellumFE?

1. Download latest release
2. Replace `vellumfe.exe` with new version
3. Launch as normal

Your configs are separate and won't be affected.

### Does VellumFE support plugins?

Not currently. Plugin system is planned for future release.

## Comparison Questions

### VellumFE vs ProfanityFE?

**VellumFE advantages:**
- Modern Rust codebase
- Better performance
- More customization options
- Active development
- Better mouse support
- Tabbed windows

**ProfanityFE advantages:**
- More mature (older codebase)
- Larger existing user base
- More tested configurations

### VellumFE vs Wrayth?

**VellumFE advantages:**
- Terminal-based (works over SSH)
- Lower resource usage
- Open source
- Cross-platform

**Wrayth advantages:**
- Native GUI (GTK)
- More visual polish
- Rich text rendering

### VellumFE vs Wizard FE?

**VellumFE advantages:**
- Modern codebase
- Cross-platform
- Better customization
- Open source

**Wizard FE advantages:**
- Java-based (different ecosystem)
- Longer history

### VellumFE vs StormFront?

**VellumFE advantages:**
- Free and open source
- Works with Lich
- Highly customizable
- Terminal-based (SSH-friendly)

**StormFront advantages:**
- Official client
- Native GUI
- Built-in game integration

## Contact and Support

### How do I report bugs?

GitHub Issues: https://github.com/your-repo/vellumfe/issues

Include:
- VellumFE version
- Operating system
- Terminal emulator
- Steps to reproduce
- Debug log excerpt

### How do I request features?

GitHub Issues (feature request): https://github.com/your-repo/vellumfe/issues

### Where can I get help?

1. **This wiki** - Check relevant pages
2. **Debug logs** - `~/.vellum-fe/debug.log`
3. **GitHub Issues** - Search existing issues
4. **Community Discord** - Real-time help

### How do I stay updated?

- Watch GitHub repository for releases
- Join community Discord
- Check changelog regularly

## See Also

- [Getting Started](Getting-Started.md) - Setup guide
- [Troubleshooting](Troubleshooting.md) - Problem solving
- [Configuration](Configuration.md) - Config reference
- [Commands Reference](Commands.md) - All commands
