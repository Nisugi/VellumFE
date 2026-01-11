# Customization

Create your perfect interface with custom layouts, highlights, keybinds, and sounds.

## Topics

- [Creating Layouts](./layouts.md) - Design custom window arrangements
- [Highlight Patterns](./highlights.md) - Color and style game text
- [Keybind Actions](./keybinds.md) - Custom keyboard shortcuts
- [Sound Alerts](./sounds.md) - Audio notifications for game events

## Quick Start

### Custom Layout

1. Press F1 → Windows → Add Window
2. Position and resize windows
3. Press Ctrl+S to save

### Add Highlights

Edit `~/.vellum-fe/highlights.toml`:

```toml
[[highlights]]
name = "death"
pattern = "appears dead"
foreground = "#00FF00"
bold = true
sound = "kill.wav"
```

### Custom Keybinds

Edit `~/.vellum-fe/keybinds.toml`:

```toml
[keybinds]
"f2" = { command = "stance offensive" }
"f3" = { command = "stance defensive" }
```

### Sound Alerts

1. Place `.wav` files in `~/.vellum-fe/sounds/`
2. Reference in highlights:

```toml
[[highlights]]
name = "whisper"
pattern = "whispers to you"
sound = "whisper.wav"
```
