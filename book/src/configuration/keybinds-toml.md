# keybinds.toml Reference

The keybinds configuration file maps keyboard shortcuts to actions.

## Location

`~/.vellum-fe/keybinds.toml`

---

## Structure Overview

```toml
[[keybinds]]
key = "Enter"
action = "send"

[[keybinds]]
key = "Ctrl+Q"
action = "quit"

[[keybinds]]
key = "F1"
action = "send"
argument = "look"
```

---

## Keybind Properties

Each `[[keybinds]]` entry defines one key mapping:

```toml
[[keybinds]]
# Required
key = "Ctrl+S"            # Key combination
action = "send"           # Action to perform

# Optional
argument = "save"         # Action argument
description = "Save game" # Description for help
context = "input"         # When this binding is active
```

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `key` | string | yes | Key combination |
| `action` | string | yes | Action name |
| `argument` | string | no | Action argument |
| `description` | string | no | Help text |
| `context` | string | no | Active context |

---

## Key Syntax

### Basic Keys

```toml
key = "A"         # Letter A
key = "1"         # Number 1
key = "F1"        # Function key
key = "Enter"     # Enter key
key = "Space"     # Space bar
key = "Tab"       # Tab key
key = "Escape"    # Escape key
```

### Modifier Keys

Combine modifiers with `+`:

```toml
key = "Ctrl+S"          # Ctrl + S
key = "Alt+F4"          # Alt + F4
key = "Shift+Tab"       # Shift + Tab
key = "Ctrl+Shift+Z"    # Ctrl + Shift + Z
key = "Ctrl+Alt+Delete" # Multiple modifiers
```

**Available modifiers:**
- `Ctrl` - Control key
- `Alt` - Alt key
- `Shift` - Shift key

### Special Keys

| Key Name | Description |
|----------|-------------|
| `Enter` | Enter/Return |
| `Space` | Space bar |
| `Tab` | Tab key |
| `Escape` | Escape key |
| `Backspace` | Backspace |
| `Delete` | Delete key |
| `Insert` | Insert key |
| `Home` | Home key |
| `End` | End key |
| `PageUp` | Page Up |
| `PageDown` | Page Down |
| `Up` | Up arrow |
| `Down` | Down arrow |
| `Left` | Left arrow |
| `Right` | Right arrow |
| `F1` - `F12` | Function keys |

### Numpad Keys

```toml
key = "Numpad0"   # Numpad 0
key = "Numpad+"   # Numpad plus
key = "Numpad*"   # Numpad multiply
key = "NumpadEnter"  # Numpad enter
```

---

## Actions

### Input Actions

| Action | Argument | Description |
|--------|----------|-------------|
| `send` | command | Send command to game |
| `send_silent` | command | Send without echo |
| `insert` | text | Insert text at cursor |
| `clear_input` | - | Clear input field |
| `history_prev` | - | Previous command |
| `history_next` | - | Next command |

**Examples:**
```toml
[[keybinds]]
key = "Enter"
action = "send"

[[keybinds]]
key = "F1"
action = "send"
argument = "attack"

[[keybinds]]
key = "Ctrl+K"
action = "clear_input"
```

### Navigation Actions

| Action | Argument | Description |
|--------|----------|-------------|
| `scroll_up` | lines | Scroll up |
| `scroll_down` | lines | Scroll down |
| `scroll_page_up` | - | Page up |
| `scroll_page_down` | - | Page down |
| `scroll_top` | - | Jump to top |
| `scroll_bottom` | - | Jump to bottom |
| `focus_next` | - | Focus next window |
| `focus_prev` | - | Focus previous window |
| `focus_window` | name | Focus specific window |

**Examples:**
```toml
[[keybinds]]
key = "PageUp"
action = "scroll_page_up"

[[keybinds]]
key = "Ctrl+Tab"
action = "focus_next"

[[keybinds]]
key = "Ctrl+1"
action = "focus_window"
argument = "main"
```

### Menu Actions

| Action | Argument | Description |
|--------|----------|-------------|
| `open_menu` | - | Open main menu |
| `open_highlight_browser` | - | Open highlight editor |
| `open_keybind_browser` | - | Open keybind editor |
| `open_color_browser` | - | Open color editor |
| `open_window_editor` | - | Open window editor |
| `close_popup` | - | Close current popup |

**Examples:**
```toml
[[keybinds]]
key = "Ctrl+M"
action = "open_menu"

[[keybinds]]
key = "Ctrl+H"
action = "open_highlight_browser"

[[keybinds]]
key = "Escape"
action = "close_popup"
```

