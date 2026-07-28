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
reserved**: `controller_wheel:portals` (r3 by default — the wheel aims
with the left stick, so its own click would jostle out stray moves)
builds its slices from the current room's noun exits at open time —
the same list `.portal` resolves — shadowing any static wheel of that
name),
and `controller_overlay`
(toggle the binding legend — curated by `[controller_overlay]
buttons`, with `shift/<button>` entries for the shift bank). Rumble
lives in `[controller_rumble]` (patterns `off`/`short`/`long`/`double`
per event). All of it is edited in the `.controller` editor's tabs —
hand-editing is never required.

Interact mode and popup-menu navigation are configurable too.
`interact_select` activates the interact focus (walk an exit, open a
creature/object menu) and confirms a menu item; `menu_up` / `menu_down` /
`menu_left` / `menu_right` move a menu selection; `menu_cancel` closes a
menu. By default these live on the d-pad (navigate), south (select), and
east (cancel) — binding one to another button moves the role there and
frees the physical default. Two escape hatches are always guaranteed no
matter how you rebind: the `interact_mode` button always exits interact
mode, and **east always cancels a menu**.

Input feel lives in `[controller_tuning]` (Tuning tab). `movement_stick`
(`"left"`/`"right"`) chooses which stick walks the compass; the other
stick aims the wheel and scrolls the story. The radial wheel is
**dwell-driven**: aim a slice, rest on it, and it *commits* — leaves
after `aim_dwell_ms`, folders and the reserved Back slice after
`nav_dwell_ms` (folders auto-descend, Back auto-ascends). Releasing the
wheel button fires the committed leaf; returning the stick to center
before releasing cancels. Sweeping across the ring never commits the
slices you pass through, so a far slice is safe to reach. Inside a folder
a Back slice is reserved at the `back_slice` screen anchor
(`up`/`down`/`left`/`right` and the four diagonals). `deadzone` (percent)
is how far the stick must deflect before a slice registers; a `0` dwell
means instant commit. `fire_debounce_ms` suppresses double-fires and
`release_grace_ms` keeps a still-deflected stick from walking as the
wheel closes. `south`/`east` remain optional accelerators while the wheel
is up (fire/descend now, back up now).

`fire_mode` chooses how a *committed leaf* fires (folders always descend
on dwell and are never fired by these modes; cancel is unchanged):

- `"release"` (default) — the behavior above: dwell to commit, fire when
  the wheel button comes up.
- `"edge"` — fire the instant deflection crosses `edge_threshold` (percent),
  no dwell. Fastest on sparse wheels. The re-arm-until-center guard means
  it fires once per hold, not repeatedly as you sweep the ring.
- `"retract"` — dwell to commit, then fire as soon as deflection drops
  `retract_delta` (percent) below its peak — a small inward flick, without
  waiting for a full return to center. Best when recenter-based firing
  feels sluggish.

Both thresholds are exposed so you can tune the feel. Every field is
optional and defaults to the shipped feel. Fire modes apply to the
native controller (they read the analog stick); the phone client's touch
wheel is dwell/release only.

Wheel slices resolve `<target_id>`/`<target_noun>` against the interact
focus, exactly like bound interact macros — so a combat wheel slice such
as `cast at <target_id>` fires at the creature currently selected in
interact mode. Slices without a placeholder are sent as-is; a slice that
needs a target with nothing focused is dropped (not sent literally) with
a note.

Each wheel declares, in the Wheels tab, which **button** opens it
and which **stick** aims it (stored in `[controller_wheels_meta.<name>]`).
The Wheels tab is the single place to set a wheel's button — the
`controller_wheel` / `controller_wheel:<name>` actions are no longer in
the Base tab's action dropdown to avoid two sources of truth. The reserved
**`portals`** wheel has its own permanent, non-deletable Wheels-tab entry
(`portals (dynamic)`): it exposes the same **Opens with** and **Aim
stick** fields, but no slice list, because its slices are generated from
the room every time it opens.
The button field is a convenience: saving it writes the matching
`[controller]` entry, which remains the runtime authority — so if the two
ever disagree, `[controller]` wins and a note says which button really
opens the wheel, and two wheels claiming one button are flagged. When a
wheel's meta doesn't record a button (e.g. it was bound before the Wheels
tab existed), the editor back-fills **Opens with** from `[controller]` so
you always see the real key. The
stick field overrides the global `movement_stick` while that wheel is
open: name the movement stick and walking is silenced for the wheel's
duration; name the other and movement stays live (e.g. an exits wheel
aimed with the right stick while you keep walking on the left). Left
unset, a wheel aims with the non-movement stick as before.

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
