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
| `all` / `any` | `conditions = [ ... ]` | All / any of the nested conditions match |

### Style Fields

| Field | Effect |
|-------|--------|
| `label` | Replace the button text |
| `fg` / `bg` | Text / background color (`#rrggbb`) |
| `dim` | Render the button dimmed |

## Reloading

`.reload hotbars` re-reads the file; the editor saves and applies
immediately.
