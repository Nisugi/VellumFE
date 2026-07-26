# Room Window

Displays current room information: name, description, objects, players, and exits.

## Basic Usage

```toml
[[windows]]
name = "room"
widget_type = "room"
row = 0
col = 0
rows = 10
cols = 50
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `show_name` | bool | false | Show room name inline in the content (the name always appears as the border title) |
| `show_desc` | bool | true | Show description |
| `show_objs` | bool | true | Show objects/creatures |
| `show_players` | bool | true | Show other players |
| `show_exits` | bool | true | Show obvious exits |

## Examples

### Full Room Display
```toml
[[windows]]
name = "room"
widget_type = "room"
rows = 12
cols = 60
show_name = true
show_desc = true
show_objs = true
show_players = true
show_exits = true
```

### Compact (Name + Exits Only)
```toml
[[windows]]
name = "room"
widget_type = "room"
rows = 3
cols = 40
show_name = true
show_desc = false
show_objs = false
show_players = false
show_exits = true
```

### Description Only
```toml
[[windows]]
name = "room"
widget_type = "room"
rows = 8
cols = 50
show_name = true
show_desc = true
show_objs = false
show_players = false
show_exits = false
```

## Display

```
┌─ Town Square ─────────────────────────┐
│ The center of town bustles with       │
│ activity. A large fountain dominates  │
│ the square.                           │
│                                       │
│ You also see a town guard.            │
│ Also here: Adventurer, Merchant       │
│ Obvious exits: north, east, south     │
└───────────────────────────────────────┘
```

In the **GUI**, the room renders as one Wrayth-style flowing block — the
description runs into "You also see…" with clickable links throughout,
and the room name is a heading sized to the window's text size (the
`show_name` toggle and border-title behavior above are TUI-specific).

## Interaction

- Click creature/player names to interact
- Right-click for context menu
