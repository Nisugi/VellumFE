# Keybinds and Macros

Keybindings let you remap keyboard shortcuts to built-in actions or literal macros. All configuration flows through `keybinds.toml`, the keybind editor popup, and the runtime map built in `App::rebuild_keybind_map`.

## Keybind Anatomy

Each entry in `keybinds.toml` looks like one of the following:

```toml
# Built-in action
["ctrl+e"]
action = "cursor_word_left"

# Macro (sends text to Lich)
["alt+1"]
[["alt+1".macro]]
macro_text = "stance defensive"
```

- The table name (e.g., `["ctrl+e"]`) is the key combination.
- Actions map to `KeyAction` variants defined in `src/config.rs`.
- Macros send `macro_text` verbatim.

### Supported Modifiers and Keys

`parse_key_string` supports:

- Modifiers: `ctrl`, `control`, `alt`, `shift`.
- Named keys: letters (`a`–`z`), numbers (`0`–`9`), function keys (`f1`–`f12`), navigation keys (`home`, `end`, `page_up`, etc.).
- Numpad keys: `num_0` … `num_9`, `num_.`, `num_+`, `num_-`, `num_*`, `num_/`.

Examples: `ctrl+shift+f`, `alt+num_3`, `num_+`, `ctrl+page_up`.

## Built-in Actions

The keybind editor lists all available actions; they correspond exactly to `KeyAction` enum variants:

- **Command Input**: `send_command`, `cursor_left/right`, `cursor_word_left/right`, `cursor_home/end`, `cursor_backspace`, `cursor_delete`.
- **History**: `previous_command`, `next_command`, `send_last_command`, `send_second_last_command`.
- **Window Navigation**: `switch_current_window` (cycles focus), `scroll_current_window_up/down_one`, `scroll_current_window_up/down_page`.
- **Search**: `start_search`, `next_search_match`, `prev_search_match`, `clear_search`.
- **Diagnostics**: `toggle_performance_stats`.
- **Macro**: `send_macro` (populated automatically when you record a macro).

## Managing Keybinds In-Game

- `.listkeybinds` shows active bindings.
- `.addkeybind` opens the form to pick a key combo and action/macro.
- `.editkeybind ctrl+e` edits an existing entry.
- `.deletekeybind ctrl+e` removes one.

The keybind form validates combinations, captures modifiers, and prevents duplicates. Updates are flushed to disk immediately; `App::rebuild_keybind_map` reloads the entire map to keep runtime state in sync.

## Default Bindings

Default `keybinds.toml` ships with comfortable mappings (arrow keys for history, `ctrl+f` for search, etc.). View them via `.listkeybinds` or open the file directly to customize.

## Interaction With Command Input

- Keybind actions run through `App::handle_key_event`. Many actions apply only when input mode is `Normal` or `Search`. When popups are active (`HighlightForm`, `SettingsEditor`, etc.), keystrokes stay inside the popup to avoid losing focus.
- If you need window-scrolling while a popup is open, use mouse wheel inside the target window.

## Troubleshooting

- If a combination does nothing, check for duplicates—the last defined binding wins.
- Numpad keys require the crossterm fork specified in `Cargo.toml` (branch `dev`). Make sure your terminal reports keypad events.
- When macros send to the wrong window, ensure your command input box retains focus (click inside or press `Tab` to exit popup mode).
