# Hands

Display items held in the left and right hands (and the prepared spell).

Which hand a widget shows is determined by its **window name**: `left` (or
`left_hand`), `right` (or `right_hand`), and `spell` (or `spell_hand`). The
easiest way to add them is the built-in `left`, `right`, and `spell`
templates via `.addwindow` or the Add Window menu.

## Basic Usage

```toml
[[windows]]
name = "right"
widget_type = "hand"
icon = "R:"
row = 0
col = 0
rows = 1
cols = 25
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `icon` | string | from template | Prefix icon (e.g. `"L:"`, `"R:"`, `"S:"`) |
| `icon_color` | string | window color | Icon color |
| `text_color` | string | window color | Item text color override |

## Example: Side by Side

```toml
[[windows]]
name = "right"
widget_type = "hand"
icon = "R:"
row = 0
col = 0
rows = 1
cols = 25
show_border = false

[[windows]]
name = "left"
widget_type = "hand"
icon = "L:"
row = 0
col = 25
rows = 1
cols = 25
show_border = false
```

## Interaction

- Click the item name to interact
- Right-click for a context menu
- When nothing is held, only the icon (e.g. `R:`) is shown
