# Inventory Widget

The inventory widget displays your character's carried items and container contents.

## Overview

Inventory widgets:
- Show items in containers (backpack, cloak, etc.)
- Display item counts and descriptions
- Support expandable container trees
- Enable quick item access

## Configuration

```toml
[[windows]]
name = "inventory"
type = "inventory"

# Position and size
row = 0
col = 80
width = 40
height = 25

# Inventory-specific options
show_containers = true     # Show container hierarchy
expand_containers = false  # Auto-expand all containers
show_count = true          # Show item counts
show_weight = true         # Show weight (if available)
sort_by = "name"           # "name", "type", "none"

# Interaction
clickable = true           # Click to interact
double_click_action = "look"  # "look", "get", "open"
```

## Properties

### show_containers

Display container hierarchy:

```toml
show_containers = true    # Show nested structure
show_containers = false   # Flat item list
```

### expand_containers

Auto-expand containers:

```toml
expand_containers = false   # Collapsed by default
expand_containers = true    # All expanded
```

### show_count

Display item counts:

```toml
show_count = true    # "5 silver coins"
show_count = false   # "silver coins"
```

### show_weight

Display item weight (if available):

```toml
show_weight = true    # Show weight info
show_weight = false   # Hide weight
```

### sort_by

Item sort order:

```toml
sort_by = "name"    # Alphabetical
sort_by = "type"    # By item type
sort_by = "none"    # Game order
```

## Display Format

### Tree View

```
┌─ Inventory ────────────────────────┐
│ ▼ a leather backpack               │
│   ├─ some silver coins (23)        │
│   ├─ a healing potion              │
│   ├─ a bronze lockpick             │
│   └─ ▶ a small pouch               │
│                                    │
│ ▼ a dark cloak                     │
│   ├─ a silver dagger               │
│   └─ a vial of poison              │
│                                    │
│ a sturdy shield                    │
│ a steel longsword                  │
└────────────────────────────────────┘
```

- ▼ = Expanded container
- ▶ = Collapsed container

### Flat View

```
┌─ Inventory ────────────────────────┐
│ some silver coins (23)             │
│ a healing potion                   │
│ a bronze lockpick                  │
│ a silver dagger                    │
│ a vial of poison                   │
│ a sturdy shield                    │
│ a steel longsword                  │
└────────────────────────────────────┘
```

### Compact View

```
┌─ Items ───────────┐
│ coins(23) potion  │
│ lockpick dagger   │
│ poison shield     │
│ longsword         │
└───────────────────┘
```

## Examples

### Full Inventory Panel

```toml
[[windows]]
name = "inventory"
type = "inventory"
row = 0
col = 80
width = 40
height = 30
show_containers = true
expand_containers = false
show_count = true
clickable = true
title = "Inventory"
border_style = "rounded"
```

### Compact Item List

```toml
[[windows]]
name = "items"
type = "inventory"
row = 0
col = 100
width = 25
height = 15
show_containers = false
show_count = true
sort_by = "name"
title = "Items"
```

### Container Focus

```toml
[[windows]]
name = "backpack"
type = "inventory"
row = 10
col = 80
width = 30
height = 15
container = "backpack"     # Focus on specific container
expand_containers = true
title = "Backpack"
```

### Merchant View

```toml
[[windows]]
name = "merchant_inv"
type = "inventory"
row = 0
col = 60
width = 50
height = 35
show_containers = true
expand_containers = true
show_weight = true
sort_by = "type"
clickable = true
double_click_action = "look"
```

## Item Interaction

### Click Actions

| Action | Result |
|--------|--------|
| Single click | Select item |
| Double click | Configurable action |
| Right click | Context menu |

### Context Menu Options

```
┌────────────────┐
│ Look           │
│ Get            │
│ Drop           │
│ Put in...      │
│ Give to...     │
│ ────────────── │
│ Examine        │
│ Appraise       │
└────────────────┘
```

### Keyboard Navigation

| Key | Action |
|-----|--------|
| `↑/↓` | Navigate items |
| `Enter` | Default action |
| `Space` | Expand/collapse |
| `l` | Look at item |
| `g` | Get item |
| `d` | Drop item |

## Item Colors

Color items by type or rarity:

```toml
[[windows]]
name = "inventory"
type = "inventory"

[windows.item_colors]
weapon = "#FF8800"       # Orange
armor = "#888888"        # Gray
potion = "#00FF00"       # Green
scroll = "#FFFF00"       # Yellow
gem = "#FF00FF"          # Magenta
coin = "#FFD700"         # Gold
container = "#00FFFF"    # Cyan
default = "#FFFFFF"      # White
```

### Rarity Colors

```toml
[windows.rarity_colors]
common = "#FFFFFF"       # White
uncommon = "#00FF00"     # Green
rare = "#0088FF"         # Blue
epic = "#FF00FF"         # Purple
legendary = "#FFD700"    # Gold
```

## Container Options

### Specific Container

Focus on a single container:

```toml
container = "backpack"    # Only show backpack contents
container = "cloak"       # Only show cloak contents
```

### Container Depth

Limit nesting depth:

```toml
max_depth = 2    # Only show 2 levels deep
max_depth = 0    # Show all levels (default)
```

### Auto-Refresh

```toml
auto_refresh = true       # Update when inventory changes
refresh_rate = 1000       # Milliseconds (if polling)
```

## Data Source

Inventory data comes from XML elements:

```xml
<inv id="stow">
  <item>a leather backpack</item>
  <item>a dark cloak</item>
</inv>

<container id="backpack">
  <item>some silver coins</item>
  <item>a healing potion</item>
</container>
```

## Integration Examples

### With Hands Display

```toml
# What you're holding
[[windows]]
name = "hands"
type = "hand"
row = 0
col = 80
width = 30
height = 3

# Full inventory below
[[windows]]
name = "inventory"
type = "inventory"
row = 3
col = 80
width = 30
height = 20
```

### Merchant Layout

```toml
# Your inventory (left)
[[windows]]
name = "my_items"
type = "inventory"
row = 0
col = 0
width = 40
height = 30
title = "Your Items"

# Merchant inventory (right)
[[windows]]
name = "merchant"
type = "text"
stream = "merchant"
row = 0
col = 50
width = 40
height = 30
title = "Merchant"
```

### Quick Access Bar

```toml
# Horizontal quick items
[[windows]]
name = "quick_items"
type = "inventory"
row = 0
col = 0
width = 80
height = 3
show_containers = false
sort_by = "type"
layout = "horizontal"
```

## Weight Tracking

If weight tracking is enabled:

```
┌─ Inventory ────────────────────────┐
│ Carried: 45.2 lbs / 100 lbs        │
│ ──────────────────────────────────│
│ ▼ leather backpack (12.5 lbs)      │
│   ├─ silver coins (2.3 lbs)        │
│   └─ healing potion (0.5 lbs)      │
└────────────────────────────────────┘
```

```toml
show_weight = true
show_encumbrance = true
encumbrance_warning = 80    # Warn at 80% capacity
```

## Troubleshooting

### Inventory not updating

1. Verify receiving inventory data
2. Check window is correct type
3. Try manual refresh command

### Containers not expanding

1. Check expand_containers setting
2. Verify container data is being sent
3. Click expand arrow manually

### Missing items

1. Check container filter settings
2. Verify all containers are included
3. Check item parsing in debug log

### Performance with large inventory

1. Set expand_containers = false
2. Reduce max_depth
3. Focus on specific containers

## See Also

- [Hands](./hands.md) - Held items display
- [Room Window](./room-window.md) - Room objects
- [Text Windows](./text-windows.md) - General text display

