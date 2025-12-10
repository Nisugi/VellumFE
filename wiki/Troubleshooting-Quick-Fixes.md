# Troubleshooting Quick Fixes

## Connection Issues

### Can't connect to Lich

**Symptom:** "Connection refused" error

**Fix:**
1. Ensure Lich is running and logged in
2. Check Lich is listening on correct port (default: 8000)
3. Verify host/port in VellumFE:
   ```bash
   vellum-fe --host 127.0.0.1 --port 8000
   ```

### Direct eAccess fails

**Symptom:** Authentication error with `--direct`

**Fix:**
1. Verify credentials (test with Lich first)
2. Delete cached certificate and retry:
   ```bash
   rm ~/.vellum-fe/simu.pem
   ```
3. Check account is active and not locked

### Connection drops frequently

**Symptom:** Disconnects every few minutes

**Fix:**
1. Enable auto-reconnect in config:
   ```toml
   [connection]
   auto_reconnect = true
   reconnect_delay = 5
   ```
2. Check network stability
3. Increase timeout values

## Display Issues

### Colors look wrong

**Symptom:** Colors don't match expected theme

**Fix:**
1. Check terminal supports 256 colors
2. Set `TERM=xterm-256color`
3. Verify colors.toml syntax

### Text is garbled/corrupted

**Symptom:** Strange characters, misaligned text

**Fix:**
1. Ensure terminal uses UTF-8 encoding
2. Try a different terminal emulator
3. Check font supports required characters

### Windows overlap incorrectly

**Symptom:** Widgets don't appear where expected

**Fix:**
1. Check terminal size matches layout expectations
2. Verify widget coordinates don't overlap
3. Press `Ctrl+R` to refresh display

### Borders look broken

**Symptom:** Border characters show as boxes/question marks

**Fix:**
1. Use a font with Unicode box-drawing support
2. Try `border_style = "ascii"` in layout.toml

## Performance Issues

### Slow/laggy response

**Symptom:** Commands take long to appear

**Fix:**
1. Reduce buffer_size in text windows
2. Disable unused widgets
3. Check for complex highlight patterns

### High CPU usage

**Symptom:** VellumFE uses excessive CPU

**Fix:**
1. Reduce highlight pattern count
2. Use simpler regex patterns
3. Increase refresh interval

### Memory usage grows over time

**Symptom:** Memory leak

**Fix:**
1. Reduce `buffer_size` on text windows
2. Lower scrollback limits
3. Restart periodically for long sessions

## Configuration Issues

### Config file not loading

**Symptom:** Settings don't apply

**Fix:**
1. Check file location:
   - Linux: `~/.config/vellum-fe/`
   - macOS: `~/Library/Application Support/vellum-fe/`
   - Windows: `%APPDATA%\vellum-fe\`
2. Validate TOML syntax
3. Check file permissions

### Keybinds not working

**Symptom:** Custom keys do nothing

**Fix:**
1. Verify keybinds.toml syntax
2. Check key isn't captured by terminal
3. Ensure no conflicting bindings

### Highlights not appearing

**Symptom:** Patterns don't match

**Fix:**
1. Test regex at regex101.com
2. Escape special characters: `\.` `\(` `\[`
3. Check case sensitivity (use `(?i)` for case-insensitive)

## Common Error Messages

### "Failed to parse config"

```
Error: Failed to parse config.toml at line 15
```

**Fix:** Check line 15 for syntax errors (missing quotes, brackets)

### "Widget name already exists"

```
Error: Duplicate widget name 'main'
```

**Fix:** Ensure all widget names are unique in layout.toml

### "Unknown stream"

```
Warning: Unknown stream 'invalid'
```

**Fix:** Use valid stream name (main, room, speech, etc.)

### "Permission denied"

```
Error: Permission denied: ~/.vellum-fe/config.toml
```

**Fix:** Check file permissions: `chmod 644 ~/.vellum-fe/config.toml`

## Quick Diagnostic Commands

```bash
# Check VellumFE version
vellum-fe --version

# Run with debug logging
vellum-fe --debug

# Test configuration
vellum-fe --check-config

# Reset to defaults
rm -rf ~/.config/vellum-fe/
vellum-fe  # Creates fresh config
```

## Still Stuck?

1. Check [existing issues](https://github.com/nisugi/vellum-fe/issues)
2. Enable debug logging and check `vellum-fe.log`
3. [Open a new issue](https://github.com/nisugi/vellum-fe/issues/new) with:
   - VellumFE version
   - OS and terminal
   - Steps to reproduce
   - Error messages/logs
