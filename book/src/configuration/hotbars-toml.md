# hotbars.toml

Named bars of buttons for the [hotbars widget](../widgets/hotkeybar.md).

**Prefer the built-in editor over hand-editing:** `.hotbars` opens the
hotbar editor in both TUI and GUI.

Location: `~/.vellum-fe/global/hotbars.toml`, plus an optional
per-character `~/.vellum-fe/profiles/<name>/hotbars.toml`. A character bar
with the same name replaces the global one entirely.

## Bars and Buttons

```toml
[[bars]]
name = "default"      # referenced by hotkeybar windows in layout.toml
title = "Actions"

[[bars.buttons]]
id = "hide"           # stable id, unique within the bar
label = "Hide"        # text shown on the button
command = "hide"      # game command sent on click or hotkey
hotkey = "alt+h"      # optional (keybinds.toml syntax); keybinds.toml wins on conflict
tooltip = "Attempt to hide"   # hover text (GUI)
category = "Stealth"  # editor grouping only
```

## Countdowns

An optional `[bars.buttons.countdown]` overlays remaining seconds on the
button:

```toml
[bars.buttons.countdown]
source = "roundtime"          # "roundtime" | "casttime" | "effect"
```

For `source = "effect"`, name the effect to track:

```toml
[bars.buttons.countdown]
source = "effect"
category = "Buffs"            # Buffs | Debuffs | Cooldowns | ActiveSpells
name = "Celerity"
name_match = "exact"          # "exact" or "contains"
```

## Conditional States

`[[bars.buttons.states]]` entries restyle a button while a condition holds.
The first matching state wins. Each state has a `when` condition and a
`style`:

```toml
# Show the Hide button green and relabeled while hidden
[[bars.buttons.states]]
[bars.buttons.states.when]
type = "indicator"
id = "hidden"
active = true
[bars.buttons.states.style]
label = "Hidden"
fg = "#80ff80"

# Dim it during roundtime
[[bars.buttons.states]]
[bars.buttons.states.when]
type = "rt_active"
[bars.buttons.states.style]
dim = true
```

### Condition Types

| `type` | Fields | Matches when |
|--------|--------|--------------|
| `effect_active` | `category`, `name`, `name_match` | The effect is up |
| `effect_inactive` | `category`, `name`, `name_match` | The effect is not up |
| `effect_time` | `category`, `name`, `name_match`, `cmp`, `seconds` | Remaining time compares true (e.g. `cmp = "<"`, `seconds = 30`) |
| `rt_active` | — | Roundtime is running |
| `ct_active` | — | Casttime is running |
| `indicator` | `id`, `active` | A status indicator matches: `standing`, `kneeling`, `sitting`, `prone`, `stunned`, `bleeding`, `hidden`, `invisible`, `webbed`, `joined`, `dead` |
| `vital` | `vital`, `cmp`, `value`, `unit` | A vital compares true: `health`/`mana`/`stamina`/`spirit`, `cmp` one of `<` `<=` `>` `>=`, `unit` `"percent"` or `"absolute"` |
| `spell_affordable` | `number` | The bundled spell table lists static costs for that spell number and your current absolute vitals cover them. Works without Lich. Fails closed on unknown numbers, variable (formula) costs, or before vitals data arrives; unlike Lich's `affordable?` it does not model feats or debuffs. |
| `all` / `any` | `conditions = [ ... ]` | All / any of the nested conditions match |

### Style Fields

| Field | Effect |
|-------|--------|
| `label` | Replace the button text |
| `fg` / `bg` | Text / background color (`#rrggbb`) |
| `dim` | Render the button dimmed |
| `icon` | Icon override for this state (see Icons below) |

A state may also carry its own `[bars.buttons.states.countdown]` (same
schema as the button-level countdown); while the state is active it
replaces the button-level source — e.g. show the cooldown timer only in
the "on cooldown" state. Likewise `command = "..."` on a state replaces
the button's command while active (literal text — `;eq ...` commands are
evaluated by Lich, so dynamic behavior belongs in the command itself).

## Icons (GUI)

Buttons can show an image face from a sprite sheet registered in the
active skin's `[sheets]` table (see the skin docs). Sheets are tiled into
fixed-size cells with no padding, numbered 1-based left→right then
top→bottom — the barbar convention. The TUI always renders the text
label; icons are ignored there, and the GUI falls back to text when no
skin supplies the sheet.

**The `.hotbars` editor does all of this without TOML:** the button form
has a Face selector (Text / Icon / Icon + label), a sheet dropdown, a
clickable "Pick cell from sheet" grid, grayscale/border controls, and the
same icon controls on every state card. The editor's "Icon sheets"
section registers a new sheet — give it a name and an image path and it
copies the PNG into the skin and records it in skin.toml for you. Each
state card also carries its own countdown source ("Countdown while
active") that replaces the button-level one while the state matches.

```toml
[[bars.buttons]]
id = "hide"
label = "Hide"
command = "hide"
icon_mode = "icon"            # "text" (default) | "icon" | "icon_and_label"

[bars.buttons.icon]
sheet = "rogue"               # sheet name from the skin's [sheets] table
cell = 5                      # 1-based cell index
grayscale = false             # desaturate (e.g. for a "not ready" look)
border = "#00ff00"            # optional solid border over the icon
border_width = 3              # pixels, 1-10 (default 2)
border_end = "#004400"        # optional: gradient border toward this color
border_dir = "radial"         # horizontal (default) | vertical |
                              # diagonal_down | diagonal_up | radial | square
```

With `icon_mode = "icon"` the label moves into the hover tooltip. States
can swap the icon (a different cell, or the same cell grayscaled) via
`[bars.buttons.states.style.icon]` — dimmed states automatically use the
grayscale variant. In the skin's `skin.toml`:

```toml
[sheets.rogue]
path = "icons/rogue.png"      # relative to the skin directory
cell = 64                     # cell edge in pixels (default 64)
```

## Reloading

`.reload hotbars` re-reads the file; the editor saves and applies
immediately.
