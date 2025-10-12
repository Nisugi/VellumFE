# Targets and Players Widgets

The Targets and Players widgets provide scrollable lists for tracking combat targets and players in your current room.

## Overview

Both widgets use the ScrollableContainer pattern (same as Active Effects) to display dynamic lists with:
- Auto-updating count in title
- Scrollable list when content exceeds window height
- Mouse wheel and keyboard scrolling support
- Status indicators displayed as suffixes

## Targets Widget

Displays all combat targets in the current room with status indicators.

### Features

- **Title**: Shows count as "Targets [05]"
- **Current Target Indicator**: Current target marked with "►" prefix
- **Status Indicators**: Shows target status on the right side
  - `[stu]` - Stunned
  - `[sit]` - Sitting
  - `[kne]` - Kneeling
  - `[sle]` - Sleeping
  - `[fro]` - Frozen
  - `[fly]` - Flying
  - `[dead]` - Dead (shown without bold)
- **Scrollable**: Use mouse wheel or keyboard to scroll through targets

### Configuration Example

```toml
[[ui.windows]]
name = "targets"
widget_type = "targets"
row = 0
col = 100
rows = 10
cols = 25
show_border = true
border_style = "single"
title = "Targets"
```

### Creating the Window

```
.createwindow targets
```

### Scrolling

- **Mouse Wheel**: Scroll up/down within the window
- **Keyboard**: Tab to focus, then use arrow keys or Page Up/Down

## Players Widget

Displays all player characters in the current room with status indicators.

### Features

- **Title**: Shows count as "Players [19]"
- **Status Indicators**: Shows player status on the right side
  - `[sit]` - Sitting
  - `[kne]` - Kneeling
  - `[sle]` - Sleeping
  - `[fly]` - Flying
  - (No status indicator means standing)
- **Scrollable**: Use mouse wheel or keyboard to scroll through players

### Configuration Example

```toml
[[ui.windows]]
name = "players"
widget_type = "players"
row = 10
col = 100
rows = 10
cols = 25
show_border = true
border_style = "single"
title = "Players"
```

### Creating the Window

```
.createwindow players
```

### Scrolling

- **Mouse Wheel**: Scroll up/down within the window
- **Keyboard**: Tab to focus, then use arrow keys or Page Up/Down

## Required Lich Script

Both widgets require the `targetlist.lic` script to be running:

```ruby
;go2 targetlist.lic
```

Or start it automatically:

```ruby
;autostart add targetlist
```

### What targetlist.lic Does

- Monitors `GameObj.targets` and `GameObj.pcs`
- Sends target data to the `combat` stream
- Sends player data to the `playerlist` stream
- Sends counts to `targetcount` and `playercount` streams
- Updates continuously as targets/players change

## Technical Details

### Stream Mapping

- **Targets Widget**: Listens to `combat` stream
- **Players Widget**: Listens to `playerlist` stream

### Data Format

**Targets Stream:**
```xml
<pushStream id="combat"/>
<color ul='true'><b>[stu] goblin</b></color>, <b>[sit] troll</b>, <b>bandit</b>
<popStream/>
```

**Players Stream:**
```xml
<pushStream id="playerlist"/>
<b>[sit] Player1</b>, <b>Player2</b>, <b>[kne] Deddalus</b>
<popStream/>
```

### Stream Buffer Accumulation

Both widgets use stream buffer accumulation to handle XML-segmented text:
1. On `StreamPush` - Clear buffer for accumulation
2. On `Text` elements - Accumulate all text in buffer
3. On `StreamPop` - Parse complete buffer and update widget

This ensures all targets/players are captured, not just the last one.

## Customization

### Border Styles

```
.border targets single #00ff00     # Green single border
.border players rounded #ff00ff    # Magenta rounded border
```

### Renaming

```
.rename targets "Combat Targets"
.rename players "Room Players"
```

### Window Sizing

Adjust `rows` and `cols` in config to fit your layout. The widgets will automatically show a scrollbar indicator when content exceeds visible area.

### No Border Layout

```toml
show_border = false
```

When border is hidden, the title (with count) still displays at the top of the widget.

## Comparison with Active Effects

All three widgets (Targets, Players, Active Effects) use the same ScrollableContainer pattern:

| Feature | Targets | Players | Active Effects |
|---------|---------|---------|----------------|
| Scrollable | ✅ | ✅ | ✅ |
| Count in Title | ✅ | ✅ | ✅ |
| Status Suffix | ✅ | ✅ | ❌ |
| Progress Bar | ❌ | ❌ | ✅ |
| Duration | ❌ | ❌ | ✅ |
| Prefix Indicator | ✅ (current target) | ❌ | ❌ |

## Troubleshooting

### Widget shows [00] count

**Problem**: targetlist.lic is not running or not sending data

**Solution**:
```ruby
;kill targetlist
;go2 targetlist.lic
```

### Only last target/player showing

**Problem**: Old version without stream buffer fix

**Solution**: Update to latest version and rebuild

### Status indicators not showing

**Problem**: targetlist.lic might be old version without status support

**Solution**: Update targetlist.lic to include status in player/target output

### Cannot scroll

**Problem**: Widget not properly configured in scroll handlers

**Solution**: Ensure widget type is "targets" or "players" (not "text")

## Related Commands

```
.createwindow targets          # Create targets widget
.createwindow players          # Create players widget
.deletewindow targets          # Remove targets widget
.deletewindow players          # Remove players widget
.windows                       # List all windows
.savelayout                    # Save current layout with these widgets
```

## Performance Notes

- Both widgets update continuously via targetlist.lic (runs every game pulse)
- Minimal performance impact - updates only when data changes
- Efficient ScrollableContainer pattern reuses items
- No memory leaks - old items are cleared before adding new ones

## See Also

- [Widget Reference](Widget-Reference.md) - All available widgets
- [Stream Routing](Stream-Routing.md) - Understanding game streams
- [Configuration Guide](Configuration-Guide.md) - Window configuration

---

← [Widget Reference](Widget-Reference.md) | [Home](Home.md) | [Configuration Guide](Configuration-Guide.md) →
