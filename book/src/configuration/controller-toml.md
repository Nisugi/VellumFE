# controller.toml

Controller (GUI gamepad) configuration lives in its own
`global/controller.toml` — separate from [keybinds.toml](keybinds-toml.md)
so a controller setup can be shared or version-controlled as one file,
and a malformed edit here cannot take keyboard input down with it.
Controller binds are **global** (pads are per-desk, not per-character).
Everything below is edited in the `.controller` editor (GUI) — hand-editing
is never required.

Installs that predate this split are migrated automatically on first run:
the `[controller*]` tables are moved out of `keybinds.toml` into
`controller.toml` (both files backed up), so existing controller setups
carry over untouched.

## Bindings

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
`color`, optional `span` and `inner` ([geometry](#wheel-geometry)), and
nested `slices` for folders — the phone client renders these same
wheels for its `wheel` binds; the name **`portals` is reserved**:
`controller_wheel:portals` (r3 by default — clicking l3 would nudge the
movement stick into stray steps, while the aim stick's click settles
harmlessly) builds its slices from the current room's noun exits at
open time — the same list `.portal` resolves — shadowing any static
wheel of that name),
and `controller_overlay`
(toggle the binding legend — curated by `[controller_overlay]
buttons`, with `shift/<button>` entries for the shift bank). Rumble
lives in `[controller_rumble]`: a pattern per event (`off`, the
built-ins `short`/`long`/`double`, or the name of one of your own),
plus custom patterns under `[[controller_rumble.patterns]]` — each has
a `name`, `strength` (0–1), `pulse_ms` (length of each buzz), `pulses`
(how many), and `gap_ms` (silence between them). The editor's Rumble
tab has a row per pattern with a **Test** button that plays it on the
pad immediately, unsaved edits included. Custom patterns are also
selectable on any highlight rule, so any text match can buzz the pad —
see [highlights.toml](highlights-toml.md). Built-in names win if a
custom pattern reuses one. All of it is edited in the `.controller`
editor's tabs — hand-editing is never required.

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
(`up`/`down`/`left`/`right` and the four diagonals) — or `back_slice =
"none"` drops the reserved seat entirely and you back out with East.
`deadzone` (percent)
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

### Wheel geometry

Wedges don't have to be even. Per-slice **`span`** fixes a wedge's width
in degrees; whatever remains of the 360° splits evenly among the
span-less slices, so a wheel with no spans keeps the classic even ring.
Bad numbers are never rejected — spans below 30° clamp up and a ring
that doesn't close rescales to fit, with a warning at load and in the
editor telling you what was adjusted. Per-slice **`inner`** sets that
slice's aim floor as a percent of full stick deflection: below it the
slice can't be aimed or committed, so a destructive command can demand a
deliberately deep throw. Unset slices use the global `deadzone`, and the
editor caps the value below the fire thresholds so a slice always has
travel left to fire. A per-wheel **`start`** in
`[controller_wheels_meta.<name>]` rotates the whole ring (degrees, 0 =
up, clockwise). Folder rings anchor to their Back seat instead and
ignore `start` — unless `back_slice = "none"`, in which case they rotate
with it too. The portals wheel always keeps an even ring, since its
slices are rebuilt per room.

Inside a folder, the "go up a level" seat is normally synthesized for
you and pinned at the `back_slice` anchor set globally in
`[controller_tuning]` (the Tuning tab). That anchor is one of the eight
compass directions, or **`none`** to drop the synthesized Back from
*every* folder — you then ascend with the East/B button. To place the
seat yourself on one ring instead, mark a slice **`back = true`**: it
becomes a real slice you can position, size, color, and floor like any
other — dwelling it still ascends instead of firing, and it never sends
a command (any `command` on it is ignored). A folder with an explicit
Back uses your ring verbatim: the synthesized seat, the `back_slice`
anchor, and the anchor rotation all step aside for that folder only. A
`back` slice on the top ring does nothing (there's no level to go up
to) and is flagged; only one Back per ring is useful.

All of this is drawn and edited live in the Wheels tab's **Visual**
designer — see [the GUI chapter](../frontends/gui.md#controllers) — or
typed exactly in its **Numeric** view; both edit the same wheel. On
the wheel itself (designer and live alike), a slice's colored fill
covers only its **activation zone** — from its aim floor out to the
rim — so the empty ring inside the fill is exactly the stick travel
that does nothing.
