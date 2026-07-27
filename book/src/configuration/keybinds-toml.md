# keybinds.toml

Keyboard shortcuts, organized into three sections by priority. You can edit
this file directly (then `.reload keybinds`) or use the in-app editor
(`.keybinds` to browse, `.addkeybind` to add).

## File Structure

The file maps keys to actions in three sections:

```toml
[app]      # Always active, highest priority (quit, search, close)
quit = "ctrl+c"
start_search = "ctrl+f"
close_window = "esc"

[menu]     # Active inside menus, forms, browsers, editors
navigate_up = "up"
select = "enter"
save = "ctrl+s"

[user]     # Game mode only — your customizations go here
enter = "send_command"
up = "previous_command"
"ctrl+r" = "send_last_command"
num_8 = { macro_text = "n\r" }
```

Note the orientation differs: `[app]` and `[menu]` are `action = "key"`,
while `[user]` is `"key" = "action"`.

## Key Names

Combine modifiers with `+`: `"ctrl+shift+a"`, `"alt+page_up"`.
Quote any name containing `+` or symbols.

| Group | Names |
|-------|-------|
| Modifiers | `ctrl`, `alt`, `shift` |
| Function keys | `f1` – `f12` |
| Arrows | `up`, `down`, `left`, `right` |
| Navigation | `home`, `end`, `page_up`, `page_down` |
| Editing | `insert`, `delete`, `backspace`, `enter`, `tab`, `esc`, `space` |
| Numpad | `num_0` – `num_9`, `"num_+"`, `"num_-"`, `"num_*"`, `"num_/"`, `"num_."` |

> **Tip**: If backspace doesn't work, your terminal may send `delete`
> instead. Run with `RUST_LOG=debug` and check the log for `KEY EVENT`
> lines to see what your terminal actually sends.

## Actions

Bind any of these in `[user]`:

| Action | Description |
|--------|-------------|
| `send_command` | Send the input line to the game |
| `previous_command` / `next_command` | Command history |
| `send_last_command` / `send_second_last_command` | Repeat recent commands |
| `cursor_left` / `cursor_right` / `cursor_home` / `cursor_end` | Move cursor |
| `cursor_word_left` / `cursor_word_right` | Move by word |
| `cursor_backspace` / `cursor_delete` | Delete characters |
| `switch_current_window` | Focus next window |
| `scroll_current_window_up_page` / `..._down_page` | Scroll by page |
| `scroll_current_window_up_one` / `..._down_one` | Scroll by line |
| `start_search` / `next_search_match` / `prev_search_match` / `clear_search` | In-window search |
| `toggle_performance_stats` | Performance overlay |
| `stop_travel` | Cancel the active `.go2` trip (while traveling, Esc does this by default) |
| `interact_mode` | Toggle [interact mode](../frontends/gui.md#interact-mode) (GUI; default `F6`) |
| `tts_next` / `tts_previous` / `tts_next_unread` / `tts_stop` | Text-to-speech navigation |
| `tts_mute_toggle` / `tts_increase_volume` / `tts_decrease_volume` / `tts_increase_rate` / `tts_decrease_rate` | TTS controls |

## Controller Bindings

The `[controller]` table (global file only — pads are per-desk, not
per-character) maps gamepad buttons to the same actions and macros as
`[user]`. Edit with `.controller` in the GUI. Buttons: `south`, `east`,
`north`, `west`, `dpad_up`/`down`/`left`/`right`, `l1`, `r1`, `l2`,
`r2`, `l3`, `r3`, `select`, `start`, `guide`.

```toml
[controller]
start = "interact_mode"
dpad_up = { macro_text = "n\r" }
```

Inside interact mode and context menus the d-pad, South, and East are
fixed navigation keys; bindings apply outside those modes.

Controller-specific actions: `controller_shift` (hold: buttons use
`[controller_shift]`, a second bank in the same format),
`controller_wheel` / `controller_wheel:<name>` (hold: radial command
wheel — default ring in `[[controller_wheel]]`, named rings in
`[controller_wheels.<name>]`; slices take `label`, `command`, optional
`color`, and nested `slices` for folders — the phone client renders
these same wheels for its `wheel` binds; the name **`portals` is
reserved**: `controller_wheel:portals` (l3 by default) builds its
slices from the current room's noun exits at open time — the same
list `.portal` resolves — shadowing any static wheel of that name),
and `controller_overlay`
(toggle the binding legend — curated by `[controller_overlay]
buttons`, with `shift/<button>` entries for the shift bank). Rumble
lives in `[controller_rumble]` (patterns `off`/`short`/`long`/`double`
per event). All of it is edited in the `.controller` editor's tabs —
hand-editing is never required.

## Macros

Send text with a keypress using the inline-table form. `\r` presses Enter:

```toml
[user]
num_8 = { macro_text = "n\r" }              # north
num_2 = { macro_text = "s\r" }              # south
f5 = { macro_text = "stance defensive\r" }
f6 = { macro_text = "hide\r" }              # omit \r to just type it
```

The default file ships numpad movement macros (`num_1`–`num_9` for
directions, `num_0` down, `"num_."` up, `"num_+"` look, and so on).

A macro can chain several commands with `\r` between them, and pause
between commands with a **sleep segment**: a segment that is just `s`
followed by seconds (`s0.5`, `s2`, `s90` — decimals fine, no upper
bound). Everything after the sleep goes out when the pause elapses;
consecutive sleeps add up. Spaces around segments are ignored, and a
bare `s` is still south.

```toml
f7 = { macro_text = "stand\rs1.5\rout\r" }   # stand, wait 1.5s, leave
f8 = { macro_text = "prep 101\r s3.2 \rcast\r" }
```

Sleep segments work anywhere command text is dispatched — keyboard
macros, controller binds, hotbar buttons, macro buttons (including taps
from the phone client), and wheel slices.

While **interact mode** is focused on an entity, macros may reference it
with `<target_id>` (exist id) and `<target_noun>`:

```toml
[controller]
west = { macro_text = "target #<target_id>\rincant 611\r" }
```

A placeholder macro pressed with nothing focused is dropped (with a
message) rather than sent literally. Exits have no exist id.

## Keybind Profiles

Save and swap whole keybind sets:

```
.savekeybinds hunting
.loadkeybinds hunting
.keybindprofiles          # list saved profiles
```
