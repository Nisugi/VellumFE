# Room Window

The room window displays current room information including name, description, exits, objects, and other players.

## Overview

Room windows:
- Show the current room name and description
- Display obvious exits
- List objects and creatures in the room
- Show other players present
- Update when you move

## Configuration

```toml
[[windows]]
name = "room"
type = "room"

# Position and size
row = 0
col = 80
width = 40
height = 15

# Room-specific options
show_title = true         # Show room name as title
show_description = true   # Show room description
show_exits = true         # Show obvious exits
show_objects = true       # Show items/creatures
show_players = true       # Show other players

# Visual options
title_color = "#FFFFFF"
description_color = "#CCCCCC"
exits_color = "#00FFFF"
objects_color = "#AAAAAA"
players_color = "#00FF00"
```

## Properties

### show_title

Display room name as widget title:

```toml
show_title = true     # Room name in title bar
show_title = false    # No title/generic title
```

### show_description

Display the room description:

```toml
show_description = true    # Full description
show_description = false   # Hide description
```

### show_exits

Display obvious exits:

```toml
show_exits = true     # Show "Obvious exits: n, e, out"
show_exits = false    # Hide exits
```

### show_objects

Display items and creatures:

```toml
show_objects = true   # Show objects in room
show_objects = false  # Hide objects
```

### show_players

Display other players:

```toml
show_players = true   # Show player names
show_players = false  # Hide players
```

## Display Layout

```
┌─ Town Square Central ───────────────┐
│ This is the heart of the town,      │
│ where merchants hawk their wares    │
│ and travelers gather to share news. │
│                                     │
│ You also see a wooden bench, a      │
│ town crier, and a young squire.     │
│                                     │
│ Also here: Adventurer, Merchant     │
│                                     │
│ Obvious paths: north, east, south,  │
│ west, out                           │
└─────────────────────────────────────┘
```

## Component Colors

Style each component separately:

```toml
[[windows]]
name = "room"
type = "room"

# Component colors
[windows.colors]
title = "#9BA2B2"           # Room name
title_bg = "#395573"        # Room name background
description = "#B0B0B0"     # Description text
exits = "#00CCCC"           # Exit directions
objects = "#888888"         # Items/creatures
players = "#00FF00"         # Other players
creatures = "#FF6600"       # Hostile creatures
```

## Examples

### Standard Room Window

```toml
[[windows]]
name = "room"
type = "room"
row = 0
col = 80
width = 40
height = 15
show_title = true
show_description = true
show_exits = true
show_objects = true
show_players = true
border_style = "rounded"
```

### Compact Room Info

```toml
[[windows]]
name = "room"
type = "room"
row = 0
col = 80
width = 40
height = 5
show_description = false   # Hide description
show_objects = false       # Hide objects
show_title = true
show_exits = true
show_players = true
```

### Full Width

```toml
[[windows]]
name = "room"
type = "room"
row = 0
col = 0
width = "100%"
height = 8
show_border = false
transparent_background = true
```

### Description Only

```toml
[[windows]]
name = "room_desc"
type = "room"
row = 0
col = 80
width = 40
height = 10
show_description = true
show_exits = false
show_objects = false
show_players = false
title = "Description"
```

### Exits Only

```toml
[[windows]]
name = "exits"
type = "room"
row = 10
col = 80
width = 40
height = 2
show_description = false
show_objects = false
show_players = false
show_exits = true
show_border = false
```

## Room Components

### Room Name

Set by `<streamWindow id='room' subtitle='Room Name'/>`:

```toml
title_color = "#9BA2B2"
title_bg = "#395573"      # Distinct background
```

### Description

Set by `<component id='room desc'>`:

```toml
description_color = "#B0B0B0"
word_wrap = true          # Wrap long descriptions
```

### Exits

Set by `<compass>` and displayed as text:

```toml
exits_color = "#00CCCC"
exits_prefix = "Obvious paths: "
```

### Objects/Creatures

Set by `<component id='room objs'>`:

```toml
objects_color = "#888888"
creature_highlight = true    # Highlight creatures
creature_color = "#FF6600"   # Creature color
```

### Players

Set by `<component id='room players'>`:

```toml
players_color = "#00FF00"
players_prefix = "Also here: "
```

## Interaction

With `links = true`:

| Input | Action |
|-------|--------|
| Click object | Look/interact |
| Click player | Look at player |
| Click exit | Move that direction |
| Right-click | Context menu |

## Data Source

Room windows receive data from multiple XML elements:

```xml
<streamWindow id='room' subtitle='Town Square'/>
<component id='room desc'>This is the description...</component>
<component id='room objs'>You also see a bench...</component>
<component id='room players'>Also here: Alice, Bob</component>
<compass><dir value='n'/><dir value='e'/></compass>
```

## Troubleshooting

### Room not updating

1. Check you're receiving room data
2. Verify window stream is correct
3. Some areas suppress room info

### Missing components

1. Enable specific show_* options
2. Check game is sending that component
3. Verify component colors are visible

### Text overflow

1. Increase window height
2. Enable word wrap
3. Hide less important components

## See Also

- [Compass](./compass.md) - Visual exit display
- [Text Windows](./text-windows.md) - General text display
- [Highlights](../configuration/highlights-toml.md) - Text styling
