# Keybind Actions

Customize keyboard shortcuts for commands and actions.

## Basic Keybind

```toml
[keybinds]
"f2" = { command = "stance offensive" }
```

## Key Format

```
modifier+key
```

### Modifiers

- `ctrl` - Control key
- `alt` - Alt key
- `shift` - Shift key

Combine: `ctrl+shift+a`

### Key Names

| Keys | Names |
|------|-------|
| Letters | `a` through `z` |
| Numbers | `0` through `9` |
| Function | `f1` through `f12` |
| Arrows | `up`, `down`, `left`, `right` |
| Navigation | `home`, `end`, `pageup`, `pagedown` |
| Numpad | `numpad0`-`numpad9`, `numpad_add`, etc. |
| Other | `enter`, `tab`, `escape`, `space`, `backspace` |

## Action Types

### Game Commands

```toml
"f2" = { command = "stance offensive" }
"numpad5" = { command = "look" }
```

### Macros (Multiple Commands)

```toml
"ctrl+h" = { macro = "hide\npause 2\nstalk" }
```

### Client Actions

```toml
"f1" = "menu"
"ctrl+c" = "copy"
"pageup" = "scroll_up"
```

## Available Actions

| Action | Description |
|--------|-------------|
| `menu` | Open main menu |
| `copy` | Copy selection |
| `scroll_up` | Scroll focused window up |
| `scroll_down` | Scroll focused window down |
| `scroll_top` | Jump to top |
| `scroll_bottom` | Jump to bottom |
| `focus_next` | Focus next window |
| `focus_prev` | Focus previous window |
| `save_layout` | Save current layout |
| `cancel` | Close menu/cancel |
| `search` | Search in window |

## Example Configuration

```toml
[keybinds]
# Navigation (numpad)
"numpad1" = { command = "go sw" }
"numpad2" = { command = "go s" }
"numpad3" = { command = "go se" }
"numpad4" = { command = "go w" }
"numpad5" = { command = "look" }
"numpad6" = { command = "go e" }
"numpad7" = { command = "go nw" }
"numpad8" = { command = "go n" }
"numpad9" = { command = "go ne" }
"numpad_add" = { command = "go out" }
"numpad_subtract" = { command = "go up" }
"numpad_multiply" = { command = "go down" }

# Combat stances
"f2" = { command = "stance offensive" }
"f3" = { command = "stance defensive" }
"f4" = { command = "stance guarded" }

# Quick actions
"f5" = { command = "look in my backpack" }
"f6" = { command = "inventory" }

# Macros
"ctrl+h" = { macro = "hide\npause 2\nstalk" }
"ctrl+l" = { macro = "search\npause 1\nloot" }

# Client
"f1" = "menu"
"ctrl+s" = "save_layout"
"ctrl+c" = "copy"
"escape" = "cancel"
```
