# Keybind Actions

Complete reference of all available keybind actions.

## Action Syntax

```toml
[keybinds."key"]
action = "action_name"
```

## Navigation Actions

### Widget Focus

| Action | Description |
|--------|-------------|
| `next_widget` | Focus next widget in tab order |
| `prev_widget` | Focus previous widget |
| `focus_input` | Focus command input |
| `focus_widget` | Focus specific widget (requires `widget` param) |

```toml
[keybinds."tab"]
action = "next_widget"

[keybinds."shift+tab"]
action = "prev_widget"

[keybinds."escape"]
action = "focus_input"

[keybinds."alt+m"]
action = "focus_widget"
widget = "main"
```

### Scrolling

| Action | Description |
|--------|-------------|
| `scroll_up` | Scroll up one page |
| `scroll_down` | Scroll down one page |
| `scroll_half_up` | Scroll up half page |
| `scroll_half_down` | Scroll down half page |
| `scroll_line_up` | Scroll up one line |
| `scroll_line_down` | Scroll down one line |
| `scroll_top` | Scroll to top |
| `scroll_bottom` | Scroll to bottom |

```toml
[keybinds."page_up"]
action = "scroll_up"

[keybinds."page_down"]
action = "scroll_down"

[keybinds."home"]
action = "scroll_top"

[keybinds."end"]
action = "scroll_bottom"
```

## Input Actions

### Command Input

| Action | Description |
|--------|-------------|
| `submit_input` | Submit current input |
| `clear_input` | Clear input line |
| `history_prev` | Previous command in history |
| `history_next` | Next command in history |
| `history_search` | Search command history |

```toml
[keybinds."enter"]
action = "submit_input"

[keybinds."ctrl+u"]
action = "clear_input"

[keybinds."up"]
action = "history_prev"
```

### Text Editing

| Action | Description |
|--------|-------------|
| `cursor_left` | Move cursor left |
| `cursor_right` | Move cursor right |
| `cursor_home` | Move to line start |
| `cursor_end` | Move to line end |
| `delete_char` | Delete character at cursor |
| `delete_word` | Delete word at cursor |
| `backspace` | Delete character before cursor |

## Browser Actions

### Open Browsers

| Action | Description |
|--------|-------------|
| `open_search` | Open text search |
| `open_help` | Open help browser |
| `open_layout_editor` | Open layout editor |
| `open_highlight_editor` | Open highlight editor |
| `open_keybind_editor` | Open keybind editor |

```toml
[keybinds."ctrl+f"]
action = "open_search"

[keybinds."f1"]
action = "open_help"
```

### Search Actions

| Action | Description |
|--------|-------------|
| `search_next` | Find next match |
| `search_prev` | Find previous match |
| `close_search` | Close search |

## Widget Actions

### Tabbed Text

| Action | Description |
|--------|-------------|
| `next_tab` | Switch to next tab |
| `prev_tab` | Switch to previous tab |
| `tab_1` through `tab_9` | Switch to numbered tab |
| `close_tab` | Close current tab |

```toml
[keybinds."ctrl+tab"]
action = "next_tab"

[keybinds."ctrl+shift+tab"]
action = "prev_tab"

[keybinds."ctrl+1"]
action = "tab_1"
```

### Compass

| Action | Description |
|--------|-------------|
| `compass_north` | Go north |
| `compass_south` | Go south |
| `compass_east` | Go east |
| `compass_west` | Go west |
| `compass_northeast` | Go northeast |
| `compass_northwest` | Go northwest |
| `compass_southeast` | Go southeast |
| `compass_southwest` | Go southwest |
| `compass_out` | Go out |
| `compass_up` | Go up |
| `compass_down` | Go down |

## Application Actions

### General

| Action | Description |
|--------|-------------|
| `quit` | Exit application |
| `reload_config` | Reload all configuration |
| `toggle_debug` | Toggle debug overlay |

```toml
[keybinds."ctrl+q"]
action = "quit"

[keybinds."ctrl+r"]
action = "reload_config"
```

### Layout

| Action | Description |
|--------|-------------|
| `toggle_widget` | Toggle widget visibility |
| `maximize_widget` | Maximize focused widget |
| `restore_layout` | Restore default layout |

```toml
[keybinds."ctrl+h"]
action = "toggle_widget"
widget = "health"
```

## TTS Actions

| Action | Description |
|--------|-------------|
| `toggle_tts` | Enable/disable TTS |
| `speak_status` | Speak current status |
| `speak_room` | Speak room description |
| `speak_last` | Repeat last spoken text |
| `stop_speaking` | Stop current speech |
| `tts_rate_up` | Increase speech rate |
| `tts_rate_down` | Decrease speech rate |
| `tts_volume_up` | Increase volume |
| `tts_volume_down` | Decrease volume |

```toml
[keybinds."ctrl+space"]
action = "toggle_tts"

[keybinds."f1"]
action = "speak_status"
```

## Macro Actions

Send game commands:

```toml
[keybinds."f1"]
macro = "attack target"

[keybinds."ctrl+1"]
macro = "prep 101;cast"

# With delay
[keybinds."f5"]
macro = "prep 901;{2000};cast"

# With input prompt
[keybinds."ctrl+g"]
macro = "go $input"
```

### Macro Variables

| Variable | Description |
|----------|-------------|
| `$input` | Prompt for input |
| `$target` | Current target |
| `$lasttarget` | Last targeted creature |
| `{N}` | Delay N milliseconds |

## Action Parameters

Some actions require additional parameters:

```toml
[keybinds."alt+m"]
action = "focus_widget"
widget = "main"          # Required: widget name

[keybinds."ctrl+h"]
action = "toggle_widget"
widget = "health"        # Required: widget name
```

## Complete Example

```toml
# keybinds.toml

# Navigation
[keybinds."tab"]
action = "next_widget"

[keybinds."shift+tab"]
action = "prev_widget"

[keybinds."escape"]
action = "focus_input"

# Scrolling
[keybinds."page_up"]
action = "scroll_up"

[keybinds."page_down"]
action = "scroll_down"

# Movement macros
[keybinds."numpad8"]
macro = "north"

[keybinds."numpad2"]
macro = "south"

# Combat macros
[keybinds."f1"]
macro = "attack target"

[keybinds."f2"]
macro = "stance defensive"

# Application
[keybinds."ctrl+q"]
action = "quit"

[keybinds."ctrl+f"]
action = "open_search"
```

## See Also

- [Keybinds Configuration](../configuration/keybinds-toml.md)
- [Macros](../automation/macros.md)
- [Config Schema](./config-schema.md)

