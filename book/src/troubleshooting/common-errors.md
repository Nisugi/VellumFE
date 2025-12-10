# Common Errors

Error messages, their meanings, and solutions.

## Configuration Errors

### "Failed to parse config"

**Message**:
```
Error: Failed to parse config.toml: expected `=` at line 15
```

**Cause**: TOML syntax error

**Solution**:
1. Check the indicated line number
2. Look for:
   - Missing `=` in key-value pairs
   - Unclosed quotes
   - Missing brackets for sections/arrays

**Example fix**:
```toml
# Wrong
[connection]
mode lich          # Missing =

# Correct
[connection]
mode = "lich"
```

### "Unknown key"

**Message**:
```
Error: Unknown key 'colour' in config.toml at line 12
```

**Cause**: Misspelled or deprecated key name

**Solution**: Check the [Configuration Reference](../configuration/README.md) for correct key names

**Common misspellings**:
- `colour` → `color`
- `colour_theme` → `color` (in theme context)
- `keybind` → `keybinds`

### "Invalid value type"

**Message**:
```
Error: Invalid type for 'port': expected integer, found string
```

**Cause**: Wrong data type for configuration value

**Solution**: Use correct types:
```toml
# Wrong
port = "8000"      # String when integer expected
enabled = 1        # Integer when boolean expected

# Correct
port = 8000        # Integer
enabled = true     # Boolean
```

### "Missing required field"

**Message**:
```
Error: Missing required field 'type' in widget definition
```

**Cause**: Required configuration field not specified

**Solution**: Add the missing field
```toml
# Wrong - missing type
[[widgets]]
name = "main"

# Correct
[[widgets]]
type = "text"
name = "main"
```

## Startup Errors

### "Failed to initialize terminal"

**Message**:
```
Error: Failed to initialize terminal: not a tty
```

**Cause**: Running in non-interactive environment

**Solution**:
- Run in a proper terminal emulator
- Don't pipe output: use `vellum-fe` not `vellum-fe | less`
- Ensure stdin is connected to a terminal

### "Could not determine terminal size"

**Message**:
```
Error: Could not determine terminal size
```

**Cause**: Terminal doesn't report dimensions

**Solution**:
1. Try a different terminal emulator
2. Set size manually:
   ```bash
   stty rows 50 cols 120
   vellum-fe
   ```
3. Check `TERM` environment variable:
   ```bash
   echo $TERM
   # Should be something like xterm-256color
   ```

### "Failed to load font"

**Message**:
```
Error: Failed to load font: font not found
```

**Cause**: Terminal can't render required characters

**Solution**:
- Install a font with Unicode support (Nerd Font, JetBrains Mono)
- Or disable Unicode in config:
  ```toml
  [display]
  unicode = false
  ```

## Connection Errors

### "Connection refused"

**Message**:
```
Error: Connection refused (os error 111)
```

**Cause**:
- Lich not running (Lich mode)
- Wrong host/port
- Firewall blocking

**Solution**:

For Lich mode:
```bash
# Start Lich first
ruby lich.rb

# Then connect
vellum-fe --host 127.0.0.1 --port 8000
```

For Direct mode:
```bash
# Check internet connectivity
ping eaccess.play.net

# Check port access
nc -zv eaccess.play.net 7910
```

### "Connection timed out"

**Message**:
```
Error: Connection timed out after 30 seconds
```

**Cause**: Network issue preventing connection

**Solution**:
1. Check internet connectivity
2. Verify firewall settings
3. Try increasing timeout:
   ```toml
   [connection]
   timeout = 60
   ```

### "Authentication failed"

**Message**:
```
Error: Authentication failed: invalid credentials
```

**Cause**: Wrong account or password (Direct mode)

**Solution**:
- Verify credentials work via Lich or web
- Check for typos
- Ensure account is active
- Try password without special characters

### "Certificate verification failed"

**Message**:
```
Error: Certificate verification failed: certificate has expired
```

**Cause**: Cached certificate is outdated or corrupt

**Solution**:
```bash
# Remove cached certificate
rm ~/.vellum-fe/simu.pem

# Reconnect - will download fresh certificate
vellum-fe --direct ...
```

## Runtime Errors

### "Widget not found"

**Message**:
```
Error: Widget 'health' not found in layout
```

**Cause**: Configuration references non-existent widget

**Solution**: Ensure widget is defined in layout:
```toml
# Make sure this exists
[[widgets]]
type = "progress"
name = "health"
```

### "Invalid regex pattern"

