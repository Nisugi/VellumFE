# Keybind Actions

VellumFE supports extensive keyboard customization. This guide covers all available keybind actions.

## Keybind Basics

Keybinds are defined in `keybinds.toml`:

```toml
[keybinds."ctrl+l"]
action = "layout_editor"

[keybinds."f1"]
action = "help"

[keybinds."ctrl+1"]
macro = "attack target"
```

## Key Format

### Modifiers

| Modifier | Format |
|----------|--------|
| Control | `ctrl+` |
| Alt | `alt+` |
| Shift | `shift+` |
| Meta/Super | `meta+` |

### Special Keys

| Key | Format |
|-----|--------|
| F1-F12 | `f1` to `f12` |
| Enter | `enter` |
| Escape | `escape` |
| Tab | `tab` |
| Backspace | `backspace` |
| Delete | `delete` |
| Insert | `insert` |
| Home | `home` |
| End | `end` |
| Page Up | `pageup` |
| Page Down | `pagedown` |
| Arrows | `up`, `down`, `left`, `right` |
| Space | `space` |

### Combination Examples

```toml
"ctrl+s"         # Ctrl + S
"ctrl+shift+s"   # Ctrl + Shift + S
"alt+f4"         # Alt + F4
"f1"             # F1 alone
"shift+tab"      # Shift + Tab
```

## Action Categories

### Navigation Actions

| Action | Description |
|--------|-------------|
| `scroll_up` | Scroll window up |
| `scroll_down` | Scroll window down |
| `page_up` | Page up |
| `page_down` | Page down |
| `scroll_top` | Jump to top |
| `scroll_bottom` | Jump to bottom |
| `focus_next` | Focus next window |
| `focus_prev` | Focus previous window |
| `focus_main` | Focus main window |
| `focus_input` | Focus command input |

### Window Actions

| Action | Description |
|--------|-------------|
| `layout_editor` | Open layout editor |
| `window_editor` | Open window editor |
| `toggle_border` | Toggle window border |
| `toggle_title` | Toggle window title |
| `maximize_window` | Maximize current window |
| `restore_window` | Restore window size |
| `close_window` | Close current popup |

### Editor Actions

| Action | Description |
|--------|-------------|
| `highlight_browser` | Open highlight browser |
| `keybind_browser` | Open keybind browser |
| `color_browser` | Open color browser |
| `theme_browser` | Open theme browser |
| `settings_editor` | Open settings |

### Input Actions

| Action | Description |
|--------|-------------|
| `history_prev` | Previous command history |
| `history_next` | Next command history |
| `clear_input` | Clear command input |
| `submit_command` | Submit current command |
| `cancel` | Cancel current operation |

### Text Actions

| Action | Description |
|--------|-------------|
| `select_all` | Select all text |
| `copy` | Copy selection |
| `cut` | Cut selection |
| `paste` | Paste clipboard |
| `search` | Open search |
| `search_next` | Find next |
| `search_prev` | Find previous |

### Tab Actions

| Action | Description |
|--------|-------------|
| `next_tab` | Next tab |
| `prev_tab` | Previous tab |
| `tab_1` to `tab_9` | Jump to tab by number |
| `close_tab` | Close current tab |

### Application Actions

| Action | Description |
|--------|-------------|
| `quit` | Exit application |
| `help` | Show help |
| `reload_config` | Reload all config |
| `reload_colors` | Reload colors |
| `reload_highlights` | Reload highlights |
| `reload_keybinds` | Reload keybinds |
| `reload_layout` | Reload layout |

### Game Actions

| Action | Description |
|--------|-------------|
| `reconnect` | Reconnect to server |
| `disconnect` | Disconnect |

## Macros

Send game commands:

```toml
[keybinds."ctrl+1"]
macro = "attack target"

[keybinds."ctrl+2"]
macro = "stance defensive"

[keybinds."ctrl+h"]
macro = "hide"
```

### Multi-Command Macros

Separate with semicolons:

```toml
[keybinds."f5"]
macro = "stance offensive;attack target"
```

