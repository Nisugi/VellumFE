# Terminal Compatibility Quick Fix

## Backspace Not Working?

If your backspace key doesn't work in VellumFE, this is a common terminal compatibility issue. Different terminals send different key codes.

### Quick Fix (30 seconds)

1. **Find your keybinds file:**
   - Windows: `C:\Users\YourName\.vellum-fe\default\keybinds.toml`
   - Linux/Mac: `~/.vellum-fe/default/keybinds.toml`
   - Or with character: `~/.vellum-fe/{character}/keybinds.toml`

2. **Edit keybinds.toml** and find the backspace line (around line 31):
   ```toml
   backspace = "cursor_backspace"
   ```

3. **Try changing it to:**
   ```toml
   delete = "cursor_backspace"
   ```

   Note: You may need to comment out the original line:
   ```toml
   # backspace = "cursor_backspace"  # Doesn't work in my terminal
   delete = "cursor_backspace"
   ```

4. **Save and restart VellumFE** - backspace should now work!

### Why Does This Happen?

Different terminals send different escape sequences:
- **Standard terminals**: Send `Backspace` key code
- **MobaXterm, PuTTY, some Windows terminals**: Send `Delete` key code
- **Some terminals**: Send `Ctrl+H`

VellumFE's keybinding system lets you map whichever key your terminal sends to the backspace action.

### Still Not Working?

If changing to `delete` didn't fix it, try `ctrl+h` in keybinds.toml:

```toml
# backspace = "cursor_backspace"  # Doesn't work
# delete = "cursor_backspace"  # Also doesn't work
"ctrl+h" = "cursor_backspace"  # Try this
```

### Finding Out What Your Terminal Sends

1. Run VellumFE with debug logging:
   ```bash
   RUST_LOG=debug ./vellum-fe
   ```

2. Press your backspace key a few times

3. Check the log file:
   - Windows: `C:\Users\YourName\.vellum-fe\debug.log`
   - Linux/Mac: `~/.vellum-fe/debug.log`

4. Look for lines like:
   ```
   KEY EVENT: Backspace, modifiers=...
   KEY EVENT: Delete, modifiers=...
   KEY EVENT: Char('h'), modifiers=CONTROL
   ```

5. Use that key name in your config

### Other Common Terminal Issues

**Problem**: Some Ctrl+ combinations don't work
- **Solution**: Try Alt+ instead, or remap to different keys

**Problem**: Numpad keys don't work
- **Solution**: Enable "Application Keypad Mode" in your terminal settings

**Problem**: Arrow keys insert weird characters
- **Solution**: Your terminal may not support proper ANSI escape sequences. Try a different terminal emulator.

### Recommended Terminal Emulators

These terminals have excellent compatibility:
- **Windows**: Windows Terminal, Alacritty, WezTerm
- **Mac**: iTerm2, Alacritty, WezTerm
- **Linux**: Alacritty, WezTerm, Kitty, GNOME Terminal

### More Help

For comprehensive keybinding documentation, see [KEYBINDINGS.md](KEYBINDINGS.md)

For questions or issues, visit the project's issue tracker.
