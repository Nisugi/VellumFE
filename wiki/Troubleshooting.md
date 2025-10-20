# Troubleshooting Guide

This guide covers common issues and solutions for VellumFE.

## Connection Issues

### "Connection failed" or "Connection refused"

**Symptoms:**
- VellumFE launches but shows connection error
- No game text appearing
- "Failed to connect to localhost:8000"

**Solutions:**

1. **Start Lich first**
   ```bash
   # Wait 5-10 seconds after starting Lich before launching VellumFE
   C:\Ruby4Lich5\3.4.x\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login YourCharacter --gemstone --without-frontend --detachable-client=8001
   ```

2. **Verify port numbers match**
   ```bash
   # Lich: --detachable-client=8001
   # VellumFE: --port 8001
   .\vellumfe.exe --port 8001
   ```

3. **Check Lich is running**
   - Windows: Task Manager → Look for `rubyw.exe`
   - Linux/Mac: `ps aux | grep lich`

4. **Check firewall**
   - Ensure localhost connections allowed
   - No firewall blocking port 8000-8010

5. **Try different port**
   ```bash
   # Lich with port 8002
   --detachable-client=8002
   # VellumFE with same port
   --port 8002
   ```

### "Connection lost" during play

**Symptoms:**
- Connected successfully, then disconnected
- Game text stops appearing
- Connection dropped message

**Solutions:**

1. **Check Lich is still running**
   - Lich may have crashed
   - Restart Lich in detached mode

2. **Network interruption**
   - Even localhost can have hiccups
   - Restart both Lich and VellumFE

3. **Check debug log**
   ```bash
   # Windows
   type C:\Users\YourName\.vellum-fe\debug.log

   # Linux/Mac
   cat ~/.vellum-fe/debug.log
   ```

## Display Issues

### No game text appearing

**Symptoms:**
- VellumFE launches, connects successfully
- Windows visible but empty
- No text in main window

**Solutions:**

1. **Wait for Lich to fully start**
   - Lich needs 5-10 seconds to initialize
   - Wait before launching VellumFE

2. **Check window streams**
   ```bash
   .windows  # List windows and their streams
   ```
   - Main window should subscribe to "main" stream
   - If no window subscribes to stream, text is discarded

3. **Verify window visibility**
   - Window may be off-screen
   - Check terminal size vs window position
   - Try `.loadlayout` to reset

4. **Check Lich output**
   - Look at Lich's console for errors
   - Verify Lich is connected to game

### Text colors not showing

**Symptoms:**
- Text appears but all same color
- Presets/highlights not working
- Everything white or default color

**Solutions:**

1. **Check terminal color support**
   - Some terminals have limited colors
   - Use Windows Terminal, iTerm2, Alacritty, or Kitty

2. **Verify preset colors in config**
   ```bash
   .settings
   # Navigate to Presets section
   # Check colors are valid hex (#RRGGBB)
   ```

3. **Check highlight syntax**
   ```bash
   .highlights
   # Verify FG/BG colors are valid
   ```

4. **Terminal theme interference**
   - Some terminal themes override colors
   - Try different terminal theme
   - Disable terminal theme color modifications

### Window borders missing or wrong

**Symptoms:**
- Borders not showing
- Wrong border style
- Borders color wrong

**Solutions:**

1. **Check show_border setting**
   ```bash
   .editwindow main
   # Verify show_border is checked
   ```

2. **Check border_style**
   ```bash
   .border main single
   .border main double
   .border main rounded
   ```

3. **Terminal font support**
   - Some fonts don't support box-drawing characters
   - Use terminal with good Unicode support
   - Try different font (Cascadia Code, Fira Code, JetBrains Mono)

### Text wrapping incorrectly

**Symptoms:**
- Text cuts off mid-word
- Wrapping at wrong position
- Lines too long or too short

**Solutions:**

1. **Check window width**
   ```bash
   .editwindow main
   # Verify cols value
   ```

2. **Borders consume space**
   - If `show_border = true`, inner width = cols - 2
   - Adjust cols to account for borders

3. **Terminal size mismatch**
   - VellumFE uses terminal's reported size
   - Resize terminal or adjust window sizes

## Performance Issues

### High CPU usage

**Symptoms:**
- VellumFE using excessive CPU
- Fan noise / heat
- Slow performance

**Solutions:**

1. **Increase poll timeout**
   ```bash
   .settings
   # Navigate to UI → poll_timeout_ms
   # Increase from 16 to 33 or 50
   ```
   - 16ms = ~60 FPS (high CPU)
   - 33ms = ~30 FPS (medium CPU)
   - 50ms = ~20 FPS (low CPU)

2. **Reduce highlight count**
   - Too many highlights slow parsing
   - Enable "fast_parse" for literal strings
   ```bash
   .highlights
   # Edit highlights, enable Fast Parse for simple patterns
   ```

3. **Simplify regex patterns**
   - Complex regex with backtracking is slow
   - Use anchors (^, $) for efficiency
   - Use word boundaries (\b)

