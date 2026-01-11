# Widgets

Widgets are the visual building blocks of your layout. Each widget type displays specific game information.

## Widget Types

| Type | Purpose |
|------|---------|
| [text](./text-windows.md) | Scrollable game text |
| [tabbedtext](./tabbed-text.md) | Multiple streams in tabs |
| [progress](./progress-bars.md) | Health, mana, stamina bars |
| [countdown](./countdowns.md) | Roundtime, cast time timers |
| [compass](./compass.md) | Available exits |
| [hand](./hands.md) | Items in hands |
| [indicator](./indicators.md) | Status conditions |
| [dashboard](./dashboard.md) | Multi-indicator panel |
| [room](./room-window.md) | Room name, description, exits |
| [injury_doll](./injury-doll.md) | Body part injuries |
| [active_effects](./active-effects.md) | Buffs and debuffs |
| [targets](./targets.md) | Creatures in room |
| [players](./players.md) | Players in room |
| [items](./items.md) | Items on ground |
| [inventory](./inventory.md) | Carried items |
| [spells](./spells.md) | Known spells |
| [container](./containers.md) | Container contents |

## Common Properties

All widgets share these properties:

```toml
[[windows]]
name = "my_widget"              # Unique identifier
widget_type = "text"            # Widget type
row = 0                         # Top position
col = 0                         # Left position
rows = 10                       # Height
cols = 40                       # Width
visible = true                  # Show/hide
show_border = true
border_style = "single"         # single, double, rounded, thick
border_color = "#808080"
title = "Custom Title"
```

## Adding Widgets

1. **Via Menu**: F1 → Windows → Add Window → [Category] → [Widget]
2. **Via Command**: `.addwindow widgetname`
3. **Via Config**: Edit layout.toml directly

## Categories

Widgets are organized into categories in the Add Window menu:

| Category | Widgets |
|----------|---------|
| Text Windows | text, tabbedtext |
| Status | progress, countdown, hand, indicator, dashboard |
| Navigation | compass, room |
| Entity | targets, players, items |
| Lists | inventory, spells, container |
| Other | injury_doll, active_effects |
