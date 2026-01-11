# Spells

Displays known spells from the spell list.

## Basic Usage

```toml
[[windows]]
name = "spells"
widget_type = "spells"
row = 0
col = 0
rows = 20
cols = 35
```

## Display

Shows spells from the `Spells` stream:

```
┌─ Spells ───────────────────┐
│ Spirit Warding I (101)     │
│ Spirit Barrier (102)       │
│ Spirit Defense (103)       │
└────────────────────────────┘
```

## Interaction

- Click spell to prepare/cast
- Right-click for context menu

## Example

```toml
[[windows]]
name = "spells"
widget_type = "spells"
row = 0
col = 120
rows = 25
cols = 35
show_border = true
title = "Known Spells"
```

## Note

Spell list is populated at login. Use `spell` command in-game to refresh.