4. **Disable sounds**
   ```bash
   .settings
   # Sound → Enabled = false
   ```

### Slow scrolling

**Symptoms:**
- Mouse wheel scrolling laggy
- PgUp/PgDn slow to respond
- Text takes time to appear

**Solutions:**

1. **Reduce buffer size**
   ```bash
   .editwindow main
   # Set buffer_size to lower value (e.g., 5000 instead of 10000)
   ```

2. **Increase poll timeout**
   - See High CPU usage above

3. **Check terminal performance**
   - Some terminals render slowly
   - Try different terminal emulator

## Mouse Issues

### Mouse not working

**Symptoms:**
- Can't click windows
- Can't move/resize windows
- Mouse has no effect

**Solutions:**

1. **Check terminal mouse support**
   - Verify terminal supports mouse events
   - Try Windows Terminal, iTerm2, Alacritty

2. **Terminal settings**
   - **Windows Terminal:** No special config needed
   - **iTerm2:** Preferences → Profiles → Terminal → "Report mouse events"
   - **Alacritty:** Mouse enabled by default

3. **Try different terminal**
   - Some terminals have poor mouse support
   - Recommended: Windows Terminal, iTerm2, Alacritty, Kitty

### Can't move window

**Symptoms:**
- Clicking title bar doesn't move window
- Drag operation not working

**Solutions:**

1. **Click middle of title bar**
   - Title bar excludes corners (1 cell margin)
   - Click near the title text

2. **Hold mouse button while dragging**
   - Click and hold
   - Drag to new position
   - Release to place

3. **Check terminal size**
   - Can't move window outside terminal bounds
   - Increase terminal size

### Can't resize window

**Symptoms:**
- Clicking borders doesn't resize
- Resize operation not working

**Solutions:**

1. **Click directly on border**
   - Click the border line exactly
   - Corners are small (1 cell), aim carefully

2. **Try edge borders instead**
   - Edges are easier targets than corners
   - Left/right edges resize width
   - Top/bottom edges resize height

3. **Check minimum size**
   - Windows have minimum size (typically 5x5)
   - Can't resize below minimum

### Text selection not working

**Symptoms:**
- Can't select text
- Selection not highlighting
- Text not copying to clipboard

**Solutions:**

1. **Ensure clicking in text window**
   - Only text windows support selection
   - Progress bars, countdown timers not selectable

2. **Release mouse button to copy**
   - Text copies on release, not while dragging
   - Check clipboard after release

3. **Use Shift for native selection**
   - Hold Shift while clicking/dragging
   - Uses terminal's native selection
   - Useful for selecting across windows

## Configuration Issues

### Config changes not applying

**Symptoms:**
- Edit config file but changes don't take effect
- Settings editor changes not saving

**Solutions:**

1. **Restart VellumFE**
   - Some settings require restart
   - Presets, keybinds require restart
   - Window settings apply immediately

2. **Check config file syntax**
   - TOML syntax errors prevent loading
   - Use TOML validator online
   - Check quotes, brackets, equals signs

3. **Check config file location**
   ```bash
   # Windows
   C:\Users\YourName\.vellum-fe\configs\default.toml

   # Linux/Mac
   ~/.vellum-fe/configs/default.toml
   ```

4. **Check character-specific config**
   - With `--character`, character config overrides default
   - Edit `<character>.toml` instead of `default.toml`

### Layout not loading

**Symptoms:**
- Save layout but can't load it
- Layout loads but wrong windows appear

**Solutions:**

1. **Check layout file location**
   ```bash
   .layouts  # List available layouts
   ```

2. **Check layout file syntax**
   - TOML syntax errors prevent loading
   - Verify `[[ui.windows]]` sections

3. **Auto-save interfering**
   - `auto_<character>.toml` has highest priority
   - Delete auto-save to use other layouts:
   ```bash
   del ~/.vellum-fe/layouts/auto_YourCharacter.toml
   ```

4. **Use absolute paths**
   - Verify row, col, rows, cols values
   - Ensure windows fit in terminal

### Highlights not matching

**Symptoms:**
- Create highlight but text not colored
- Pattern should match but doesn't

**Solutions:**

1. **Test regex pattern**
   - Use regex101.com to test
   - Patterns are case-sensitive by default

