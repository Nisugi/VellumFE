# Players

Lists other players in the current room.

## Basic Usage

```toml
[[windows]]
name = "players"
widget_type = "players"
row = 0
col = 0
rows = 8
cols = 25
```

## Display

```
┌─ Players [02] ──────┐
│ Adventurer          │
│ Merchant (sitting)  │
└─────────────────────┘
```

- Shows player names
- Status in parentheses (sitting, kneeling, etc.)
- Count in title

## Interaction

- Click player name to interact
- Right-click for context menu

## Example

```toml
[[windows]]
name = "players"
widget_type = "players"
row = 0
col = 100
rows = 8
cols = 25
show_border = true
title = "Also Here"
```
