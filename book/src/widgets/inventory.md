# Inventory

Displays items carried on your character.

## Basic Usage

```toml
[[windows]]
name = "inventory"
widget_type = "inventory"
row = 0
col = 0
rows = 15
cols = 35
```

## Display

Shows items from the `inv` stream:

```
┌─ Inventory ────────────────┐
│ a leather backpack         │
│ a silver ring              │
│ some gold coins            │
│ a steel sword              │
└────────────────────────────┘
```

## Interaction

- Click item to interact
- Right-click for context menu
- Drag items to rearrange or drop

## Example

```toml
[[windows]]
name = "inventory"
widget_type = "inventory"
row = 0
col = 100
rows = 20
cols = 40
show_border = true
title = "Carried Items"
```

## Updating

Inventory updates when you:
- Type `inventory` command
- Pick up or drop items
- Open/close containers
