# Hotbars

Bars of clickable command buttons — VellumFE's take on barbar-style action
bars. Buttons send a game command on click (or hotkey), can overlay a
countdown, and can restyle themselves while a condition holds (hidden,
stunned, low health, a spell about to drop...).

Bars are defined once in [hotbars.toml](../configuration/hotbars-toml.md)
and displayed by `hotkeybar` windows in a layout. Several windows can show
the same bar, and one layout can show several bars.

## Basic Usage

Prefer the built-in editor: `.hotbars` (TUI and GUI) creates and edits bars
without touching TOML. Then add a window for the bar:

```toml
[[windows]]
name = "actions"
widget_type = "hotkeybar"
row = 38
col = 0
rows = 1
cols = 60
bar = "default"              # bar name from hotbars.toml
orientation = "horizontal"   # or "vertical" (one button per row)
```

Or in-app: `.addwindow hotkeybar`.

## Display

```
┌─ Actions ──────────────────────────────────────┐
│ [Look] [Hidden] [Defensive] [Offensive] [Heal] │
└────────────────────────────────────────────────┘
```

- Buttons show their `label`, restyled (color, relabel, dim) while a
  state condition matches — e.g. the Hide button turns green and reads
  "Hidden" while you're hidden, or dims during roundtime.
- Buttons with a countdown source show remaining seconds
  (`Heal  12s`).
- In the GUI, hovering shows the tooltip and hotkey.

## Hotkeys

Each button can bind a key combo (`keybinds.toml` syntax, e.g. `alt+h`,
`f5`). Hotkeys register automatically while a window shows the bar;
existing `keybinds.toml` bindings win on conflict.

## Commands

| Command | Purpose |
|---------|---------|
| `.hotbars` / `.hotbar` | Open the hotbar editor |
| `.reload hotbars` | Reload hotbars.toml from disk |

## Per-Character Bars

The global `~/.vellum-fe/global/hotbars.toml` applies to all characters. A
per-character `~/.vellum-fe/profiles/<name>/hotbars.toml` can define bars
too — a character bar with the same name replaces the global one entirely.

See [hotbars.toml](../configuration/hotbars-toml.md) for the full button
and condition schema.