### Macro with Variables

Use `$input` for prompted input:

```toml
[keybinds."ctrl+g"]
macro = "go $input"
# Prompts for input, then sends "go <input>"
```

## Default Keybinds

### Global

| Key | Action |
|-----|--------|
| `Escape` | Cancel/close popup |
| `Ctrl+C` | Copy |
| `Ctrl+V` | Paste |
| `Ctrl+X` | Cut |
| `Ctrl+A` | Select all |

### Navigation

| Key | Action |
|-----|--------|
| `Page Up` | Scroll up |
| `Page Down` | Scroll down |
| `Home` | Scroll to top |
| `End` | Scroll to bottom |
| `Tab` | Focus next |
| `Shift+Tab` | Focus prev |

### Command Input

| Key | Action |
|-----|--------|
| `Enter` | Submit command |
| `Up` | History prev |
| `Down` | History next |
| `Ctrl+L` | Clear input |

### Browsers

| Key | Action |
|-----|--------|
| `Up/k` | Navigate up |
| `Down/j` | Navigate down |
| `Enter` | Select |
| `Delete` | Delete item |
| `Escape` | Close |

## Context-Specific Keybinds

Some keybinds only work in specific contexts:

```toml
# Only in Normal mode
[keybinds."ctrl+l"]
action = "layout_editor"
mode = "normal"

# Only in Navigation mode
[keybinds."j"]
action = "scroll_down"
mode = "navigation"
```

## Priority Layers

Keybinds are processed in order:

1. **Global keybinds** - Always active
2. **Menu keybinds** - In popup/editor
3. **User keybinds** - Game mode
4. **Default input** - Character insertion

## Examples

### Combat Setup

```toml
# Quick attacks
[keybinds."f1"]
macro = "attack target"

[keybinds."f2"]
macro = "stance offensive;attack target"

[keybinds."f3"]
macro = "stance defensive"

[keybinds."f4"]
macro = "hide"

# Movement
[keybinds."numpad8"]
macro = "go north"

[keybinds."numpad2"]
macro = "go south"

[keybinds."numpad4"]
macro = "go west"

[keybinds."numpad6"]
macro = "go east"
```

### Spellcasting

```toml
[keybinds."ctrl+1"]
macro = "prep 101;cast"

[keybinds."ctrl+2"]
macro = "prep 103;cast"

[keybinds."ctrl+3"]
macro = "prep 107;cast target"
```

### Navigation

```toml
[keybinds."ctrl+shift+up"]
action = "scroll_top"

[keybinds."ctrl+shift+down"]
action = "scroll_bottom"

[keybinds."alt+1"]
action = "focus_main"

[keybinds."alt+2"]
macro = ".focus combat"
```

### Quick Commands

```toml
[keybinds."ctrl+l"]
macro = "look"

[keybinds."ctrl+i"]
macro = "inventory"

[keybinds."ctrl+e"]
macro = "experience"

[keybinds."ctrl+w"]
macro = "wealth"
```

## Using the Keybind Browser

```
.keybinds
```

Features:
- Browse all keybinds
- Add new keybinds
- Edit existing keybinds
- Delete keybinds
- Filter by type

## Conflict Resolution

If two keybinds use the same key:

1. Higher priority wins
2. Later definition overwrites earlier
3. Context-specific beats general

Check for conflicts:

```
.keybinds
```

Look for duplicate key combos.

## Troubleshooting

### Keybind Not Working

1. Check for conflicts
2. Verify key format is correct
3. Check context/mode setting
4. Reload: `.reload keybinds`

### Key Not Detected

Some keys may not be captured:
- System shortcuts (Alt+Tab, etc.)
- Terminal shortcuts
- Media keys

### Modifier Issues

Some terminals don't report all modifiers:
- Try different modifier combos
- Use function keys (F1-F12) as alternatives

## See Also

- [Keybinds Configuration](../configuration/keybinds-toml.md) - Full reference
- [Browser Editors](../architecture/browser-editors.md) - Keybind browser
- [Command Input](../widgets/command-input.md) - Input widget