### Client Actions

| Action | Argument | Description |
|--------|----------|-------------|
| `quit` | - | Exit VellumFE |
| `reload_config` | - | Reload configuration |
| `clear_window` | name | Clear window content |
| `toggle_links` | - | Toggle clickable links |
| `toggle_border` | name | Toggle window border |
| `copy` | - | Copy selection |
| `paste` | - | Paste clipboard |

**Examples:**
```toml
[[keybinds]]
key = "Ctrl+Q"
action = "quit"

[[keybinds]]
key = "F5"
action = "reload_config"

[[keybinds]]
key = "Ctrl+L"
action = "clear_window"
argument = "main"
```

### Game Actions

| Action | Argument | Description |
|--------|----------|-------------|
| `move` | direction | Move in direction |
| `look` | - | Look around |
| `inventory` | - | Check inventory |

**Examples:**
```toml
[[keybinds]]
key = "Numpad8"
action = "move"
argument = "north"

[[keybinds]]
key = "Numpad2"
action = "move"
argument = "south"
```

---

## Contexts

Keybinds can be limited to specific contexts:

| Context | Active When |
|---------|-------------|
| `global` | Always (default) |
| `input` | Input field focused |
| `menu` | Menu/popup open |
| `browser` | Browser popup open |
| `editor` | Editor popup open |

**Examples:**
```toml
# Only active when input is focused
[[keybinds]]
key = "Tab"
action = "autocomplete"
context = "input"

# Only active in menus
[[keybinds]]
key = "Enter"
action = "select"
context = "menu"
```

---

## Default Keybinds

VellumFE ships with these defaults:

### Navigation
```toml
[[keybinds]]
key = "PageUp"
action = "scroll_page_up"

[[keybinds]]
key = "PageDown"
action = "scroll_page_down"

[[keybinds]]
key = "Home"
action = "scroll_top"

[[keybinds]]
key = "End"
action = "scroll_bottom"

[[keybinds]]
key = "Ctrl+Tab"
action = "focus_next"
```

### Input
```toml
[[keybinds]]
key = "Enter"
action = "send"

[[keybinds]]
key = "Up"
action = "history_prev"

[[keybinds]]
key = "Down"
action = "history_next"

[[keybinds]]
key = "Ctrl+C"
action = "copy"

[[keybinds]]
key = "Ctrl+V"
action = "paste"
```

### Menus
```toml
[[keybinds]]
key = "Ctrl+M"
action = "open_menu"

[[keybinds]]
key = "Ctrl+H"
action = "open_highlight_browser"

[[keybinds]]
key = "Ctrl+K"
action = "open_keybind_browser"

[[keybinds]]
key = "Ctrl+E"
action = "open_window_editor"

[[keybinds]]
key = "Escape"
action = "close_popup"
```

### Client
```toml
[[keybinds]]
key = "Ctrl+Q"
action = "quit"

[[keybinds]]
key = "F5"
action = "reload_config"

[[keybinds]]
key = "Ctrl+L"
action = "clear_window"
argument = "main"
```

---

## Example Configurations

### Numpad Movement

```toml
[[keybinds]]
key = "Numpad8"
action = "send"
argument = "north"
description = "Move north"

[[keybinds]]
key = "Numpad2"
action = "send"
argument = "south"

[[keybinds]]
key = "Numpad4"
action = "send"
argument = "west"

[[keybinds]]
key = "Numpad6"
action = "send"
argument = "east"

[[keybinds]]
key = "Numpad7"
action = "send"
argument = "northwest"

[[keybinds]]
key = "Numpad9"
action = "send"
argument = "northeast"

[[keybinds]]
key = "Numpad1"
action = "send"
argument = "southwest"

[[keybinds]]
key = "Numpad3"
action = "send"
argument = "southeast"

[[keybinds]]
key = "Numpad5"
action = "send"
argument = "out"
```

### Combat Macros

```toml
[[keybinds]]
key = "F1"
action = "send"
argument = "attack"
description = "Basic attack"

[[keybinds]]
key = "F2"
action = "send"
argument = "incant 101"
description = "Cast Spirit Warding I"

[[keybinds]]
key = "F3"
action = "send"
argument = "stance defensive"
description = "Defensive stance"
```

---

## See Also

- [Keybind Actions Reference](../reference/keybind-actions.md) - Complete action list
- [Quick Tour](../getting-started/quick-tour.md) - Default keybinds
- [Customization](../customization/keybind-actions.md) - Keybind customization guide
