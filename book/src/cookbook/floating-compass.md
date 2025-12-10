# Floating Compass

Overlay a compass on top of the main text window for space efficiency.

## Goal

Display the compass as a semi-transparent overlay, saving vertical space while keeping navigation visible.

## Basic Floating Compass

```toml
# Main window (full width)
[[widgets]]
type = "text"
name = "main"
title = "Game"
x = 0
y = 0
width = 100
height = 85
streams = ["main", "room", "combat"]
z_index = 1

# Floating compass (top right corner)
[[widgets]]
type = "compass"
name = "compass"
x = 85
y = 1
width = 14
height = 7
z_index = 10
style = "compact"
border = false
clickable = true
```

## Transparent Compass

With transparency support:

```toml
[[widgets]]
type = "compass"
name = "compass"
x = 85
y = 1
width = 14
height = 7
z_index = 10
style = "unicode"
transparent = true
opacity = 0.8
```

## Corner Positions

### Top Right (Default)

```toml
[[widgets]]
type = "compass"
name = "compass"
x = "86%"
y = 1
anchor = "top_right"
```

### Top Left

```toml
[[widgets]]
type = "compass"
name = "compass"
x = 1
y = 1
anchor = "top_left"
```

### Bottom Right

```toml
[[widgets]]
type = "compass"
name = "compass"
x = "86%"
y = "78%"
anchor = "bottom_right"
```

## Compact Styles

### Minimal Compass

```toml
[[widgets]]
type = "compass"
name = "compass"
style = "minimal"
width = 7
height = 3
# Shows: N E S W in a row
```

### Icons Only

```toml
[[widgets]]
type = "compass"
name = "compass"
style = "icons"
width = 9
height = 5
# Shows directional arrows only
```

### Full Rose

```toml
[[widgets]]
type = "compass"
name = "compass"
style = "rose"
width = 15
height = 9
# Shows full compass rose with all directions
```

## Compass with Room Name

Combine compass with room title:

```toml
[[widgets]]
type = "compass"
name = "compass_with_room"
x = 75
y = 0
width = 25
height = 10
style = "unicode"
show_room_name = true
room_name_position = "top"
```

## Toggle Visibility

Keybind to show/hide compass:

```toml
[keybinds."ctrl+c"]
action = "toggle_widget"
widget = "compass"
```

## How It Works

### Z-Index Layering

Higher `z_index` draws on top:
```toml
z_index = 1   # Main window (bottom)
z_index = 10  # Compass (top)
```

### Transparency

When `transparent = true`:
- Background is not drawn
- Text underneath may show through
- `opacity` controls visibility (0.0-1.0)

### Click-Through

```toml
click_through = true  # Clicks go to window below
click_through = false # Compass captures clicks
```

## Variations

### Mini Compass

Smallest possible:
```toml
[[widgets]]
type = "compass"
name = "mini_compass"
style = "dots"
width = 5
height = 3
# Shows • for available directions
```

### Direction Indicators Only

Show only when exits available:
```toml
[[widgets]]
type = "compass"
name = "compass"
style = "indicators"
hide_when_empty = true
```

### Centered Floating

```toml
[[widgets]]
type = "compass"
name = "compass"
x = "center"
y = 2
width = 15
height = 7
z_index = 10
```

## Tips

1. **Test Overlap**: Ensure compass doesn't cover important text
2. **Use Borders Wisely**: Borderless saves space
3. **Consider Clickability**: Click navigation is convenient
4. **Adjust Opacity**: Find balance between visible and unobtrusive

## Troubleshooting

### Compass Covers Text

- Reduce compass size
- Move to corner with less activity
- Add toggle keybind

### Clicks Not Working

- Check `clickable = true`
- Verify `z_index` is highest
- Ensure not blocked by `click_through`

## See Also

- [Compass Widget](../widgets/compass.md)
- [layout.toml Reference](../configuration/layout-toml.md)
- [Transparent Overlays](./transparent-overlays.md)

