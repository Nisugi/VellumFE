# Keybindings Guide

VellumFE supports fully customizable keybindings. All key mappings are defined in your config file and can be remapped to suit your terminal or preferences.

## Config Location

Keybindings are stored in separate keybinds.toml files:
- **Character-specific**: `~/.vellum-fe/{character}/keybinds.toml`
- **Default**: `~/.vellum-fe/default/keybinds.toml`
- **Built-in defaults**: Embedded in the application (see `defaults/keybinds.toml`)

## Terminal Compatibility

**If backspace doesn't work in your terminal**, this is usually because different terminals send different key codes:

- Some terminals send `Backspace` key code
- Others send `Delete` key code for the backspace key
- Some send `Ctrl+H`

### Quick Fix for Backspace

Edit your keybinds.toml file and change the backspace binding:

```toml
# Try changing this:
backspace = "cursor_backspace"

# To this if backspace doesn't work:
delete = "cursor_backspace"

# Or this:
"ctrl+h" = "cursor_backspace"
```

### Testing Key Codes

To see what key code your terminal sends:
1. Run VellumFE with debug logging: `RUST_LOG=debug cargo run`
2. Press keys in the command input
3. Check the log file: `~/.vellum-fe/debug.log` (or `debug_<character>.log`)
4. Look for lines like `KEY EVENT: Backspace, modifiers=...`

## Default Keybindings

### Basic Input
- `Enter` - Send command
- `Backspace` - Delete character before cursor
- `Left` - Move cursor left
- `Right` - Move cursor right
- `Home` - Move cursor to start of line
- `End` - Move cursor to end of line

### Word Navigation
- `Ctrl+Left` - Move cursor one word left
- `Ctrl+Right` - Move cursor one word right
- `Ctrl+W` - Delete word

### Command History
- `Up` - Previous command
- `Down` - Next command
- `Ctrl+R` - Repeat last command
- `Ctrl+T` - Repeat second-to-last command

### Window Management
- `Tab` - Cycle focused window / Auto-complete commands
- `Page Up` - Scroll current window up
- `Page Down` - Scroll current window down

### Search
- `Ctrl+F` - Start search
- `F3` - Next search match
- `Shift+F3` - Previous search match
- `Esc` - Clear search

### Debug
- `Ctrl+P` - Toggle performance stats

## Available Actions

When creating custom keybindings, you can use these actions:

### Command Input
- `send_command` - Submit current command
- `cursor_left` - Move cursor left one character
- `cursor_right` - Move cursor right one character
- `cursor_word_left` - Move cursor left one word
- `cursor_word_right` - Move cursor right one word
- `cursor_home` - Move cursor to start
- `cursor_end` - Move cursor to end
- `cursor_backspace` - Delete character before cursor
- `cursor_delete` - Delete word after cursor

### Command History
- `previous_command` - Navigate to previous command in history
- `next_command` - Navigate to next command in history
- `send_last_command` - Instantly send last command
- `send_second_last_command` - Instantly send second-to-last command

### Window Management
- `switch_current_window` - Cycle focused window
- `scroll_current_window_up_one` - Scroll up by 1 line
- `scroll_current_window_down_one` - Scroll down by 1 line
- `scroll_current_window_up_page` - Scroll up by 10 lines
- `scroll_current_window_down_page` - Scroll down by 10 lines

### Search
- `start_search` - Enter search mode
- `next_search_match` - Jump to next match
- `prev_search_match` - Jump to previous match
- `clear_search` - Exit search mode

### Other
- `toggle_performance_stats` - Show/hide performance statistics

## Key Format

Keys are specified as strings in lowercase:

### Basic Keys
- Letter keys: `"a"`, `"b"`, `"c"`, etc.
- Special keys: `"enter"`, `"backspace"`, `"delete"`, `"tab"`, `"esc"`
- Arrow keys: `"up"`, `"down"`, `"left"`, `"right"`
- Navigation: `"home"`, `"end"`, `"page_up"`, `"page_down"`
- Function keys: `"f1"`, `"f2"`, ..., `"f12"`

### Modifiers
Combine keys with modifiers using `+`:
- `"ctrl+c"` - Ctrl+C
- `"alt+f"` - Alt+F
- `"shift+f3"` - Shift+F3
- `"ctrl+shift+a"` - Ctrl+Shift+A

### Numpad Keys
- `"num_0"` through `"num_9"`
- `"num_+"`, `"num_-"`, `"num_*"`, `"num_/"`
- `"num_."`

## Example Custom Configuration

Edit keybinds.toml:

```toml
# Use Delete instead of Backspace
delete = "cursor_backspace"

# Map F1 to send "look"
[f1]
macro_text = "look\r"

# Map Ctrl+Q to send "quit"
["ctrl+q"]
macro_text = "quit\r"

# Numpad movement macros (already included by default)
[num_1]
macro_text = "sw\r"

[num_2]
macro_text = "s\r"

[num_3]
macro_text = "se\r"
```

## Macros vs Actions

There are two types of keybindings:

1. **Actions** - Built-in functions like cursor movement, history navigation
   ```toml
   backspace = "cursor_backspace"
   enter = "send_command"
   ```

2. **Macros** - Send literal text to the game (note: include `\r` for enter key)
   ```toml
   [f1]
   macro_text = "look\r"

   ["ctrl+q"]
   macro_text = "quit\r"
   ```

## Troubleshooting

### Keybind Not Working
1. Check the config file syntax is correct
2. Ensure no duplicate keybinds (later ones override earlier ones)
3. Check debug log to see what key code is being sent
4. Some key combinations may be intercepted by your terminal or OS

### MobaXterm on Windows
MobaXterm may send different key codes than expected. Common issues:
- Backspace might send `Delete` instead
- Some Ctrl combinations might not work
- Try Alt combinations as alternatives

### Finding Your Keybinds File
```bash
# Linux/Mac
ls ~/.vellum-fe/default/keybinds.toml
ls ~/.vellum-fe/*/keybinds.toml  # All characters

# Windows (PowerShell)
ls $env:USERPROFILE\.vellum-fe\default\keybinds.toml
ls $env:USERPROFILE\.vellum-fe\*\keybinds.toml  # All characters
```

## Need Help?

1. Check `~/.vellum-fe/debug.log` for key event logging
2. Report issues at https://github.com/your-repo/vellum-fe/issues
3. Share your terminal type and OS when reporting keybind issues