**Message**:
```
Error: Invalid regex in highlights.toml: unclosed group at position 5
```

**Cause**: Malformed regular expression

**Solution**:
1. Check regex syntax
2. Escape special characters properly
3. Test pattern with regex tester

**Common regex fixes**:
```toml
# Wrong - unescaped special chars
pattern = "You (get|take"    # Unclosed group

# Correct
pattern = "You (get|take)"   # Closed group

# Wrong - unescaped brackets
pattern = "[Player]"         # Character class

# Correct
pattern = "\\[Player\\]"     # Literal brackets
```

### "Stream buffer overflow"

**Message**:
```
Warning: Stream buffer overflow, dropping oldest entries
```

**Cause**: Too much data incoming, buffer full

**Solution**:
```toml
# Increase buffer size
[performance]
stream_buffer_size = 100000

# Or reduce scrollback
[[widgets]]
type = "text"
scrollback = 1000  # Reduce from default
```

### "Render timeout"

**Message**:
```
Warning: Render timeout - frame dropped
```

**Cause**: UI can't keep up with updates

**Solution**:
```toml
[performance]
render_rate = 30      # Reduce from 60
batch_updates = true  # Combine updates
lazy_render = true    # Skip unchanged
```

## Widget Errors

### "Widget overlap"

**Message**:
```
Warning: Widget 'status' overlaps with 'health'
```

**Cause**: Two widgets occupy same screen space

**Solution**: Adjust widget positions:
```toml
# Check x, y, width, height don't overlap
[[widgets]]
type = "progress"
name = "health"
x = 0
y = 0
width = 20
height = 3

[[widgets]]
type = "indicator"
name = "status"
x = 0
y = 3        # Start after health ends
width = 20
height = 5
```

### "Widget outside bounds"

**Message**:
```
Error: Widget 'compass' extends beyond terminal (x=90, width=20, terminal=100)
```

**Cause**: Widget doesn't fit in terminal

**Solution**:
- Use percentage-based positioning
- Make widget smaller
- Check terminal size

### "Invalid data source"

**Message**:
```
Error: Invalid data_source 'vital.health' for widget 'hp_bar'
```

**Cause**: Data source path doesn't exist

**Solution**: Use valid data sources:
```toml
# Wrong
data_source = "vital.health"

# Correct
data_source = "vitals.health"
```

Valid sources: `vitals.health`, `vitals.mana`, `vitals.stamina`, `vitals.spirit`, `roundtime`, `casttime`

## Keybind Errors

### "Invalid key name"

**Message**:
```
Error: Invalid key name 'Control+F' in keybinds.toml
```

**Cause**: Wrong key naming convention

**Solution**: Use correct format:
```toml
# Wrong
[keybinds."Control+F"]

# Correct
[keybinds."ctrl+f"]
```

**Key naming rules**:
- Modifiers: `ctrl`, `alt`, `shift` (lowercase)
- Separator: `+`
- Keys: lowercase (`f1`, `enter`, `space`)

### "Duplicate keybind"

**Message**:
```
Warning: Duplicate keybind 'ctrl+f' - later definition wins
```

**Cause**: Same key defined multiple times

**Solution**: Remove duplicates or use different keys

### "Unknown action"

**Message**:
```
Error: Unknown action 'focusInput' in keybind
```

**Cause**: Invalid action name

**Solution**: Check [Keybind Actions](../reference/keybind-actions.md) for valid actions
```toml
# Wrong
action = "focusInput"

# Correct
action = "focus_input"
```

## Recovery Steps

### General Recovery

1. **Start with defaults**:
   ```bash
   vellum-fe --default-config
   ```

2. **Isolate the problem**:
   ```bash
   # Test each config file
   vellum-fe --config ~/.vellum-fe/config.toml --no-layout
   vellum-fe --config ~/.vellum-fe/config.toml --layout ~/.vellum-fe/layout.toml
   ```

3. **Binary search configs**:
   - Comment out half the config
   - If it works, problem is in commented half
   - Repeat until found

### Reset Everything

```bash
# Backup
mv ~/.vellum-fe ~/.vellum-fe.backup

# Fresh start
vellum-fe

# Copy back configs one at a time
cp ~/.vellum-fe.backup/config.toml ~/.vellum-fe/
# Test
cp ~/.vellum-fe.backup/layout.toml ~/.vellum-fe/
# Test
# Continue until problem returns
```

## See Also

- [Configuration Reference](../configuration/README.md)
- [Platform Issues](./platform-issues.md)
- [Connection Issues](./connection-issues.md)