2. **Check escape characters**
   - Special chars need escaping: `. * + ? [ ] ( ) { } ^ $ | \`
   - Use `\.` for literal period, `\*` for literal asterisk

3. **Check window receives stream**
   - Highlight only applies to subscribed windows
   - Verify window subscribes to correct stream

4. **Check highlight priority**
   - Last matching highlight wins
   - Reorder highlights in config file

5. **Disable fast_parse for regex**
   - Fast parse only for literal strings
   - Disable if using regex features

## Keybind Issues

### Keybind not working

**Symptoms:**
- Press key combo but nothing happens
- Keybind not triggering action/macro

**Solutions:**

1. **Check key combo syntax**
   - Use `Ctrl+A`, not `Ctrl-A` or `Ctrl A`
   - Case insensitive: `Ctrl+a` or `Ctrl+A` both work

2. **Check for conflicts**
   - Terminal may intercept key
   - Try different key combination
   - Some keys reserved by terminal:
     - Ctrl+C (interrupt)
     - Ctrl+Z (suspend)
     - Ctrl+S/Ctrl+Q (flow control)

3. **Check action name**
   - Action must exactly match built-in name
   - Check spelling and case

4. **Verify keybind in config**
   ```toml
   [[keybinds]]
   key = "F1"
   action_type = "macro"
   action = "stance offensive"
   ```

5. **Try different terminal**
   - Some terminals don't support all key combos
   - Try Windows Terminal, iTerm2, Alacritty

## Clickable Links Issues

### Links not clickable

**Symptoms:**
- Text not clickable
- No context menu appears
- Links don't highlight

**Solutions:**

1. **Launch with --links flag**
   ```bash
   .\vellumfe.exe --port 8001 --character YourCharacter --links
   ```

2. **Check game objects**
   - Only game objects in `<a>` tags are clickable
   - Not all text is clickable

3. **Click directly on linked text**
   - Must click the highlighted word
   - Try different word in multi-word link

### Context menu empty or wrong

**Symptoms:**
- Menu appears but no options
- Menu shows wrong commands

**Solutions:**

1. **Check cmdlist1.xml**
   - File must exist: `defaults/cmdlist1.xml`
   - Contains 588 command entries

2. **Check object data**
   - Game must send exist ID and noun
   - Some objects may have limited actions

3. **Report missing commands**
   - If command missing from menu, needs addition to cmdlist1.xml
   - Community can contribute additions

## Debug Logging

### Enabling Debug Logs

Debug logs are always enabled and written to:

**Default:**
```bash
~/.vellum-fe/debug.log
```

**Character-specific:**
```bash
~/.vellum-fe/debug_YourCharacter.log
```

### Viewing Debug Logs

**Windows:**
```bash
type C:\Users\YourName\.vellum-fe\debug.log
```

**Linux/Mac:**
```bash
cat ~/.vellum-fe/debug.log
tail -f ~/.vellum-fe/debug.log  # Follow log in real-time
```

### What to Look For

**Connection errors:**
```
ERROR Failed to connect to localhost:8000
ERROR Connection refused
```

**Parsing errors:**
```
ERROR Failed to parse XML tag
WARN Unknown preset id: foo
```

**Configuration errors:**
```
ERROR Failed to load config
ERROR TOML parse error
```

**Performance warnings:**
```
WARN Event loop lagging
WARN High CPU usage detected
```

## Getting Help

### Information to Provide

When reporting issues, include:

1. **VellumFE version**
   ```bash
   .\vellumfe.exe --version
   ```

2. **Operating system**
   - Windows 10/11
   - macOS version
   - Linux distribution

3. **Terminal emulator**
   - Windows Terminal, iTerm2, Alacritty, etc.

4. **Connection details**
   - Port number
   - Character name (if using --character)

5. **Error messages**
   - From terminal
   - From debug log

6. **Steps to reproduce**
   - What you did
   - What happened
   - What you expected

### Where to Get Help

1. **Check this wiki**
   - Most common issues covered

2. **Debug logs**
   - Check `~/.vellum-fe/debug.log` first

3. **GitHub Issues**
   - Report bugs: https://github.com/your-repo/vellumfe/issues
   - Search existing issues first

4. **Community Discord**
   - Real-time help from community
   - Share configs and layouts

## Common Error Messages

### "Failed to parse TOML"

**Cause:** Syntax error in config file

**Solution:** Check TOML syntax:
- Quotes around strings
- Proper array syntax: `["item1", "item2"]`
- Proper table syntax: `[[section]]`

### "Unknown preset id"

**Cause:** Game using preset not in config

**Solution:** Add preset to config:
```toml
[[presets]]
id = "missing_preset"
fg = "#ffffff"
bg = "-"
```

### "Window not found"

**Cause:** Command references non-existent window

**Solution:** Check window name:
```bash
.windows  # List all windows
```

### "Stream has no subscribers"

**Cause:** Game pushing to stream with no window

**Solution:** Create window for that stream:
```bash
.customwindow newwin stream_name
```

## Performance Benchmarks

Expected performance on modern hardware:

- **CPU Usage:** 1-5% idle, 5-15% active
- **Memory Usage:** 10-50 MB
- **Frame Rate:** 30-60 FPS (based on poll_timeout_ms)
- **Latency:** <10ms input to display

If performance significantly worse, see Performance Issues section.

## See Also

- [Getting Started](Getting-Started.md) - Basic setup
- [Configuration](Configuration.md) - Config file reference
- [Commands Reference](Commands.md) - All dot commands
- [FAQ](FAQ.md) - Frequently asked questions
