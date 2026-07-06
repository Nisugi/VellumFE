# Customization

Create your perfect interface with custom layouts, highlights, keybinds,
sounds, and themes.

## Topics

- [Creating Layouts](./layouts.md) - Design custom window arrangements
- [Highlight Patterns](./highlights.md) - Color, filter, and reroute game text
- [Keybind Actions](./keybinds.md) - Custom keyboard shortcuts
- [Sound Alerts](./sounds.md) - Audio notifications for game events
- [Themes](./themes.md) - Switch or build UI color themes

## Quick Start

### Custom Layout

1. `.menu` → Windows → Add Window (or `.addwindow`)
2. Ctrl+drag to move windows, drag borders to resize
3. `.savelayout myname` to save

### Add a Highlight

`.addhighlight` in-app, or edit `~/.vellum-fe/global/highlights.toml`:

```toml
[death]
pattern = "appears dead"
fg = "#00ff00"
bold = true
sound = "kill.wav"
```

### Add a Keybind

`.addkeybind` in-app, or edit `~/.vellum-fe/global/keybinds.toml`:

```toml
[user]
f2 = { macro_text = "stance offensive\r" }
f3 = { macro_text = "stance defensive\r" }
```

### Switch Theme

```
.themes
```

Apply file edits without restarting: `.reload`
