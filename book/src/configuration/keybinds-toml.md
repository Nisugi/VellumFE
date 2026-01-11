# keybinds.toml

Keyboard shortcuts for client actions and game commands.

## Basic Format

```toml
[keybinds]
"ctrl+s" = "save_layout"
"f1" = "menu"
"ctrl+c" = "copy"
"numpad1" = { command = "go southwest" }
```

## Key Names

### Modifiers
Combine with `+`: `ctrl+shift+a`, `alt+f1`

| Modifier | Name |
|----------|------|
| Control | `ctrl` |
| Alt | `alt` |
| Shift | `shift` |

### Special Keys

| Key | Name |
|-----|------|
| Function keys | `f1` through `f12` |
| Arrow keys | `up`, `down`, `left`, `right` |
| Navigation | `home`, `end`, `pageup`, `pagedown` |
| Editing | `insert`, `delete`, `backspace` |
| Other | `enter`, `tab`, `escape`, `space` |
| Numpad | `numpad0`-`numpad9`, `numpad_add`, `numpad_subtract`, etc. |

## Action Types

### Client Actions

```toml
"f1" = "menu"                   # Open main menu
"ctrl+c" = "copy"               # Copy selection
"pageup" = "scroll_up"          # Scroll focused window
"pagedown" = "scroll_down"
"ctrl+home" = "scroll_top"
"ctrl+end" = "scroll_bottom"
"ctrl+s" = "save_layout"
"ctrl+tab" = "focus_next"       # Cycle window focus
"escape" = "cancel"             # Close menu/cancel
```

### Game Commands

```toml
"numpad1" = { command = "go southwest" }
"numpad5" = { command = "look" }
"f2" = { command = "stance defensive" }
```

### Macros (Multiple Commands)

```toml
"ctrl+h" = { macro = "hide\npause 2\nstalk" }
```

## Common Keybinds

```toml
[keybinds]
# Navigation
"numpad1" = { command = "go southwest" }
"numpad2" = { command = "go south" }
"numpad3" = { command = "go southeast" }
"numpad4" = { command = "go west" }
"numpad5" = { command = "look" }
"numpad6" = { command = "go east" }
"numpad7" = { command = "go northwest" }
"numpad8" = { command = "go north" }
"numpad9" = { command = "go northeast" }
"numpad_add" = { command = "go out" }
"numpad_subtract" = { command = "go up" }
"numpad_multiply" = { command = "go down" }

# Client
"f1" = "menu"
"ctrl+c" = "copy"
"ctrl+s" = "save_layout"
"pageup" = "scroll_up"
"pagedown" = "scroll_down"
"escape" = "cancel"

# Search
"ctrl+f" = "search"
"ctrl+pageup" = "prev_search_match"
"ctrl+pagedown" = "next_search_match"
```

## Available Actions

| Action | Description |
|--------|-------------|
| `menu` | Open main menu |
| `copy` | Copy selected text |
| `scroll_up` / `scroll_down` | Scroll focused window |
| `scroll_top` / `scroll_bottom` | Jump to top/bottom |
| `focus_next` / `focus_prev` | Cycle window focus |
| `save_layout` | Save current layout |
| `cancel` | Close menu or cancel operation |
| `search` | Open search in focused window |
| `prev_search_match` / `next_search_match` | Cycle search results |
