# Transparent Overlays

Create semi-transparent widgets that layer over other content.

## Goal

Display status information as overlays without completely blocking the view of the main game window.

## Basic Transparent Widget

```toml
[[widgets]]
type = "progress"
name = "health_overlay"
title = ""
x = 1
y = 1
width = 20
height = 1
data_source = "vitals.health"
z_index = 100
transparent = true
opacity = 0.7
border = false
```

## Overlay Status Bar

Minimal status overlay at top of screen:

```toml
# Transparent status strip
[[widgets]]
type = "dashboard"
name = "status_overlay"
x = 0
y = 0
width = 100
height = 1
z_index = 100
transparent = true
opacity = 0.8
border = false
components = ["health_mini", "mana_mini", "rt_mini"]

# Component definitions
[widgets.status_overlay.health_mini]
type = "progress"
width = 15
data_source = "vitals.health"
format = "HP:{value}"

[widgets.status_overlay.mana_mini]
type = "progress"
width = 15
data_source = "vitals.mana"
format = "MP:{value}"

[widgets.status_overlay.rt_mini]
type = "countdown"
width = 10
data_source = "roundtime"
format = "RT:{value}"
```

## Corner Vitals Overlay

Health/mana in corner:

```toml
[[widgets]]
type = "text"
name = "main"
x = 0
y = 0
width = 100
height = 100
z_index = 1

# Health overlay (top left)
[[widgets]]
type = "progress"
name = "health"
x = 1
y = 1
width = 15
height = 1
data_source = "vitals.health"
z_index = 50
transparent = true
opacity = 0.75
border = false
show_text = true
format = "❤ {percent}%"

# Mana overlay (below health)
[[widgets]]
type = "progress"
name = "mana"
x = 1
y = 2
width = 15
height = 1
data_source = "vitals.mana"
z_index = 50
transparent = true
opacity = 0.75
border = false
show_text = true
format = "✦ {percent}%"
```

## Overlay HUD Panel

Grouped status overlays:

```toml
[[widgets]]
type = "dashboard"
name = "hud"
x = "75%"
y = 1
width = "24%"
height = 15
z_index = 100
transparent = true
opacity = 0.85
border_style = "rounded"
border_opacity = 0.5
components = ["vitals", "timers", "status"]

[widgets.hud.vitals]
layout = "vertical"
items = ["health", "mana", "stamina", "spirit"]

[widgets.hud.timers]
layout = "horizontal"
items = ["roundtime", "casttime"]

[widgets.hud.status]
type = "indicator"
indicators = ["hidden", "stunned", "prone"]
```

## Notification Overlay

Temporary overlay for alerts:

```toml
[[widgets]]
type = "text"
name = "notification"
x = "center"
y = 5
width = 40
height = 3
z_index = 200
transparent = true
opacity = 0.9
visible = false
auto_hide = 3000
border_style = "double"
align = "center"
```

Trigger configuration:
```toml
[[triggers]]
pattern = "You are stunned"
command = ".notify_show Stunned!"
```

## Opacity Levels

Recommended opacity settings:

| Use Case | Opacity | Notes |
|----------|---------|-------|
| Critical alerts | 0.95 | Nearly opaque |
| Status bars | 0.75-0.85 | Readable but see-through |
| Background info | 0.5-0.6 | Subtle overlay |
| Watermark | 0.2-0.3 | Barely visible |

## How It Works

### Transparency Rendering

When `transparent = true`:
1. Background is not filled
2. Only text/graphics are drawn
3. `opacity` affects all rendering (0.0-1.0)

### Z-Index Stacking

```
z_index = 1    # Base layer (main window)
z_index = 50   # Mid layer (status)
z_index = 100  # Top layer (HUD)
z_index = 200  # Alert layer (notifications)
```

### Color Blending

With transparency, colors blend:
```toml
# Original: #ff0000 (red) at 0.5 opacity
# Over:     #000000 (black) background
# Result:   ~#800000 (dark red)
```

## Advanced: Conditional Transparency

Change opacity based on state:

```toml
[[widgets]]
type = "progress"
name = "health"
transparent = true
opacity = 0.5

# Increase opacity when low
[widgets.health.dynamic_opacity]
condition = "value < 30"
opacity = 0.95
```

## Tips

1. **Test Readability**: Ensure text is readable over content
2. **Use High Contrast**: Bright colors work better transparent
3. **Consider Activity**: Busy areas need lower opacity
4. **Add Borders**: Light borders improve definition

## Troubleshooting

### Text Hard to Read

- Increase opacity
- Add background blur (if supported)
- Use high contrast colors
- Add shadow/outline to text

### Overlay Blocks Interaction

```toml
click_through = true  # Pass clicks to window below
```

### Flickering

- Reduce render rate for overlay
- Enable double buffering
- Use solid backgrounds for fast-updating widgets

## Terminal Support

Transparency requires terminal support:
- **Full support**: iTerm2, Kitty, WezTerm
- **Partial**: Windows Terminal (with settings)
- **None**: Basic terminals, SSH

Check support:
```bash
echo $COLORTERM  # Should include "truecolor"
```

## See Also

- [Floating Compass](./floating-compass.md)
- [Creating Themes](../customization/creating-themes.md)
- [Display Issues](../troubleshooting/display-issues.md)

