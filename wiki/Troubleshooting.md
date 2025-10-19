# Troubleshooting Guide

This guide helps you diagnose and fix common issues with vellum-fe.

## Table of Contents

- [Connection Issues](#connection-issues)
- [Window Issues](#window-issues)
- [Performance Issues](#performance-issues)
- [Display Issues](#display-issues)
- [Input Issues](#input-issues)
- [Configuration Issues](#configuration-issues)
- [Known Issues](#known-issues)
- [Getting Help](#getting-help)

---

## Connection Issues

### Cannot Connect to Lich

**Symptoms:**
- Application shows "Connecting..." indefinitely
- Error: "Connection refused"
- Error: "Connection reset by peer"

**Causes & Solutions:**

#### 1. Lich Not Running

**Solution:**
- Start Lich in detached mode **before** launching vellum-fe
- Wait 5-10 seconds after starting Lich before connecting

**Windows:**
```powershell
# Note: Replace 3.4.x with your actual Ruby version (e.g., 3.4.2, 3.4.5, etc.)
C:\Ruby4Lich5\3.4.x\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

**Linux/Mac:**
```bash
ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

#### 2. Wrong Port

**Solution:**
- Check that Lich is using port 8000 (or your configured port)
- Verify in `config.toml`:
```toml
[connection]
port = 8000  # Must match Lich's --detachable-client=8000
```

#### 3. Wrong Host

**Solution:**
- If Lich is on another machine, update the host:
```toml
[connection]
host = "192.168.1.100"  # IP of machine running Lich
```

#### 4. Firewall Blocking Connection

**Solution:**
- Check firewall settings on both machines
- Allow TCP port 8000 (or your configured port)
- Test with: `telnet localhost 8000` (should connect if Lich is running)

### Connection Drops

**Symptoms:**
- Connected, then disconnected unexpectedly
- "Connection reset by peer"

**Causes & Solutions:**

#### 1. Lich Crashed or Exited

**Solution:**
- Check if Lich is still running
- Restart Lich if needed
- Restart vellum-fe

#### 2. Network Interruption

**Solution:**
- Check network connection
- If Lich is remote, verify network stability
- Consider using a local Lich instance

#### 3. Game Server Disconnection

**Solution:**
- If GemStone IV disconnects you, Lich may close the connection
- Log back in through Lich
- Restart vellum-fe

---

## Window Issues

### Window Not Receiving Text

**Symptoms:**
- Window exists but shows no text
- Text appears in wrong window

**Causes & Solutions:**

#### 1. Stream Routing Issue

**Diagnosis:**
- Check which streams the window subscribes to
- Use `.windows` to list active windows

**Solution:**
```toml
# Edit config.toml
[[ui.windows]]
name = "mywindow"
streams = ["main", "thoughts"]  # Add missing streams
```

Or use `.deletewindow` and `.customwindow` to recreate:
```
.deletewindow mywindow
.customwindow mywindow main,thoughts
```

See [Stream Routing Guide](Stream-Routing.md) for details.

#### 2. Stream Conflict

**Diagnosis:**
- Each stream can only route to one window
- Later windows in config "steal" streams from earlier windows

**Solution:**
- Search `config.toml` for duplicate stream names
- Remove stream from one window
- Or create custom routing

#### 3. Parser Issue

**Diagnosis:**
- Game may be using a new/unknown stream name
- Enable debug logs to see stream names

**Solution:**
```bash
RUST_LOG=debug cargo run
# Check ~/.vellum-fe/debug.log for stream names
```

### Cannot Move or Resize Window

**Symptoms:**
- Clicking and dragging doesn't move window
- Clicking edges doesn't resize

**Causes & Solutions:**

#### 1. Terminal Doesn't Support Mouse

**Solution:**
- Use a terminal with mouse support:
  - Windows Terminal ✅
  - iTerm2 (macOS) ✅
  - Alacritty ✅
  - GNOME Terminal ✅
- Avoid: CMD.exe, PowerShell ISE

#### 3. Clicking Wrong Area

**Solution:**
- **To move:** Click title bar (top border, not corners)
- **To resize:** Click edges or corners
- Title bar excludes 1 cell on each side (corners reserved for resize)

### Window Off-Screen

**Symptoms:**
- Window defined in config but not visible
- Window moved off-screen during resizing

**Causes & Solutions:**

#### 1. Window Outside Terminal Bounds

**Solution:**

**Option A: Edit config.toml**
```toml
[[ui.windows]]
name = "mywindow"
row = 0    # Reset to top-left
col = 0
rows = 10
cols = 40
```

**Option B: Delete and Recreate**
```
.deletewindow mywindow
.createwindow mywindow
```

#### 2. Terminal Too Small

**Solution:**
- Resize terminal to be larger
- Windows with `row` or `col` beyond terminal size won't render
- Minimum recommended: 80 columns × 24 rows

### Window Border Not Showing

**Symptoms:**
- Window has no border
- Title not visible

**Causes & Solutions:**

#### 1. Border Disabled in Config

**Solution:**
```toml
[[ui.windows]]
name = "mywindow"
show_border = true  # Enable border
border_style = "single"  # Set style
```

Or use `.border` command:
```
.border mywindow single
```

#### 2. Border Style "none"

**Solution:**
```
.border mywindow single  # Change to single, double, rounded, or thick
```

---

## Performance Issues

### High CPU Usage

**Symptoms:**
- CPU usage constantly high (>50%)
- Fans spinning up
- System feels sluggish

**Causes & Solutions:**

#### 1. Too Many Windows

**Solution:**
- Delete unused windows: `.deletewindow <name>`
- Reduce number of active windows
- Use combined windows (multiple streams in one window)

#### 2. Very Large Scrollback Buffers

**Solution:**
```toml
# Reduce buffer_size in config.toml
[[ui.windows]]
name = "main"
buffer_size = 1000  # Reduce from 10000
```

#### 3. High Game Activity

**Solution:**
- This is normal during high-traffic periods (lots of combat, chat, etc.)
- Performance should improve during idle periods

### Slow Scrolling

**Symptoms:**
- Lag when scrolling with Page Up/Down or mouse wheel
- Text takes time to render when scrolling

**Causes & Solutions:**

#### 1. Large Buffer with Long Lines

**Solution:**
- Reduce `buffer_size` in window config
- Wider windows with more text are slower to render

#### 2. Terminal Rendering

**Solution:**
- Try a faster terminal emulator:
  - Alacritty (fastest)
  - Kitty
  - Windows Terminal
- Disable terminal transparency
- Disable terminal blur effects

### Memory Usage Growing

**Symptoms:**
- Memory usage increases over time
- Eventually causes slowdown or crash

**Causes & Solutions:**

#### 1. Unbounded Scrollback

**Solution:**
- Set reasonable `buffer_size` limits:
```toml
[[ui.windows]]
buffer_size = 5000  # Not 50000 or unlimited
```

#### 2. Memory Leak (Bug)

**Solution:**
- Restart vellum-fe periodically
- Report issue on GitHub with steps to reproduce

---

## Display Issues

### Colors Not Showing Correctly

**Symptoms:**
- Colors appear wrong or washed out
- All text is white/gray
- Background colors don't work

**Causes & Solutions:**

#### 1. Terminal Color Support

**Solution:**
- Use a terminal with 24-bit color support:
  - Windows Terminal ✅
  - iTerm2 ✅
  - Alacritty ✅
  - Kitty ✅
- Check terminal settings for color mode

#### 2. Preset Colors Not Defined

**Solution:**
```toml
# Add presets to config.toml
[[presets]]
id = "speech"
fg = "#53a684"
```

See [Configuration Guide](Configuration-Guide.md#preset-colors) for common presets.

### Text Wrapping Issues

**Symptoms:**
- Lines don't wrap correctly
- Text cut off at window edge
- Wrapped text loses color

**Causes & Solutions:**

#### 1. Window Too Narrow

**Solution:**
- Resize window to be wider
- Minimum 20 columns recommended

#### 2. Known Wrapping Bug

**Notes:**
- Styled text should maintain color through wrapping
- If not, this is a bug - please report

### Progress Bars Not Updating

**Symptoms:**
- Health/mana/stamina bars stuck at 0 or old value
- Bars don't change when stats change in-game

**Causes & Solutions:**

#### 1. Game Not Sending Updates

**Solution:**
- Type `health` in-game to trigger an update
- Type `mana`, `stamina`, etc.
- Check debug logs for `<progressBar>` tags

#### 2. Wrong Window Name

**Solution:**
- Progress bars must have specific names to auto-update:
  - `health`, `mana`, `stamina`, `spirit`
  - `mindstate` (or `mind`)
  - `encumbrance` (or `encumlevel`)
  - `stance`
  - `bloodpoints` (or `lblBPs` or `blood`)

#### 3. Parser Issue

**Solution:**
- Enable debug logging:
```bash
RUST_LOG=debug cargo run
```
- Check `~/.vellum-fe/debug.log` for `progressBar` entries
- Report if tags are being received but not updating bars

### Countdown Timers Not Working

**Symptoms:**
- Roundtime/casttime/stun timers stuck
- Timers don't count down

**Causes & Solutions:**

#### 1. Game Not Sending Updates

**Solution:**
- Perform an action that triggers roundtime (attack, cast spell, etc.)
- Check debug logs for `<roundTime>` or `<castTime>` tags

#### 2. Wrong Window Name

**Solution:**
- Countdown windows must have specific names:
  - `roundtime` - for roundtime
  - `casttime` - for cast time
  - `stun` - for stun timer

#### 3. System Time Issues

**Solution:**
- Verify system clock is correct
- Countdown timers use Unix timestamps from game server
- Incorrect system time causes wrong countdown values

---

## Input Issues

### Commands Not Sending

**Symptoms:**
- Typing and pressing Enter does nothing
- Commands appear but aren't sent to game

**Causes & Solutions:**

#### 1. Not Connected

**Solution:**
- Check connection status
- Verify Lich is running
- Restart connection

#### 2. Focus Not on Command Input

**Solution:**
- Click on the command input area
- Type directly (don't need to click first in most cases)

### Command History Not Working

**Symptoms:**
- Up/Down arrows don't cycle through history
- Previous commands not saved

**Causes & Solutions:**

#### 1. No Commands in History Yet

**Solution:**
- Type and send at least one command first
- History builds as you send commands

#### 2. Arrows Controlling Window Scroll

**Solution:**
- Ensure focus is on command input, not a window
- Click in command input area

### Cannot Copy Text

**Symptoms:**
- Clicking and dragging doesn't select text
- Ctrl+C doesn't copy

**Causes & Solutions:**

#### 1. Use VellumFE Text Selection

**Solution:**
- Hold Shift while dragging to select text
- Text is automatically copied to clipboard
- See [Text Selection](Text-Selection.md) for details

#### 2. Terminal Doesn't Support Copy

**Solution:**
- Try terminal's copy shortcut:
  - Windows Terminal: Ctrl+Shift+C
  - iTerm2: Cmd+C
  - GNOME Terminal: Ctrl+Shift+C
- Try right-click → Copy
- Check terminal documentation

---

## Configuration Issues

### Config File Not Found

**Symptoms:**
- Application creates new default config each launch
- Changes to config.toml not taking effect

**Causes & Solutions:**

#### 1. Editing Wrong File

**Solution:**
- Verify config location:
  - Linux/Mac: `~/.vellum-fe/config.toml`
  - Windows: `C:\Users\YourName\.vellum-fe\config.toml`
- Use absolute path to be sure

#### 2. Syntax Error in Config

**Solution:**
- TOML syntax is strict
- Check for:
  - Missing quotes around strings
  - Missing commas in arrays
  - Mismatched brackets
- Use a TOML validator or linter
- Check application output for parsing errors

**Example errors:**
```toml
# Wrong
streams = [main, thoughts]

# Right
streams = ["main", "thoughts"]
```

### Layout Not Saving

**Symptoms:**
- `.savelayout` command succeeds but layout not saved
- `.loadlayout` says layout not found

**Causes & Solutions:**

#### 1. Layouts Directory Doesn't Exist

**Solution:**
- Create layouts directory:
```bash
mkdir -p ~/.vellum-fe/layouts
```

#### 2. Permission Issue

**Solution:**
- Check file permissions on `~/.vellum-fe/` directory
- Ensure application has write access

#### 3. Invalid Layout Name

**Solution:**
- Use alphanumeric names (no special characters)
- Avoid spaces in layout names
```
# Good
.savelayout hunting
.savelayout combat_stance_1

# Bad
.savelayout my layout
.savelayout combat/stance
```

---

## Known Issues

### Known Bugs

#### 1. Blank Lines in Parser

**Issue:** Some blank lines in game output are not preserved correctly.

**Workaround:** None currently.

**Status:** Planned fix - see [Feature Roadmap](Feature-Roadmap.md)

#### 2. Duplicate Prompts

**Issue:** Prompts may appear twice when thoughts stream is active.

**Workaround:** Filter prompts or use custom stream routing.

**Status:** Planned fix - see [Feature Roadmap](Feature-Roadmap.md)

#### 3. Windows Can Overlap

**Issue:** Windows use absolute positioning and can overlap.

**Workaround:** This is intentional behavior. Manually position windows to avoid overlap.

**Status:** By design.

#### 4. No Window Z-Ordering

**Issue:** Cannot control which window appears "on top" when overlapping.

**Workaround:** Windows render in config file order. Place important windows later in config.

**Status:** Not planned.

### Platform-Specific Issues

#### Windows

**Issue:** CMD.exe has limited mouse support.

**Solution:** Use Windows Terminal, Alacritty, or another modern terminal.

**Issue:** Unicode characters may not display correctly.

**Solution:**
- Use a Nerd Font (for countdown icons)
- Ensure terminal is set to UTF-8 encoding

#### Linux

**Issue:** Some terminals don't support mouse tracking.

**Solution:** Use GNOME Terminal, Konsole, Kitty, or Alacritty.

#### macOS

**Issue:** Default Terminal.app has basic mouse support.

**Solution:** Use iTerm2 or Alacritty for full mouse support.

---

## Getting Help

### Enable Debug Logging

For any issue, debug logs help diagnose the problem:

```bash
RUST_LOG=debug cargo run
```

Logs are written to: `~/.vellum-fe/debug.log`

### Check Debug Log

```bash
# Linux/Mac
tail -f ~/.vellum-fe/debug.log

# Windows (PowerShell)
Get-Content ~/.vellum-fe/debug.log -Tail 50 -Wait
```

Look for:
- Connection errors
- Parse errors
- Stream routing info
- XML tag processing

### Report an Issue

If you found a bug:

1. **Check existing issues:** https://github.com/yourusername/vellum-fe/issues
2. **Gather information:**
   - Your OS and terminal emulator
   - Steps to reproduce the issue
   - Debug log excerpt (if relevant)
   - Config file (if relevant)
3. **Create a new issue** with:
   - Clear title describing the problem
   - Detailed description
   - Steps to reproduce
   - Expected vs. actual behavior
   - Any error messages or log output

### Get Community Help

- **Discord:** (Link if available)
- **Forums:** (Link if available)
- **GitHub Discussions:** (Link if available)

### Useful Diagnostic Commands

```
.windows          - List active windows
.templates        - List available templates
.layouts          - List saved layouts

.randomprogress   - Test progress bars
.randomcountdowns - Test countdown timers
.randomcompass    - Test compass
.randominjuries   - Test injury doll
```

---

[← Previous: Mouse and Keyboard](Mouse-and-Keyboard.md) | [Next: Development Guide →](Development-Guide.md)
