# Items

Lists non-creature objects on the ground (dropped items, loot, furniture).

## Basic Usage

```toml
[[windows]]
name = "items"
widget_type = "items"
row = 0
col = 0
rows = 10
cols = 30
```

## Display

```
┌─ Items [04] ──────────────┐
│ a silver ring             │
│ some gold coins           │
│ a wooden chest            │
│ a torn scroll             │
└───────────────────────────┘
```

- Shows item names from room
- Count in title
- Updates when room changes or items are dropped/picked up

## Interaction

- Click item to interact (look, get)
- Right-click for context menu
- Drag to inventory to pick up

## Example

```toml
[[windows]]
name = "loot"
widget_type = "items"
row = 20
col = 100
rows = 8
cols = 30
show_border = true
title = "Ground"
```

## Note

Items widget shows objects from the room description that are NOT creatures. Creatures appear in the [Targets](./targets.md) widget instead.
