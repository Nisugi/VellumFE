# Keybind Cheatsheet

## Navigation

| Key | Action |
|-----|--------|
| `Tab` | Focus next window |
| `Shift+Tab` | Focus previous window |
| `Ctrl+1-9` | Focus window by number |
| `Alt+↑/↓/←/→` | Focus window by direction |

## Scrolling

| Key | Action |
|-----|--------|
| `PgUp` | Scroll up one page |
| `PgDn` | Scroll down one page |
| `Shift+PgUp` | Scroll up 5 lines |
| `Shift+PgDn` | Scroll down 5 lines |
| `Home` | Scroll to top |
| `End` | Scroll to bottom (live) |
| `Ctrl+Home` | Scroll to very top |

## Command Input

| Key | Action |
|-----|--------|
| `Enter` | Send command |
| `↑` | Previous command |
| `↓` | Next command |
| `Ctrl+U` | Clear input line |
| `Ctrl+W` | Delete word backward |
| `Ctrl+A` | Move to start |
| `Ctrl+E` | Move to end |
| `Ctrl+←` | Move word left |
| `Ctrl+→` | Move word right |

## Text Selection

| Key | Action |
|-----|--------|
| `Shift+↑/↓/←/→` | Extend selection |
| `Ctrl+Shift+←/→` | Select word |
| `Ctrl+A` (in window) | Select all |
| `Ctrl+C` | Copy selection |
| `Ctrl+V` | Paste |
| `Ctrl+X` | Cut selection |

## Application

| Key | Action |
|-----|--------|
| `F1` | Open main menu |
| `Ctrl+L` | Clear current window |
| `Ctrl+R` | Refresh display |
| `Ctrl+Q` | Quit application |
| `Ctrl+Z` | Suspend (Unix) |
| `Escape` | Close popup/cancel |

## Quick Actions

| Key | Action |
|-----|--------|
| `F2` | Quick save layout |
| `F3` | Toggle window borders |
| `F4` | Toggle timestamps |
| `F5` | Reconnect |
| `F6` | Toggle logging |
| `F7` | Toggle sound |
| `F8` | Toggle TTS |
| `F9` | Toggle compact mode |
| `F10` | Open settings |
| `F11` | Toggle fullscreen |
| `F12` | Screenshot |

## Window Editing

| Key | Action |
|-----|--------|
| `Ctrl+N` | New window |
| `Ctrl+D` | Delete window |
| `Ctrl+R` | Rename window |
| `Ctrl+↑/↓/←/→` | Resize window |
| `Alt+Shift+↑/↓/←/→` | Move window |

## Custom Keybinds

Add to `keybinds.toml`:

```toml
[[keybinds]]
key = "F5"
action = "send"
command = "look"

[[keybinds]]
key = "Ctrl+H"
action = "send"
command = "health"

[[keybinds]]
key = "Ctrl+Shift+S"
action = "send_silent"
command = "stance defensive"
```

## Keybind Actions Reference

| Action | Description |
|--------|-------------|
| `send` | Send command to game |
| `send_silent` | Send without echo |
| `scroll_up` | Scroll focused window up |
| `scroll_down` | Scroll focused window down |
| `scroll_top` | Scroll to top |
| `scroll_bottom` | Scroll to bottom |
| `focus_next` | Focus next window |
| `focus_prev` | Focus previous window |
| `focus_window` | Focus specific window |
| `clear_window` | Clear focused window |
| `toggle_border` | Toggle window border |
| `menu` | Open menu |
| `quit` | Exit application |
