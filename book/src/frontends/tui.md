# Terminal (TUI)

The default frontend. Runs in any modern terminal:

```bash
vellum-fe --port 8000 --character YourName
```

Most of this book describes the TUI: its [widgets](../widgets/README.md)
are laid out on a character grid via
[layout.toml](../configuration/layout-toml.md), and everything is
keyboard-driven with mouse support for links, scrolling, selection, and
window dragging.

## Terminal Recommendations

- Use a terminal with 24-bit color (Windows Terminal, kitty, alacritty,
  WezTerm, iTerm2) and leave `color_mode = "direct"`.
- On terminals limited to 256 colors, set `color_mode = "slot"` and run
  `.setpalette`, or use `"indexed"` as a safe fallback.
- Use a Nerd Font if you want the default countdown glyphs and compass to
  render perfectly.

## TUI-Only Features

A few things only make sense in a terminal:

- `.setpalette` / `.resetpalette` — reprogram the terminal's color palette
- `.transparent` — toggle transparent window backgrounds
- `.resize` — refit the layout to the current terminal size
